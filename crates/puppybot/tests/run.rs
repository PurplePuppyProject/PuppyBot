//! Tests of running the bot, each in a process of its own, as Docker runs it: the configuration
//! and the token file, the log, the exit statuses, and stopping on a signal.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Clippy counts only the #[test] functions as tests, not their helpers in this file"
)]

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

/// The token in the tests' token files, which the log must never show.
const TOKEN: &str = "MTIz.test-token_for.the-tests";

/// The smallest configuration, with the token file next to it.
const CONFIG: &str = "[discord]\ntoken-file = \"token\"\n";

/// How long a test waits for the program, before it gives up.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A directory with the configuration `config.toml` and, if given, the token file `token` with
/// the permissions `mode`.
fn setup(config: &str, token: Option<(&str, u32)>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), config).unwrap();
    if let Some((contents, mode)) = token {
        let path = dir.path().join("token");
        fs::write(&path, contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }
    dir
}

/// The program, with the configuration in `dir`, and `RUST_LOG` only if given.
fn command(dir: &Path, rust_log: Option<&OsStr>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_puppybot"));
    command
        .arg("--config")
        .arg(dir.join("config.toml"))
        .env_remove("RUST_LOG")
        .stdin(Stdio::null());
    if let Some(rust_log) = rust_log {
        command.env("RUST_LOG", rust_log);
    }
    command
}

/// Runs the program until it exits by itself, and returns its exit status and its errors or log.
fn run_to_end(dir: &Path, rust_log: Option<&OsStr>) -> (Option<i32>, String) {
    let Output { status, stderr, .. } = command(dir, rust_log).output().unwrap();
    (status.code(), String::from_utf8(stderr).unwrap())
}

/// The program while it runs, with what it logged so far.
struct Running {
    child: Child,
    lines: Receiver<String>,
    log: Vec<String>,
}

impl Running {
    fn start(dir: &Path, rust_log: Option<&OsStr>) -> Self {
        let mut child = command(dir, rust_log)
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
    fn wait_for(&mut self, text: &str) {
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

    /// Sends `signal`, e.g. `TERM`, waits until the program exits, and returns its exit status
    /// and the whole log.
    fn stop_with(mut self, signal: &str) -> (Option<i32>, String) {
        let pid = self.child.id().to_string();
        let sent = Command::new("kill")
            .args(["-s", signal, &pid])
            .status()
            .unwrap();
        assert!(sent.success(), "kill -s {signal} {pid}: {sent}");
        loop {
            match self.lines.recv_timeout(TIMEOUT) {
                Ok(line) => self.log.push(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let _ = self.child.kill();
                    panic!("still running after SIG{signal}, the log: {:#?}", self.log);
                }
            }
        }
        let status = self.child.wait().unwrap();
        (status.code(), self.log.join("\n"))
    }
}

#[test]
fn stops_on_sigterm() {
    let dir = setup(CONFIG, Some((TOKEN, 0o600)));
    let mut bot = Running::start(dir.path(), None);
    bot.wait_for("PuppyBot started");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(log.contains(" INFO "), "{log}");
    assert!(log.contains("PuppyBot stopped"), "{log}");
    assert!(log.contains("signal=\"SIGTERM\""), "{log}");
    assert!(!log.contains(TOKEN), "{log}");
    // Only owner may read the token file: no warning.
    assert!(!log.contains("WARN"), "{log}");
}

#[test]
fn stops_on_sigint_and_logs_json() {
    let config = format!("{CONFIG}[log]\nformat = \"json\"\n");
    let dir = setup(&config, Some((TOKEN, 0o400)));
    let mut bot = Running::start(dir.path(), None);
    bot.wait_for("PuppyBot started");
    let (status, log) = bot.stop_with("INT");
    assert_eq!(status, Some(0), "{log}");
    for line in log.lines() {
        assert!(line.starts_with('{') && line.ends_with('}'), "{line}");
    }
    assert!(log.contains("\"level\":\"INFO\""), "{log}");
    assert!(log.contains("\"message\":\"PuppyBot stopped\""), "{log}");
    assert!(log.contains("\"signal\":\"SIGINT\""), "{log}");
    assert!(!log.contains(TOKEN), "{log}");
}

#[test]
fn rust_log_takes_precedence_over_the_level() {
    let config = format!("{CONFIG}[log]\nlevel = \"error\"\n");
    let dir = setup(&config, Some((TOKEN, 0o600)));
    let mut bot = Running::start(dir.path(), Some(OsStr::new("info")));
    bot.wait_for("PuppyBot started");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
}

#[test]
fn warns_of_a_token_file_that_others_can_read() {
    let dir = setup(CONFIG, Some((TOKEN, 0o644)));
    let mut bot = Running::start(dir.path(), None);
    bot.wait_for("PuppyBot started");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(log.contains(" WARN "), "{log}");
    assert!(
        log.contains("can be read or changed by other users (mode 644)"),
        "{log}"
    );
}

#[test]
fn the_level_of_the_configuration_holds_back_details() {
    // The warning about the token file is held back at the level error, the error isn't.
    let config = format!("{CONFIG}[log]\nlevel = \"error\"\n");
    let dir = setup(&config, Some(("two words", 0o644)));
    let (status, log) = run_to_end(dir.path(), None);
    assert_eq!(status, Some(1), "{log}");
    assert!(!log.contains("WARN"), "{log}");
    assert!(log.contains(" ERROR "), "{log}");
}

#[test]
fn a_token_file_that_is_missing() {
    let dir = setup(CONFIG, None);
    let (status, log) = run_to_end(dir.path(), None);
    assert_eq!(status, Some(1), "{log}");
    let path = dir.path().join("token");
    assert!(
        log.contains(&format!(
            "PuppyBot can't run: can't read the Discord token from {}: ",
            path.display()
        )),
        "{log}"
    );
}

#[test]
fn a_token_file_that_holds_no_valid_token() {
    let dir = setup(CONFIG, Some(("Bot two words\n", 0o600)));
    let (status, log) = run_to_end(dir.path(), None);
    assert_eq!(status, Some(1), "{log}");
    assert!(log.contains("isn't printable ASCII"), "{log}");
    assert!(!log.contains("two words"), "{log}");
}

#[test]
fn a_configuration_that_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let (status, err) = run_to_end(dir.path(), None);
    assert_eq!(status, Some(2), "{err}");
    let path = dir.path().join("config.toml");
    assert!(
        err.starts_with(&format!(
            "puppybot: can't read the configuration {}: ",
            path.display()
        )),
        "{err}"
    );
}

#[test]
fn a_configuration_with_a_mistake_names_its_place() {
    let dir = setup("[discord]\ntokn-file = \"token\"\n", None);
    let (status, err) = run_to_end(dir.path(), None);
    assert_eq!(status, Some(2), "{err}");
    assert!(
        err.contains("is invalid: TOML parse error at line 2"),
        "{err}"
    );
    assert!(err.contains("unknown field `tokn-file`"), "{err}");
}

#[test]
fn rust_log_that_is_invalid() {
    let dir = setup(CONFIG, Some((TOKEN, 0o600)));
    let (status, err) = run_to_end(dir.path(), Some(OsStr::new("puppybot=loud")));
    assert_eq!(status, Some(2), "{err}");
    assert!(
        err.starts_with("puppybot: RUST_LOG isn't a valid filter: "),
        "{err}"
    );

    let (status, err) = run_to_end(dir.path(), Some(OsStr::from_bytes(b"inf\xff")));
    assert_eq!(status, Some(2), "{err}");
    assert_eq!(err, "puppybot: RUST_LOG isn't UTF-8\n");
}
