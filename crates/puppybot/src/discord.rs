//! The connection to Discord: the gateway, from which the events come, and the HTTP API, with
//! which PuppyBot answers them.

use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use twilight_gateway::{
    CloseFrame, ConfigBuilder, Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _,
};
use twilight_http::Client;
use twilight_model::application::interaction::Interaction;
use twilight_model::channel::message::AllowedMentions;
use twilight_model::gateway::payload::incoming::Ready;
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;

use crate::commands;
use crate::config::DiscordConfig;
use crate::shutdown::{Signals, Stop};

/// What PuppyBot gets from the gateway: events of its servers, for which Discord asks no
/// permission. Slash commands come without an intent.
const INTENTS: Intents = Intents::GUILDS;

/// The events that PuppyBot handles. The others aren't even parsed; the closes of the
/// connection always come.
const EVENTS: EventTypeFlags = EventTypeFlags::READY
    .union(EventTypeFlags::GUILD_CREATE)
    .union(EventTypeFlags::INTERACTION_CREATE);

/// How long PuppyBot waits for Discord to close the connection after a signal.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Why PuppyBot can't connect to Discord, or stopped being connected.
#[derive(Debug)]
pub enum Error {
    /// The system has no CA certificates, which the connections check Discord's with.
    NoCertificates(String),
    /// Discord closed the connection for good, e.g. as it rejected the token, so connecting
    /// again wouldn't help.
    Closed(Option<CloseFrame<'static>>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCertificates(errors) => write!(
                f,
                "found no CA certificates to check Discord's certificates with: {errors}"
            ),
            Self::Closed(frame) => {
                f.write_str("Discord closed the connection for good")?;
                frame.as_ref().map_or(Ok(()), |frame| {
                    write!(f, " with the code {}: {}", frame.code, frame.reason)
                })
            }
        }
    }
}

impl std::error::Error for Error {}

/// Makes TLS use ring, and checks that the system has CA certificates, without which twilight
/// would panic.
///
/// # Errors
///
/// [`Error::NoCertificates`] if there are none, e.g. as `SSL_CERT_FILE` names a missing file.
pub fn prepare_tls() -> Result<(), Error> {
    // This fails only if a provider was installed before, which then stays.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let found = rustls_native_certs::load_native_certs();
    if found.certs.is_empty() {
        let errors: Vec<String> = found.errors.iter().map(ToString::to_string).collect();
        return Err(Error::NoCertificates(errors.join("; ")));
    }
    Ok(())
}

/// PuppyBot on Discord: the connection, and what it does with the events.
pub struct Discord {
    shard: Shard,
    bot: Bot,
}

impl Discord {
    /// The connection with `token`, as `config` says, after [`prepare_tls`]. It connects once
    /// [`Self::run`] runs.
    #[must_use]
    pub fn new(config: &DiscordConfig, token: &SecretString) -> Self {
        let token = token.expose_secret().to_owned();
        let mut http = Client::builder()
            .token(token.clone())
            .default_allowed_mentions(AllowedMentions::default());
        if let Some(url) = &config.api_url {
            // http:// and an address on this machine, as the configuration checked.
            http = http.proxy(url.trim_start_matches("http://").to_owned(), true);
        }
        let mut gateway = ConfigBuilder::new(token, INTENTS);
        if let Some(url) = &config.gateway_url {
            gateway = gateway.proxy_url(url.clone());
        }
        Self {
            shard: Shard::with_config(ShardId::ONE, gateway.build()),
            bot: Bot {
                http: http.build(),
                guilds: config.guilds.iter().map(|&guild| Id::from(guild)).collect(),
            },
        }
    }

    /// Handles the events of Discord until a signal stops PuppyBot, and then closes the
    /// connection. On other closes, the connection connects again by itself.
    ///
    /// # Errors
    ///
    /// [`Error::Closed`] if Discord closed the connection for good.
    pub async fn run(mut self, signals: &mut Signals) -> Result<Stop, Error> {
        let mut last_close = None;
        let stop = loop {
            let item = tokio::select! {
                stop = signals.recv() => break stop,
                item = self.shard.next_event(EVENTS) => item,
            };
            match item {
                Some(Ok(Event::GatewayClose(frame))) => last_close = frame,
                Some(Ok(event)) => self.bot.handle(event).await,
                Some(Err(error)) => tracing::warn!("can't read an event of Discord: {error}"),
                // The shard may not be polled any more.
                None => return Err(Error::Closed(last_close)),
            }
        };
        tracing::info!(signal = stop.name(), "closing the connection to Discord");
        self.shard.close(CloseFrame::NORMAL);
        // Until Discord closed it, too, but not for long. No events are wanted any more, so the
        // next that comes is Discord's close, unless something fails.
        let _ = tokio::time::timeout(
            CLOSE_TIMEOUT,
            self.shard.next_event(EventTypeFlags::empty()),
        )
        .await;
        Ok(stop)
    }
}

/// What PuppyBot does with the events of Discord.
struct Bot {
    http: Client,
    /// The servers of PuppyBot, from the configuration.
    guilds: BTreeSet<Id<GuildMarker>>,
}

impl Bot {
    async fn handle(&self, event: Event) {
        match event {
            Event::Ready(ready) => self.ready(&ready).await,
            Event::GuildCreate(guild) => self.joined(guild.id()).await,
            Event::InteractionCreate(interaction) => self.interaction(&interaction.0).await,
            // Not wanted, see EVENTS.
            _ => {}
        }
    }

    /// Connected: registers the commands on each of PuppyBot's servers, and only there.
    async fn ready(&self, ready: &Ready) {
        tracing::info!(user = %ready.user.name, "connected to Discord");
        let client = self.http.interaction(ready.application.id);
        if let Err(error) = client.set_global_commands(&[]).await {
            tracing::warn!("can't remove the global commands: {error}");
        }
        let commands = commands::commands();
        for &guild in &self.guilds {
            match client.set_guild_commands(guild, &commands).await {
                Ok(_) => tracing::info!(server = %guild, "registered the commands"),
                Err(error) => {
                    tracing::warn!(server = %guild, "can't register the commands: {error}");
                }
            }
        }
    }

    /// A server, when PuppyBot connects or joins it: it leaves those that aren't its own.
    async fn joined(&self, guild: Id<GuildMarker>) {
        if self.guilds.contains(&guild) {
            return;
        }
        tracing::warn!(
            server = %guild,
            "leaving a server that isn't in [discord] guilds"
        );
        if let Err(error) = self.http.leave_guild(guild).await {
            tracing::warn!(server = %guild, "can't leave the server: {error}");
        }
    }

    /// A slash command, or another interaction: answers the slash commands.
    async fn interaction(&self, interaction: &Interaction) {
        let Some(response) = commands::answer(interaction, &self.guilds) else {
            return;
        };
        let answer = self.http.interaction(interaction.application_id);
        if let Err(error) = answer
            .create_response(interaction.id, &interaction.token, &response)
            .await
        {
            tracing::warn!("can't answer a command: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use twilight_gateway::CloseFrame;
    use twilight_model::gateway::payload::incoming::{GuildCreate, InteractionCreate};
    use twilight_model::guild::UnavailableGuild;

    use super::*;
    use crate::commands::tests::{GUILD, interaction};

    #[test]
    fn errors_say_why() {
        assert_eq!(
            Error::Closed(Some(CloseFrame::new(4004, "Authentication failed."))).to_string(),
            "Discord closed the connection for good with the code 4004: Authentication failed."
        );
        assert_eq!(
            Error::Closed(None).to_string(),
            "Discord closed the connection for good"
        );
    }

    /// Each event, with an API that fails every request, as nothing listens on its port:
    /// warnings, which the tests of the program check. They cover only the events that it
    /// wants, as twilight delivers no others; this covers both.
    #[test]
    fn every_event_with_an_api_that_fails() {
        prepare_tls().unwrap();
        // The client starts its rate limiter in the runtime, as Discord::new does.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let bot = Bot {
                    http: Client::builder()
                        .token("token".to_owned())
                        .proxy("127.0.0.1:1".to_owned(), true)
                        .build(),
                    guilds: BTreeSet::from([Id::new(GUILD)]),
                };
                let ready = serde_json::from_str(
                    r#"{"v": 10, "session_id": "session", "resume_gateway_url": "ws://127.0.0.1:1",
                    "shard": [0, 1], "guilds": [], "application": {"id": "2", "flags": 0},
                    "user": {"id": "3", "username": "PuppyBot", "discriminator": "0",
                    "avatar": null, "bot": true, "mfa_enabled": false}}"#,
                )
                .unwrap();
                bot.handle(Event::Ready(ready)).await;
                let other = UnavailableGuild {
                    id: Id::new(GUILD + 1),
                    unavailable: true,
                };
                bot.handle(Event::GuildCreate(Box::new(GuildCreate::Unavailable(
                    other,
                ))))
                .await;
                let interaction = interaction(2, "info", "de", Some(GUILD));
                bot.handle(Event::InteractionCreate(Box::new(InteractionCreate(
                    interaction,
                ))))
                .await;
                // Not wanted, see EVENTS.
                bot.handle(Event::Resumed).await;
            });
    }
}
