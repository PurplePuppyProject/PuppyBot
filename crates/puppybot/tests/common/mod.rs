//! What the tests of running PuppyBot share: the program while it runs, and Discord on this
//! machine, a gateway and an HTTP API that speak enough of Discord's protocols for PuppyBot.

#![allow(dead_code, reason = "each test file uses part of it")]

use std::convert::Infallible;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio_websockets::{CloseCode, Message, ServerBuilder};

/// The token in the tests' token files, which the log must never show.
pub const TOKEN: &str = "MTIz.test-token_for.the-tests";

/// The server of PuppyBot in the tests' configurations.
pub const GUILD: u64 = 1_100_000_000_000_000_001;

/// A server that isn't PuppyBot's.
pub const OTHER_GUILD: u64 = 1_100_000_000_000_000_009;

/// The application of the bot, as the gateway says.
pub const APPLICATION: u64 = 1_200_000_000_000_000_002;

/// The database of the tests, in the directory of the configuration.
pub const DATABASE: &str = "puppybot.redb";

/// The permission "Manage Server", as Discord sends permissions: a number.
pub const MANAGE_GUILD: u64 = 1 << 5;

/// How long a test waits for the program or Discord, before it gives up.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The configuration of the tests: the token file `token`, the server [`GUILD`], Discord on
/// this machine if given, and `extra`, e.g. a table `[log]`.
pub fn config(discord: Option<&FakeDiscord>, extra: &str) -> String {
    let urls = discord.map_or_else(String::new, |discord| {
        format!(
            "gateway-url = \"{}\"\napi-url = \"{}\"\n",
            discord.gateway_url, discord.api_url
        )
    });
    format!(
        "[discord]\ntoken-file = \"token\"\nguilds = [{GUILD}]\n{urls}[storage]\npath = \"{DATABASE}\"\n{extra}"
    )
}

/// A directory with the configuration `config.toml` and, if given, the token file `token` with
/// the permissions `mode`.
pub fn setup(config: &str, token: Option<(&str, u32)>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), config).unwrap();
    if let Some((contents, mode)) = token {
        let path = dir.path().join("token");
        fs::write(&path, contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }
    dir
}

/// The program, with the configuration in `dir`, and the environment variables `envs`, but
/// none of `RUST_LOG`, `SSL_CERT_FILE` and `SSL_CERT_DIR` of the tests' own environment.
fn command(dir: &Path, envs: &[(&str, &OsStr)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_puppybot"));
    command
        .arg("--config")
        .arg(dir.join("config.toml"))
        .env_remove("RUST_LOG")
        .env_remove("SSL_CERT_FILE")
        .env_remove("SSL_CERT_DIR")
        .envs(envs.iter().copied())
        .stdin(Stdio::null());
    command
}

/// Runs the program until it exits by itself, and returns its exit status and its errors or
/// log, as lines. Gives up as [`Running::wait`] does, if it doesn't exit.
pub fn run_to_end(dir: &Path, envs: &[(&str, &OsStr)]) -> (Option<i32>, String) {
    Running::start(dir, envs).wait()
}

/// The program while it runs, with what it logged so far.
pub struct Running {
    child: Child,
    lines: Receiver<String>,
    log: Vec<String>,
}

impl Running {
    pub fn start(dir: &Path, envs: &[(&str, &OsStr)]) -> Self {
        let mut child = command(dir, envs)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                if line.map(|line| sender.send(line)).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            log: Vec::new(),
        }
    }

    /// Waits until the log has a line with `text`.
    pub fn wait_for(&mut self, text: &str) {
        loop {
            match self.lines.recv_timeout(TIMEOUT) {
                Ok(line) => {
                    let found = line.contains(text);
                    self.log.push(line);
                    if found {
                        return;
                    }
                }
                Err(error) => {
                    let _ = self.child.kill();
                    panic!(
                        "no line with {text:?} ({error:?}), the log: {:#?}",
                        self.log
                    );
                }
            }
        }
    }

    /// Sends `signal`, e.g. `TERM`, and returns what [`Self::wait`] does.
    pub fn stop_with(self, signal: &str) -> (Option<i32>, String) {
        let pid = self.child.id().to_string();
        let sent = Command::new("kill")
            .args(["-s", signal, &pid])
            .status()
            .unwrap();
        assert!(sent.success(), "kill -s {signal} {pid}: {sent}");
        self.wait()
    }

    /// Waits until the program exits, and returns its exit status and the whole log.
    pub fn wait(mut self) -> (Option<i32>, String) {
        loop {
            match self.lines.recv_timeout(TIMEOUT) {
                Ok(line) => self.log.push(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let _ = self.child.kill();
                    panic!("still running, the log: {:#?}", self.log);
                }
            }
        }
        let status = self.child.wait().unwrap();
        (status.code(), self.log.join("\n"))
    }
}

/// What the gateway sends after PuppyBot identified, in this order.
#[derive(Debug, Clone)]
pub enum Send {
    /// An event, e.g. `READY`, with its data as JSON.
    Dispatch(&'static str, String),
    /// A message as it is, e.g. one that isn't valid.
    Text(String),
    /// Closes the connection with this code and reason.
    Close(u16, &'static str),
    /// Stops reading, so the gateway ignores PuppyBot's close, too.
    Silence,
}

/// The event `READY`, as Discord sends it once PuppyBot identified.
pub fn ready(gateway_url: &str) -> Send {
    Send::Dispatch(
        "READY",
        format!(
            r#"{{"v": 10, "session_id": "session", "resume_gateway_url": "{gateway_url}",
            "shard": [0, 1], "guilds": [], "application": {{"id": "{APPLICATION}", "flags": 0}},
            "user": {{"id": "1200000000000000003", "username": "PuppyBot", "discriminator": "0",
            "avatar": null, "bot": true, "mfa_enabled": false}}}}"#
        ),
    )
}

/// The event `GUILD_CREATE` of a server, in the short form of an unavailable one.
pub fn guild_create(guild: u64) -> Send {
    Send::Dispatch(
        "GUILD_CREATE",
        format!(r#"{{"id": "{guild}", "unavailable": true}}"#),
    )
}

/// /settings `subcommand` on [`GUILD`], with `to` if given, by someone with the locale
/// `locale` and the permissions `permissions`. PuppyBot answers it at [`SETTINGS_ANSWER`].
pub fn settings(subcommand: &str, to: Option<&str>, locale: &str, permissions: u64) -> Send {
    let to = to.map_or_else(String::new, |value| {
        format!(r#"{{"name": "to", "type": 3, "value": "{value}"}}"#)
    });
    Send::Dispatch(
        "INTERACTION_CREATE",
        format!(
            r#"{{"id": "1200000000000000006", "application_id": "{APPLICATION}", "type": 2,
            "token": "settings-token", "version": 1, "locale": "{locale}",
            "guild_id": "{GUILD}", "attachment_size_limit": 26214400,
            "authorizing_integration_owners": {{}}, "entitlements": [],
            "member": {{"deaf": false, "mute": false, "flags": 0, "roles": [], "joined_at": null,
                "permissions": "{permissions}", "user": {{"id": "1400000000000000005",
                "username": "puppy", "discriminator": "0", "avatar": null}}}},
            "data": {{"id": "1200000000000000007", "name": "settings", "type": 1,
                "options": [{{"name": "{subcommand}", "type": 1, "options": [{to}]}}]}}}}"#
        ),
    )
}

/// Where PuppyBot answers [`settings`].
pub const SETTINGS_ANSWER: &str =
    "POST /api/v10/interactions/1200000000000000006/settings-token/callback";

/// The channel of the messages of the tests, see [`message`].
pub const CHANNEL: u64 = 1_300_000_000_000_000_004;

/// Where PuppyBot writes to [`CHANNEL`], e.g. its answers to text commands.
pub const CHANNEL_MESSAGES: &str = "POST /api/v10/channels/1300000000000000004/messages";

/// How messages mention PuppyBot, the user of [`ready`].
pub const MENTION: &str = "<@1200000000000000003>";

/// The event `MESSAGE_CREATE` of the message `id` with `content` in [`CHANNEL`] on the server
/// `guild`, by a person, or by a bot.
pub fn message(id: u64, content: &str, guild: u64, bot: bool) -> Send {
    let content = serde_json::to_string(content).unwrap();
    Send::Dispatch(
        "MESSAGE_CREATE",
        format!(
            r#"{{"id": "{id}", "channel_id": "{CHANNEL}", "guild_id": "{guild}",
            "content": {content}, "type": 0, "flags": 0,
            "author": {{"id": "1400000000000000005", "username": "puppy", "discriminator": "0",
                "avatar": null, "bot": {bot}}},
            "member": {{"deaf": false, "mute": false, "flags": 0, "roles": [],
                "joined_at": "2026-01-01T00:00:00.000000+00:00"}},
            "timestamp": "2026-11-08T12:00:00.000000+00:00", "edited_timestamp": null,
            "tts": false, "mention_everyone": false, "mentions": [], "mention_roles": [],
            "attachments": [], "embeds": [], "components": [], "pinned": false}}"#
        ),
    )
}

/// The event `INTERACTION_CREATE` of an interaction of the type `kind` with the command
/// `name`, e.g. a slash command, of 2, by someone with `locale`, on the server `guild`.
pub fn interaction(kind: u8, name: &str, locale: &str, guild: u64) -> Send {
    let data = if kind == 3 {
        r#"{"custom_id": "button", "component_type": 2}"#.to_owned()
    } else {
        format!(r#"{{"id": "1200000000000000004", "name": "{name}", "type": 1}}"#)
    };
    Send::Dispatch(
        "INTERACTION_CREATE",
        format!(
            r#"{{"id": "1200000000000000005", "application_id": "{APPLICATION}", "type": {kind},
            "token": "interaction-token", "version": 1, "locale": "{locale}",
            "guild_id": "{guild}", "attachment_size_limit": 26214400,
            "authorizing_integration_owners": {{}}, "entitlements": [], "data": {data}}}"#
        ),
    )
}

/// A request that the HTTP API got.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

/// Discord on this machine: a gateway that sends the same script after each IDENTIFY, and an
/// HTTP API that records the requests and answers them all with `status`.
pub struct FakeDiscord {
    runtime: tokio::runtime::Runtime,
    pub gateway_url: String,
    pub api_url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    /// What PuppyBot sent to the gateway, e.g. IDENTIFY and its close.
    received: Arc<Mutex<Vec<String>>>,
}

impl FakeDiscord {
    /// Starts the gateway, which sends what `script` returns for the URL of the gateway, and the
    /// API, which answers with 2xx, or with `failing`, e.g. 400, if given.
    pub fn start(script: impl FnOnce(&str) -> Vec<Send>, failing: Option<u16>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (gateway, api) = runtime.block_on(async {
            (
                TcpListener::bind("127.0.0.1:0").await.unwrap(),
                TcpListener::bind("127.0.0.1:0").await.unwrap(),
            )
        });
        let gateway_url = format!("ws://{}", gateway.local_addr().unwrap());
        let api_url = format!("http://{}", api.local_addr().unwrap());
        let script = Arc::new(script(&gateway_url));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = Arc::new(Mutex::new(Vec::new()));

        let gateway_received = Arc::clone(&received);
        runtime.spawn(async move {
            while let Ok((stream, _)) = gateway.accept().await {
                tokio::spawn(connection(
                    stream,
                    Arc::clone(&script),
                    Arc::clone(&gateway_received),
                ));
            }
        });
        let api_requests = Arc::clone(&requests);
        runtime.spawn(async move {
            while let Ok((stream, _)) = api.accept().await {
                let requests = Arc::clone(&api_requests);
                tokio::spawn(async move {
                    let service =
                        service_fn(move |request| answer(request, Arc::clone(&requests), failing));
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Self {
            runtime,
            gateway_url,
            api_url,
            requests,
            received,
        }
    }

    /// Waits until the API got a request whose method and path start with `start`, e.g.
    /// `"POST /api/v10/interactions/"`, and returns it.
    pub fn wait_for_request(&self, start: &str) -> Request {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(request) =
                self.requests.lock().unwrap().iter().find(|request| {
                    format!("{} {}", request.method, request.path).starts_with(start)
                })
            {
                return request.clone();
            }
            assert!(
                Instant::now() < deadline,
                "no request {start:?}, the requests: {:#?}",
                self.requests.lock().unwrap()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// The requests that the API got so far.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    /// What PuppyBot sent to the gateway so far.
    pub fn received(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}

/// One connection to the gateway: HELLO, waiting for IDENTIFY, the script, and then answering
/// heartbeats until PuppyBot closes the connection.
async fn connection(stream: TcpStream, script: Arc<Vec<Send>>, received: Arc<Mutex<Vec<String>>>) {
    let Ok((_, mut socket)) = ServerBuilder::new().accept(stream).await else {
        return;
    };
    let hello = r#"{"op": 10, "d": {"heartbeat_interval": 45000}}"#;
    if socket.send(Message::text(hello)).await.is_err() {
        return;
    }
    let heartbeat_ack = r#"{"op": 11}"#;
    loop {
        let Some(Ok(message)) = socket.next().await else {
            return;
        };
        let Some(text) = message.as_text() else {
            continue;
        };
        received.lock().unwrap().push(text.to_owned());
        if text.contains(r#""op":2"#) {
            break;
        }
        if text.contains(r#""op":1"#) {
            let _ = socket.send(Message::text(heartbeat_ack)).await;
        }
    }
    let mut sequence = 0;
    for step in script.iter() {
        let message = match step {
            Send::Dispatch(kind, data) => {
                sequence += 1;
                Message::text(format!(
                    r#"{{"op": 0, "s": {sequence}, "t": "{kind}", "d": {data}}}"#
                ))
            }
            Send::Text(text) => Message::text(text.clone()),
            Send::Close(code, reason) => Message::close(CloseCode::try_from(*code).ok(), reason),
            Send::Silence => {
                tokio::time::sleep(TIMEOUT).await;
                return;
            }
        };
        if socket.send(message).await.is_err() {
            return;
        }
    }
    while let Some(Ok(message)) = socket.next().await {
        if let Some((code, reason)) = message.as_close() {
            received
                .lock()
                .unwrap()
                .push(format!("close {} {reason}", u16::from(code)));
            break;
        }
        if message
            .as_text()
            .is_some_and(|text| text.contains(r#""op":1"#))
        {
            let _ = socket.send(Message::text(heartbeat_ack)).await;
        }
    }
    let _ = socket.close().await;
}

/// Records a request to the API, and answers it as Discord would, or with `failing`.
async fn answer(
    request: hyper::Request<Incoming>,
    requests: Arc<Mutex<Vec<Request>>>,
    failing: Option<u16>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let body = request
        .into_body()
        .collect()
        .await
        .map(|body| String::from_utf8_lossy(&body.to_bytes()).into_owned())
        .unwrap_or_default();
    let (status, reply) = match failing {
        Some(status) => (status, r#"{"message": "Invalid Form Body", "code": 50035}"#),
        None if path.ends_with("/commands") => (200, "[]"),
        None => (204, ""),
    };
    requests
        .lock()
        .unwrap()
        .push(Request { method, path, body });
    Ok(hyper::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(reply)))
        .unwrap())
}
