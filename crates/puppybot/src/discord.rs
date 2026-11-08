//! The connection to Discord: the gateway, from which the events come, and the HTTP API, with
//! which PuppyBot answers them.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use puppybot_core::i18n::Lang;
use puppybot_core::text::Text;
use puppybot_core::text_command::{self, TextCommand};
use secrecy::{ExposeSecret, SecretString};
use twilight_gateway::{
    CloseFrame, ConfigBuilder, Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _,
};
use twilight_http::Client;
use twilight_model::application::interaction::Interaction;
use twilight_model::channel::Message;
use twilight_model::channel::message::{AllowedMentions, MessageType};
use twilight_model::gateway::payload::incoming::Ready;
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;

use crate::commands::{self, Request};
use crate::config::DiscordConfig;
use crate::shutdown::{Signals, Stop};
use crate::storage::Settings;

/// What PuppyBot gets from the gateway: events of its servers, and the messages on them with
/// their text, for the text commands. Slash commands come without an intent. Discord counts
/// the text of messages, `MESSAGE_CONTENT`, as privileged: the Developer Portal must allow it,
/// or Discord closes the connection with the code 4014.
const INTENTS: Intents = Intents::GUILDS
    .union(Intents::GUILD_MESSAGES)
    .union(Intents::MESSAGE_CONTENT);

/// The events that PuppyBot handles. The others aren't even parsed; the closes of the
/// connection always come.
const EVENTS: EventTypeFlags = EventTypeFlags::READY
    .union(EventTypeFlags::GUILD_CREATE)
    .union(EventTypeFlags::INTERACTION_CREATE)
    .union(EventTypeFlags::MESSAGE_CREATE);

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
    /// The connection with `token`, as `config` says, after [`prepare_tls`], with the settings
    /// of the servers in `settings`. It connects once [`Self::run`] runs.
    #[must_use]
    pub fn new(config: &DiscordConfig, token: &SecretString, settings: Box<dyn Settings>) -> Self {
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
                settings,
                mentions: OnceCell::new(),
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
    /// The settings of the servers.
    settings: Box<dyn Settings>,
    /// How messages mention PuppyBot, once Discord said who it is.
    mentions: OnceCell<[String; 2]>,
}

impl Bot {
    async fn handle(&self, event: Event) {
        match event {
            Event::Ready(ready) => self.ready(&ready).await,
            Event::GuildCreate(guild) => self.joined(guild.id()).await,
            Event::InteractionCreate(interaction) => self.interaction(&interaction.0).await,
            Event::MessageCreate(message) => self.message(&message.message).await,
            // Not wanted, see EVENTS.
            _ => {}
        }
    }

    /// Connected: registers the commands on each of PuppyBot's servers, and only there.
    async fn ready(&self, ready: &Ready) {
        tracing::info!(user = %ready.user.name, "connected to Discord");
        // The same after connecting again.
        let id = ready.user.id;
        let _ = self.mentions.set([format!("<@{id}>"), format!("<@!{id}>")]);
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
        let Some(asked) = commands::request(interaction, &self.guilds) else {
            return;
        };
        let response = commands::response(self.execute(asked.request, asked.lang));
        let answer = self.http.interaction(interaction.application_id);
        if let Err(error) = answer
            .create_response(interaction.id, &interaction.token, &response)
            .await
        {
            tracing::warn!("can't answer a command: {error}");
        }
    }

    /// Does what `request` asks for, and returns the answer, in `lang`.
    fn execute(&self, request: Request<'_>, lang: Lang) -> String {
        match request {
            Request::Answer(text) => text.render(lang),
            Request::ShowLanguage(guild) => match self.settings.language(guild) {
                Ok(chosen) => Text::LanguageIs { chosen }.render(lang),
                Err(error) => {
                    tracing::warn!(server = %guild, "can't read the language of the server from the database: {error}");
                    Text::StorageFailed.render(lang)
                }
            },
            Request::SetLanguage(guild, new) => match self.settings.set_language(guild, new) {
                Ok(()) => {
                    tracing::info!(
                        server = %guild,
                        language = new.code(),
                        "changed the language of the server"
                    );
                    Text::LanguageSet { lang: new }.render(lang)
                }
                Err(error) => {
                    tracing::warn!(server = %guild, "can't save the language of the server in the database: {error}");
                    Text::StorageFailed.render(lang)
                }
            },
            Request::ShowPrefix(guild) => match self.settings.prefix(guild) {
                Ok(chosen) => Text::PrefixIs {
                    chosen: chosen.as_deref(),
                }
                .render(lang),
                Err(error) => {
                    tracing::warn!(server = %guild, "can't read the prefix of the server from the database: {error}");
                    Text::StorageFailed.render(lang)
                }
            },
            Request::SetPrefix(guild, prefix) => match self.settings.set_prefix(guild, prefix) {
                Ok(()) => {
                    tracing::info!(server = %guild, prefix, "changed the prefix of the server");
                    Text::PrefixSet { prefix }.render(lang)
                }
                Err(error) => {
                    tracing::warn!(server = %guild, "can't save the prefix of the server in the database: {error}");
                    Text::StorageFailed.render(lang)
                }
            },
        }
    }

    /// A message: answers the text commands of people on PuppyBot's servers, in the language of
    /// the server, as their messages are for everyone in the channel.
    async fn message(&self, message: &Message) {
        // Not the messages of bots, which may answer each other, nor of webhooks, nor what
        // Discord writes, e.g. that someone joined.
        let ordinary = matches!(message.kind, MessageType::Regular | MessageType::Reply);
        if !ordinary || message.author.bot || message.webhook_id.is_some() {
            return;
        }
        let Some(guild) = message.guild_id.filter(|guild| self.guilds.contains(guild)) else {
            return;
        };
        let prefix = self.prefix(guild);
        let mentions = self.mentions.get().map_or(Vec::new(), |mentions| {
            mentions.iter().map(String::as_str).collect()
        });
        let Some((command, _)) = text_command::parse(&message.content, &prefix, &mentions) else {
            return;
        };
        let text = match command {
            TextCommand::Info => Text::Info {
                version: crate::VERSION,
            },
            TextCommand::Help => Text::Help { prefix: &prefix },
        };
        let content = text.render(self.language(guild));
        let nobody = AllowedMentions::default();
        let reply = self
            .http
            .create_message(message.channel_id)
            .content(&content)
            .reply(message.id)
            // Also if the message is gone by now.
            .fail_if_not_exists(false)
            .allowed_mentions(Some(&nobody));
        if let Err(error) = reply.await {
            tracing::warn!(server = %guild, "can't answer a text command: {error}");
        }
    }

    /// The prefix of the text commands on `guild`: the one chosen, or the default, also if the
    /// settings can't be read.
    fn prefix(&self, guild: Id<GuildMarker>) -> String {
        match self.settings.prefix(guild) {
            Ok(chosen) => chosen.unwrap_or_else(|| text_command::DEFAULT_PREFIX.to_owned()),
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the prefix of the server from the database: {error}");
                text_command::DEFAULT_PREFIX.to_owned()
            }
        }
    }

    /// The language of `guild`: the one chosen, or the default, also if the settings can't be
    /// read.
    fn language(&self, guild: Id<GuildMarker>) -> Lang {
        match self.settings.language(guild) {
            Ok(chosen) => chosen.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the language of the server from the database: {error}");
                Lang::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use twilight_gateway::CloseFrame;
    use twilight_model::gateway::payload::incoming::{
        GuildCreate, InteractionCreate, MessageCreate,
    };
    use twilight_model::guild::UnavailableGuild;

    use puppybot_core::i18n::Lang;

    use super::*;
    use crate::commands::tests::{GUILD, interaction};
    use crate::storage::{self, Storage};

    /// Settings that can be neither read nor saved, e.g. on a full disk.
    struct Broken;

    /// The error of [`Broken`].
    fn disk_full() -> storage::Error {
        storage::Error::Access(redb::Error::Io(io::Error::other("disk full")))
    }

    impl Settings for Broken {
        fn language(&self, _: Id<GuildMarker>) -> Result<Option<Lang>, storage::Error> {
            Err(disk_full())
        }

        fn set_language(&self, _: Id<GuildMarker>, _: Lang) -> Result<(), storage::Error> {
            Err(disk_full())
        }

        fn prefix(&self, _: Id<GuildMarker>) -> Result<Option<String>, storage::Error> {
            Err(disk_full())
        }

        fn set_prefix(&self, _: Id<GuildMarker>, _: &str) -> Result<(), storage::Error> {
            Err(disk_full())
        }
    }

    /// PuppyBot on the server [`GUILD`], with `settings`, and an API that fails every request,
    /// as nothing listens on its port. The client starts its rate limiter in the runtime, as
    /// `Discord::new` does, so this must run in one.
    fn bot(settings: Box<dyn Settings>) -> Bot {
        Bot {
            http: Client::builder()
                .token("token".to_owned())
                .proxy("127.0.0.1:1".to_owned(), true)
                .build(),
            guilds: BTreeSet::from([Id::new(GUILD)]),
            settings,
            mentions: OnceCell::new(),
        }
    }

    /// The message `content` on the server [`GUILD`], by a person, as Discord's gateway sends
    /// it.
    fn message(content: &str) -> Message {
        let content = serde_json::to_string(content).unwrap();
        serde_json::from_str(&format!(
            r#"{{"id": "1300000000000000010", "channel_id": "1300000000000000004",
            "guild_id": "{GUILD}", "content": {content}, "type": 0, "flags": 0,
            "author": {{"id": "1400000000000000005", "username": "puppy", "discriminator": "0",
                "avatar": null}},
            "timestamp": "2026-11-08T12:00:00.000000+00:00", "edited_timestamp": null,
            "tts": false, "mention_everyone": false, "mentions": [], "mention_roles": [],
            "attachments": [], "embeds": [], "components": [], "pinned": false}}"#
        ))
        .unwrap()
    }

    /// Runs `test` in a runtime like the program's, with a log at every level that goes
    /// nowhere: tracing builds only the messages that a log wants, and the tests reach them all.
    fn in_runtime(test: impl Future<Output = ()>) {
        prepare_tls().unwrap();
        let log = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(io::sink)
            .finish();
        tracing::subscriber::with_default(log, || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(test);
        });
    }

    #[test]
    fn the_language_of_a_server() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let en = |text: Text<'_>| text.render(Lang::En);
            assert_eq!(
                bot.execute(Request::ShowLanguage(guild), Lang::En),
                en(Text::LanguageIs { chosen: None })
            );
            assert_eq!(bot.language(guild), Lang::En);
            assert_eq!(
                bot.execute(Request::SetLanguage(guild, Lang::De), Lang::En),
                en(Text::LanguageSet { lang: Lang::De })
            );
            assert_eq!(
                bot.execute(Request::ShowLanguage(guild), Lang::En),
                en(Text::LanguageIs {
                    chosen: Some(Lang::De)
                })
            );
            assert_eq!(bot.language(guild), Lang::De);
            assert_eq!(
                bot.execute(Request::Answer(Text::NotHere), Lang::De),
                Text::NotHere.render(Lang::De)
            );
        });
    }

    #[test]
    fn the_prefix_of_a_server() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let en = |text: Text<'_>| text.render(Lang::En);
            assert_eq!(
                bot.execute(Request::ShowPrefix(guild), Lang::En),
                en(Text::PrefixIs { chosen: None })
            );
            assert_eq!(bot.prefix(guild), "!");
            assert_eq!(
                bot.execute(Request::SetPrefix(guild, "pb?"), Lang::En),
                en(Text::PrefixSet { prefix: "pb?" })
            );
            assert_eq!(
                bot.execute(Request::ShowPrefix(guild), Lang::En),
                en(Text::PrefixIs {
                    chosen: Some("pb?")
                })
            );
            assert_eq!(bot.prefix(guild), "pb?");
        });
    }

    #[test]
    fn settings_that_fail() {
        in_runtime(async {
            let bot = bot(Box::new(Broken));
            let guild = Id::new(GUILD);
            for request in [
                Request::ShowLanguage(guild),
                Request::SetLanguage(guild, Lang::En),
                Request::ShowPrefix(guild),
                Request::SetPrefix(guild, "?"),
            ] {
                assert_eq!(
                    bot.execute(request, Lang::De),
                    Text::StorageFailed.render(Lang::De),
                    "{request:?}"
                );
            }
            // Text commands still work, as by default.
            assert_eq!(bot.prefix(guild), "!");
            assert_eq!(bot.language(guild), Lang::En);
        });
    }

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

    /// Each event, with an API that fails every request: warnings, which the tests of the
    /// program check. They cover only the events that it wants, as twilight delivers no others;
    /// this covers both.
    #[test]
    fn every_event_with_an_api_that_fails() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
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
            let message = MessageCreate {
                channel_type: None,
                message: message("<@3> info"),
            };
            bot.handle(Event::MessageCreate(Box::new(message))).await;
            // Not wanted, see EVENTS.
            bot.handle(Event::Resumed).await;
        });
    }
}
