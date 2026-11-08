//! The connection to Discord: the gateway, from which the events come, and the HTTP API, with
//! which PuppyBot answers them.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::fmt;
use std::time::Duration;

use puppybot_core::dice::Dice;
use puppybot_core::games::{Coin, GameText, Guessing, Hand, Hint};
use puppybot_core::i18n::Lang;
use puppybot_core::poll::Poll;
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
use twilight_model::guild::Guild;
use twilight_model::id::Id;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, MessageMarker};
use twilight_model::util::Timestamp;

use crate::commands::{self, Answer, Audience, Request};
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
                die: crate::random::die,
                games: RefCell::new(BTreeMap::new()),
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
    /// A die with so many sides, see [`crate::random::die`], or another one in tests.
    die: fn(u32) -> Option<u32>,
    /// The games of guessing a number that run, by channel. A restart ends them.
    games: RefCell<BTreeMap<Id<ChannelMarker>, Guessing>>,
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
        let channel = interaction.channel.as_ref().map(|channel| channel.id);
        let response = commands::response(self.execute(asked.request, asked.lang, channel).await);
        let answer = self.http.interaction(interaction.application_id);
        if let Err(error) = answer
            .create_response(interaction.id, &interaction.token, &response)
            .await
        {
            tracing::warn!("can't answer a command: {error}");
        }
    }

    /// Does what `request` asks for, in `channel`, and returns the answer, in `lang`.
    async fn execute(
        &self,
        request: Request<'_>,
        lang: Lang,
        channel: Option<Id<ChannelMarker>>,
    ) -> Answer {
        let everyone = |content| Answer::new(content, Audience::Everyone);
        let content = match request {
            Request::Coin => return everyone(self.coin(lang)),
            Request::EightBall(question) => return everyone(self.eight_ball(question, lang)),
            Request::RpsChoose => {
                return Answer {
                    buttons: commands::rps_buttons(lang),
                    ..everyone(GameText::RpsChoose.render(lang))
                };
            }
            Request::RpsPlay { hand, player } => {
                return Answer {
                    update: true,
                    ..everyone(self.rps(hand, player, lang))
                };
            }
            Request::Guess {
                number,
                max,
                player,
            } => return everyone(self.guess(channel, number, max, player, lang)),
            Request::ServerInfo(guild) => server_text(self.server(guild).await, lang),
            Request::Poll(poll) => {
                let posted = match channel {
                    Some(channel) => self.post_poll(&poll, channel, None).await,
                    None => false,
                };
                Text::poll_posted(posted).render(lang)
            }
            Request::Roll { dice, secret } => {
                let audience = if secret {
                    Audience::Asker
                } else {
                    Audience::Everyone
                };
                return Answer::new(self.rolled(&dice, lang), audience);
            }
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
        };
        Answer::new(content, Audience::Asker)
    }

    /// Flips a coin, and returns the answer, in `lang`.
    fn coin(&self, lang: Lang) -> String {
        let mut die = self.die;
        Coin::flip(&mut die)
            .map_or(GameText::GameFailed, GameText::Coin)
            .render(lang)
    }

    /// Asks the magic 8-ball `question`, and returns its answer, in `lang`.
    fn eight_ball(&self, question: &str, lang: Lang) -> String {
        let mut die = self.die;
        puppybot_core::games::eight_ball(&mut die)
            .map_or(GameText::GameFailed, |answer| GameText::EightBall {
                question,
                answer,
            })
            .render(lang)
    }

    /// Plays `hand` of `player` against a hand of PuppyBot, and returns the outcome, in `lang`.
    fn rps(&self, hand: Hand, player: &str, lang: Lang) -> String {
        let mut die = self.die;
        Hand::choose(&mut die)
            .map_or(GameText::GameFailed, |bot| GameText::RpsPlayed {
                player,
                hand,
                bot,
            })
            .render(lang)
    }

    /// A guess of `number` by `player` in `channel`: without a number, a new game up to `max`, if
    /// none runs there. Returns the answer, in `lang`.
    fn guess(
        &self,
        channel: Option<Id<ChannelMarker>>,
        number: Option<u32>,
        max: Option<u32>,
        player: &str,
        lang: Lang,
    ) -> String {
        let Some(channel) = channel else {
            return GameText::GameFailed.render(lang);
        };
        let mut games = self.games.borrow_mut();
        let text = if let Some(game) = games.get_mut(&channel) {
            if let Some(number) = number {
                let hint = game.guess(number);
                let max = game.max();
                if matches!(hint, Hint::Right { .. }) {
                    games.remove(&channel);
                }
                GameText::Guessed {
                    player,
                    number,
                    hint,
                    max,
                }
            } else {
                GameText::GuessRunning { max: game.max() }
            }
        } else {
            let mut die = self.die;
            match Guessing::new(max.unwrap_or(Guessing::DEFAULT_MAX), &mut die) {
                Some(game) => {
                    games.insert(channel, game);
                    GameText::GuessStarted { max: game.max() }
                }
                None => GameText::GameFailed,
            }
        };
        text.render(lang)
    }

    /// The server `guild`, as Discord says, with about how many members it has.
    async fn server(&self, guild: Id<GuildMarker>) -> Result<Guild, Box<dyn StdError>> {
        Ok(self
            .http
            .guild(guild)
            .with_counts(true)
            .await?
            .model()
            .await?)
    }

    /// Posts `poll` in `channel`, as a reply to `reply_to` if given, and returns whether it
    /// worked. The body is PuppyBot's own: twilight's polls can't say how long they run.
    async fn post_poll(
        &self,
        poll: &Poll,
        channel: Id<ChannelMarker>,
        reply_to: Option<Id<MessageMarker>>,
    ) -> bool {
        let body = poll_message(poll, reply_to);
        match self.http.create_message(channel).payload_json(&body).await {
            Ok(_) => true,
            Err(error) => {
                tracing::warn!(channel = %channel, "can't post a poll: {error}");
                false
            }
        }
    }

    /// Rolls `dice`, and returns the answer, in `lang`.
    fn rolled(&self, dice: &Dice, lang: Lang) -> String {
        let mut side = self.die;
        if let Some(roll) = dice.roll(&mut side) {
            Text::Rolled {
                dice: &dice.to_string(),
                roll: &roll.to_string(),
            }
            .render(lang)
        } else {
            tracing::warn!("can't roll dice: the system has no randomness");
            Text::DiceFailed.render(lang)
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
        let Some((command, arguments)) = text_command::parse(&message.content, &prefix, &mentions)
        else {
            return;
        };
        let lang = self.language(guild);
        let content = match command {
            TextCommand::Info => Text::Info {
                version: crate::VERSION,
            }
            .render(lang),
            TextCommand::Help => Text::Help { prefix: &prefix }.render(lang),
            TextCommand::Dice => match Dice::parse_or_d6(arguments) {
                Ok(dice) => self.rolled(&dice, lang),
                Err(error) => Text::dice_error(error).render(lang),
            },
            TextCommand::Server => server_text(self.server(guild).await, lang),
            TextCommand::User => user_text(message, lang),
            TextCommand::Coin => self.coin(lang),
            TextCommand::EightBall => self.eight_ball(arguments, lang),
            TextCommand::Rps => GameText::RpsChoose.render(lang),
            TextCommand::Guess => {
                let author = &message.author;
                let player = author.global_name.as_deref().unwrap_or(&author.name);
                let number = arguments.parse().ok();
                self.guess(Some(message.channel_id), number, None, player, lang)
            }
            // The poll is the answer.
            TextCommand::Poll => match Poll::parse(arguments) {
                Ok(poll) => {
                    self.post_poll(&poll, message.channel_id, Some(message.id))
                        .await;
                    return;
                }
                Err(error) => Text::poll_error(error).render(lang),
            },
        };
        let nobody = AllowedMentions::default();
        let buttons = if command == TextCommand::Rps {
            commands::buttons(&commands::rps_buttons(lang))
        } else {
            Vec::new()
        };
        let reply = self
            .http
            .create_message(message.channel_id)
            .content(&content)
            .components(&buttons)
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

/// The answer to /server and the text command server, in `lang`, of what Discord said about
/// the server.
fn server_text(server: Result<Guild, Box<dyn StdError>>, lang: Lang) -> String {
    match server {
        Ok(server) => Text::ServerInfo {
            name: &server.name,
            created: commands::created(server.id.get()),
            members: server.approximate_member_count,
        }
        .render(lang),
        Err(error) => {
            tracing::warn!("can't get the details of the server: {error}");
            Text::InfoFailed.render(lang)
        }
    }
}

/// The answer to the text command user, in `lang`: since when the first person that `message`
/// mentions, but no bot, e.g. PuppyBot, or else its author, is on Discord and on the server.
fn user_text(message: &Message, lang: Lang) -> String {
    let (name, id, member) =
        if let Some(mention) = message.mentions.iter().find(|mention| !mention.bot) {
            (mention.name.as_str(), mention.id, mention.member.as_ref())
        } else {
            let author = &message.author;
            let name = author.global_name.as_deref().unwrap_or(&author.name);
            (name, author.id, message.member.as_ref())
        };
    Text::UserInfo {
        name,
        created: commands::created(id.get()),
        joined: member
            .and_then(|member| member.joined_at)
            .map(Timestamp::as_secs),
    }
    .render(lang)
}

/// The body of a message with `poll`, which mentions nobody, as a reply to `reply_to` if given.
fn poll_message(poll: &Poll, reply_to: Option<Id<MessageMarker>>) -> Vec<u8> {
    let answers: Vec<serde_json::Value> = poll
        .answers()
        .iter()
        .map(|answer| serde_json::json!({ "poll_media": { "text": answer } }))
        .collect();
    let poll = serde_json::json!({
        "question": { "text": poll.question() },
        "answers": answers,
        "duration": poll.hours(),
        "allow_multiselect": poll.multiple(),
        "layout_type": 1,
    });
    let nobody = serde_json::json!({ "parse": [] });
    let message = match reply_to {
        Some(message) => serde_json::json!({
            "poll": poll,
            "allowed_mentions": nobody,
            "message_reference": { "message_id": message.to_string(), "fail_if_not_exists": false },
        }),
        None => serde_json::json!({ "poll": poll, "allowed_mentions": nobody }),
    };
    serde_json::to_vec(&message).unwrap_or_default()
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
            // Every die at its most.
            die: Some,
            games: RefCell::new(BTreeMap::new()),
        }
    }

    /// The text of `answer`, and who sees it.
    fn said(answer: Answer) -> (String, Audience) {
        (answer.content, answer.audience)
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
            let en = |text: Text<'_>| (text.render(Lang::En), Audience::Asker);
            assert_eq!(
                said(
                    bot.execute(Request::ShowLanguage(guild), Lang::En, None)
                        .await
                ),
                en(Text::LanguageIs { chosen: None })
            );
            assert_eq!(bot.language(guild), Lang::En);
            assert_eq!(
                said(
                    bot.execute(Request::SetLanguage(guild, Lang::De), Lang::En, None)
                        .await
                ),
                en(Text::LanguageSet { lang: Lang::De })
            );
            assert_eq!(
                said(
                    bot.execute(Request::ShowLanguage(guild), Lang::En, None)
                        .await
                ),
                en(Text::LanguageIs {
                    chosen: Some(Lang::De)
                })
            );
            assert_eq!(bot.language(guild), Lang::De);
            assert_eq!(
                said(
                    bot.execute(Request::Answer(Text::NotHere), Lang::De, None)
                        .await
                ),
                (Text::NotHere.render(Lang::De), Audience::Asker)
            );
        });
    }

    #[test]
    fn the_prefix_of_a_server() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let en = |text: Text<'_>| (text.render(Lang::En), Audience::Asker);
            assert_eq!(
                said(
                    bot.execute(Request::ShowPrefix(guild), Lang::En, None)
                        .await
                ),
                en(Text::PrefixIs { chosen: None })
            );
            assert_eq!(bot.prefix(guild), "!");
            assert_eq!(
                said(
                    bot.execute(Request::SetPrefix(guild, "pb?"), Lang::En, None)
                        .await
                ),
                en(Text::PrefixSet { prefix: "pb?" })
            );
            assert_eq!(
                said(
                    bot.execute(Request::ShowPrefix(guild), Lang::En, None)
                        .await
                ),
                en(Text::PrefixIs {
                    chosen: Some("pb?")
                })
            );
            assert_eq!(bot.prefix(guild), "pb?");
        });
    }

    #[test]
    fn dice_for_everyone_or_secret() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let roll = |secret| Request::Roll {
                dice: Dice::parse("2d6+3").unwrap(),
                secret,
            };
            let rolled = "`2d6+3` gives [6, 6] + 3 = **15**".to_owned();
            assert_eq!(
                said(bot.execute(roll(false), Lang::En, None).await),
                (rolled.clone(), Audience::Everyone)
            );
            assert_eq!(
                said(bot.execute(roll(true), Lang::En, None).await),
                (rolled, Audience::Asker)
            );

            // Without randomness, no roll.
            let mut bot = bot;
            bot.die = |_| None;
            assert_eq!(
                said(bot.execute(roll(false), Lang::De, None).await),
                (Text::DiceFailed.render(Lang::De), Audience::Everyone)
            );
        });
    }

    #[test]
    fn polls_that_fail() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let poll = || Request::Poll(Poll::parse("Pizza? | Ja | Nein").unwrap());
            let failed = (Text::PollFailed.render(Lang::En), Audience::Asker);
            // Without a channel, and in one where the API fails.
            assert_eq!(said(bot.execute(poll(), Lang::En, None).await), failed);
            assert_eq!(
                said(bot.execute(poll(), Lang::En, Some(Id::new(4))).await),
                failed
            );
        });
    }

    /// A server, as Discord's API sends it with its counts.
    pub(crate) const SERVER: &str = r#"{"id": "1100000000000000001", "name": "Purple Puppies",
        "icon": null, "owner_id": "1400000000000000005", "afk_channel_id": null,
        "afk_timeout": 300, "verification_level": 0, "default_message_notifications": 0,
        "explicit_content_filter": 0, "roles": [], "emojis": [], "features": [],
        "mfa_level": 0, "application_id": null, "system_channel_id": null,
        "system_channel_flags": 0, "rules_channel_id": null, "vanity_url_code": null,
        "description": null, "banner": null, "premium_tier": 0, "preferred_locale": "de",
        "public_updates_channel_id": null, "nsfw_level": 0, "premium_progress_bar_enabled": false,
        "stickers": [], "approximate_member_count": 42, "approximate_presence_count": 7}"#;

    #[test]
    fn the_server_and_its_members() {
        let server = serde_json::from_str(SERVER).map_err(Into::into);
        assert_eq!(
            server_text(server, Lang::En),
            "**Purple Puppies** was created on <t:1682330837:D> (<t:1682330837:R>), and has about 42 members."
        );
        in_runtime(async {
            let failed = server_text(Err("no answer".into()), Lang::De);
            assert_eq!(failed, Text::InfoFailed.render(Lang::De));
            // With an API that fails.
            let bot = bot(Box::new(Storage::in_memory()));
            assert_eq!(
                said(
                    bot.execute(Request::ServerInfo(Id::new(GUILD)), Lang::En, None)
                        .await
                ),
                (Text::InfoFailed.render(Lang::En), Audience::Asker)
            );
        });
    }

    #[test]
    fn people_and_since_when_they_are_here() {
        let joined = r#""member": {"deaf": false, "mute": false, "flags": 0, "roles": [],
            "joined_at": "2026-01-01T00:00:00.000000+00:00"}"#;
        let mut message: Message = serde_json::from_str(&format!(
            r#"{{"id": "1300000000000000010", "channel_id": "1300000000000000004",
            "guild_id": "{GUILD}", "content": "!user", "type": 0, "flags": 0,
            "author": {{"id": "1400000000000000005", "username": "puppy", "discriminator": "0",
                "avatar": null, "global_name": "Puppy"}}, {joined},
            "timestamp": "2026-11-08T12:00:00.000000+00:00", "edited_timestamp": null,
            "tts": false, "mention_everyone": false, "mention_roles": [],
            "mentions": [{{"id": "3", "username": "PuppyBot", "discriminator": "0",
                "avatar": null, "bot": true, "public_flags": 0}}],
            "attachments": [], "embeds": [], "components": [], "pinned": false}}"#
        ))
        .unwrap();
        // The author, as PuppyBot is a bot.
        assert_eq!(
            user_text(&message, Lang::En),
            "**Puppy** is on Discord since <t:1753856410:D> (<t:1753856410:R>), and on this server since <t:1767225600:D>."
        );
        // Someone else.
        message.mentions = serde_json::from_str(&format!(
            r#"[{{"id": "1100000000000000001", "username": "rex", "discriminator": "0",
                "avatar": null, "bot": false, "public_flags": 0, {joined}}}]"#
        ))
        .unwrap();
        assert_eq!(
            user_text(&message, Lang::En),
            "**rex** is on Discord since <t:1682330837:D> (<t:1682330837:R>), and on this server since <t:1767225600:D>."
        );
        message.mentions.clear();
        message.member = None;
        assert!(user_text(&message, Lang::De).ends_with("auf Discord."));
    }

    #[test]
    fn games() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let everyone = |text: GameText<'_>| (text.render(Lang::En), Audience::Everyone);
            assert_eq!(
                said(bot.execute(Request::Coin, Lang::En, None).await),
                everyone(GameText::Coin(Coin::Tails))
            );
            assert_eq!(
                said(
                    bot.execute(Request::EightBall("Sonne?"), Lang::En, None)
                        .await
                ),
                everyone(GameText::EightBall {
                    question: "Sonne?",
                    answer: 19
                })
            );
            let choose = bot.execute(Request::RpsChoose, Lang::De, None).await;
            assert_eq!(choose.content, GameText::RpsChoose.render(Lang::De));
            assert_eq!(choose.buttons, commands::rps_buttons(Lang::De));
            let played = Request::RpsPlay {
                hand: Hand::Rock,
                player: "Puppy",
            };
            let played = bot.execute(played, Lang::En, None).await;
            assert!(played.update);
            assert_eq!(
                played.content,
                "**Puppy** chose rock, PuppyBot scissors: **Puppy wins!**"
            );
        });
    }

    #[test]
    fn guessing_a_number() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let channel = Some(Id::new(4));
            let guess = |number| Request::Guess {
                number,
                max: Some(10),
                player: "Puppy",
            };
            let answer = |text: GameText<'_>| (text.render(Lang::En), Audience::Everyone);
            // A new game, whose number is 10, as every die is at its most.
            assert_eq!(
                said(bot.execute(guess(Some(3)), Lang::En, channel).await),
                answer(GameText::GuessStarted { max: 10 })
            );
            assert_eq!(
                said(bot.execute(guess(None), Lang::En, channel).await),
                answer(GameText::GuessRunning { max: 10 })
            );
            let guessed = |number, hint| GameText::Guessed {
                player: "Puppy",
                number,
                hint,
                max: 10,
            };
            assert_eq!(
                said(bot.execute(guess(Some(3)), Lang::En, channel).await),
                answer(guessed(3, Hint::Higher))
            );
            assert_eq!(
                said(bot.execute(guess(Some(11)), Lang::En, channel).await),
                answer(guessed(11, Hint::Outside))
            );
            assert_eq!(
                said(bot.execute(guess(Some(10)), Lang::En, channel).await),
                answer(guessed(10, Hint::Right { tries: 2 }))
            );
            // Over: the next starts a new one.
            assert_eq!(
                said(bot.execute(guess(Some(5)), Lang::En, channel).await),
                answer(GameText::GuessStarted { max: 10 })
            );
            // Without a channel, no game.
            assert_eq!(
                said(bot.execute(guess(None), Lang::En, None).await),
                answer(GameText::GameFailed)
            );
        });
    }

    #[test]
    fn games_without_randomness() {
        in_runtime(async {
            let mut bot = bot(Box::new(Storage::in_memory()));
            bot.die = |_| None;
            let failed = (GameText::GameFailed.render(Lang::En), Audience::Everyone);
            for request in [
                Request::Coin,
                Request::EightBall(""),
                Request::RpsPlay {
                    hand: Hand::Paper,
                    player: "Puppy",
                },
                Request::Guess {
                    number: None,
                    max: None,
                    player: "Puppy",
                },
            ] {
                let answer = bot
                    .execute(request.clone(), Lang::En, Some(Id::new(4)))
                    .await;
                assert_eq!(said(answer), failed, "{request:?}");
            }
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
                    said(bot.execute(request.clone(), Lang::De, None).await),
                    (Text::StorageFailed.render(Lang::De), Audience::Asker),
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
