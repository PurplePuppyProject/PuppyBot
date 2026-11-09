//! The connection to Discord: the gateway, from which the events come, and the HTTP API, with
//! which PuppyBot answers them.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::fmt;
use std::time::{Duration, Instant};

use puppybot_core::dice::Dice;
use puppybot_core::games::{Coin, GameText, Guessing, Hand, Hint};
use puppybot_core::github::{
    Commit, GithubText, Release, Repo, Watch as Shown, is_valid_branch, new_commits, new_releases,
};
use puppybot_core::i18n::Lang;
use puppybot_core::poll::Poll;
use puppybot_core::quotes::{self, QuoteText};
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
use crate::github::{self, Github, OwnedCommit, OwnedRelease};
use crate::shutdown::{Signals, Stop};
use crate::storage::{Deleted, Quote, Saved, Seen, Store, Watch, Watched};

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
    .union(EventTypeFlags::MESSAGE_CREATE)
    .union(EventTypeFlags::MESSAGE_DELETE)
    .union(EventTypeFlags::MESSAGE_DELETE_BULK);

/// How long PuppyBot waits for Discord to close the connection after a signal.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long PuppyBot trusts GitHub's answer whether a repository is public.
const PUBLIC_FOR: Duration = Duration::from_secs(3600);

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
    /// and the quotes of the servers in `store`, and `github`, which it checks every `interval`
    /// for the repositories that the servers announce. It connects once [`Self::run`] runs.
    #[must_use]
    pub fn new(
        config: &DiscordConfig,
        token: &SecretString,
        store: Box<dyn Store>,
        github: Github,
        interval: Duration,
    ) -> Self {
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
                store,
                mentions: OnceCell::new(),
                die: crate::random::die,
                games: RefCell::new(BTreeMap::new()),
                github,
                interval,
                public: RefCell::new(BTreeMap::new()),
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
        let bot = &self.bot;
        let shard = &mut self.shard;
        // Between the events, in the same task: twilight sends the heartbeats only while it
        // waits for an event, so a slow answer of GitHub mustn't hold it up.
        let github = bot.watch_github();
        tokio::pin!(github);
        let stop = loop {
            let item = tokio::select! {
                stop = signals.recv() => break stop,
                item = shard.next_event(EVENTS) => item,
                () = &mut github => continue,
            };
            match item {
                Some(Ok(Event::GatewayClose(frame))) => last_close = frame,
                Some(Ok(event)) => bot.handle(event).await,
                Some(Err(error)) => tracing::warn!("can't read an event of Discord: {error}"),
                // The shard may not be polled any more.
                None => return Err(Error::Closed(last_close)),
            }
        };
        tracing::info!(signal = stop.name(), "closing the connection to Discord");
        shard.close(CloseFrame::NORMAL);
        // Until Discord closed it, too, but not for long. No events are wanted any more, so the
        // next that comes is Discord's close, unless something fails.
        let _ =
            tokio::time::timeout(CLOSE_TIMEOUT, shard.next_event(EventTypeFlags::empty())).await;
        Ok(stop)
    }
}

/// What PuppyBot does with the events of Discord.
struct Bot {
    http: Client,
    /// The servers of PuppyBot, from the configuration.
    guilds: BTreeSet<Id<GuildMarker>>,
    /// The settings and the quotes of the servers.
    store: Box<dyn Store>,
    /// How messages mention PuppyBot, once Discord said who it is.
    mentions: OnceCell<[String; 2]>,
    /// A die with so many sides, see [`crate::random::die`], or another one in tests.
    die: fn(u32) -> Option<u32>,
    /// The games of guessing a number that run, by channel. A restart ends them.
    games: RefCell<BTreeMap<Id<ChannelMarker>, Guessing>>,
    /// GitHub's API, for the repositories that the servers announce.
    github: Github,
    /// How long PuppyBot waits between its checks of GitHub, see [`github::interval`].
    interval: Duration,
    /// When GitHub last said whether each repository is public, and what it said.
    public: RefCell<BTreeMap<String, (Instant, bool)>>,
}

impl Bot {
    async fn handle(&self, event: Event) {
        match event {
            Event::Ready(ready) => self.ready(&ready).await,
            Event::GuildCreate(guild) => self.joined(guild.id()).await,
            Event::InteractionCreate(interaction) => self.interaction(&interaction.0).await,
            Event::MessageCreate(message) => self.message(&message.message).await,
            Event::MessageDelete(gone) => self.forget(gone.guild_id, &[gone.id]),
            Event::MessageDeleteBulk(gone) => self.forget(gone.guild_id, &gone.ids),
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
        let client = self.http.interaction(interaction.application_id);
        let token = &interaction.token;
        let answered = if matches!(asked.request, Request::GithubAdd { .. }) {
            // GitHub may take longer than Discord waits for an answer, 3 seconds: first that
            // PuppyBot is working on it, then the answer.
            match client
                .create_response(interaction.id, token, &commands::working())
                .await
            {
                Ok(_) => {
                    let answer = self.execute(asked.request, asked.lang, channel).await;
                    let nobody = AllowedMentions::default();
                    client
                        .update_response(token)
                        .content(Some(&answer.content))
                        .allowed_mentions(Some(&nobody))
                        .await
                        .map(drop)
                }
                Err(error) => Err(error),
            }
        } else {
            let response =
                commands::response(self.execute(asked.request, asked.lang, channel).await);
            client
                .create_response(interaction.id, token, &response)
                .await
                .map(drop)
        };
        if let Err(error) = answered {
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
            Request::SaveQuote(guild, quote) => self.save_quote(guild, quote, lang),
            Request::GithubAdd {
                guild,
                repo,
                channel,
                branch,
                releases,
            } => {
                self.github_add(guild, repo, channel, branch, releases, lang)
                    .await
            }
            Request::GithubRemove(guild, repo) => self.github_remove(guild, repo, lang),
            Request::GithubList(guild) => self.github_list(guild, lang),
            Request::ShowQuote(guild, number) => {
                let (content, shown) = self.show_quote(guild, number, lang);
                let audience = if shown {
                    Audience::Everyone
                } else {
                    Audience::Asker
                };
                return Answer::new(content, audience);
            }
            Request::DeleteQuote {
                guild,
                number,
                asker,
                may_manage,
            } => self.delete_quote(guild, number, (!may_manage).then_some(asker), lang),
            Request::Roll { dice, secret } => {
                let audience = if secret {
                    Audience::Asker
                } else {
                    Audience::Everyone
                };
                return Answer::new(self.rolled(&dice, lang), audience);
            }
            Request::Answer(text) => text.render(lang),
            Request::ShowLanguage(guild) => self.show_language(guild, lang),
            Request::SetLanguage(guild, new) => self.change_language(guild, new, lang),
            Request::ShowPrefix(guild) => self.show_prefix(guild, lang),
            Request::SetPrefix(guild, prefix) => self.change_prefix(guild, prefix, lang),
        };
        Answer::new(content, Audience::Asker)
    }

    /// The language of `guild`, as /settings language shows it, in `lang`.
    fn show_language(&self, guild: Id<GuildMarker>, lang: Lang) -> String {
        match self.store.language(guild) {
            Ok(chosen) => Text::LanguageIs { chosen }.render(lang),
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the language of the server from the database: {error}");
                Text::StorageFailed.render(lang)
            }
        }
    }

    /// Chooses `new` as the language of `guild`, and returns the answer, in `lang`.
    fn change_language(&self, guild: Id<GuildMarker>, new: Lang, lang: Lang) -> String {
        match self.store.set_language(guild, new) {
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
        }
    }

    /// The prefix of `guild`, as /settings prefix shows it, in `lang`.
    fn show_prefix(&self, guild: Id<GuildMarker>, lang: Lang) -> String {
        match self.store.prefix(guild) {
            Ok(chosen) => Text::PrefixIs {
                chosen: chosen.as_deref(),
            }
            .render(lang),
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the prefix of the server from the database: {error}");
                Text::StorageFailed.render(lang)
            }
        }
    }

    /// Chooses `prefix` for the text commands of `guild`, and returns the answer, in `lang`.
    fn change_prefix(&self, guild: Id<GuildMarker>, prefix: &str, lang: Lang) -> String {
        match self.store.set_prefix(guild, prefix) {
            Ok(()) => {
                tracing::info!(server = %guild, prefix, "changed the prefix of the server");
                Text::PrefixSet { prefix }.render(lang)
            }
            Err(error) => {
                tracing::warn!(server = %guild, "can't save the prefix of the server in the database: {error}");
                Text::StorageFailed.render(lang)
            }
        }
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

    /// Saves `quote` on `guild`, if it is one, and returns the answer, in `lang`.
    fn save_quote(&self, guild: Id<GuildMarker>, quote: Quote, lang: Lang) -> String {
        let text = match quotes::quote(&quote.text) {
            Ok(text) => text.to_owned(),
            Err(error) => return QuoteText::error(error).render(lang),
        };
        let quote = Quote { text, ..quote };
        match self.store.save_quote(guild, &quote) {
            Ok(saved) => {
                if let Saved::New(number) = saved {
                    tracing::info!(server = %guild, number, "saved a quote");
                }
                saved_text(saved).render(lang)
            }
            Err(error) => {
                tracing::warn!(server = %guild, "can't save a quote in the database: {error}");
                Text::StorageFailed.render(lang)
            }
        }
    }

    /// The quote `number` of `guild`, or one at random, in `lang`, and whether it is one: a quote
    /// is for everyone, what went wrong only for the person who asked.
    fn show_quote(
        &self,
        guild: Id<GuildMarker>,
        number: Option<u32>,
        lang: Lang,
    ) -> (String, bool) {
        let mut die = self.die;
        let found = match number {
            Some(number) => self
                .store
                .quote(guild, number)
                .map(|quote| quote.map(|quote| (number, quote))),
            None => self.store.random_quote(guild, &mut die),
        };
        match found {
            Ok(Some((number, quote))) => {
                let link = format!(
                    "https://discord.com/channels/{guild}/{}/{}",
                    quote.channel, quote.message
                );
                let shown = QuoteText::Shown {
                    number,
                    author: &quote.name,
                    said: commands::created(quote.message),
                    text: &quote.text,
                    link: &link,
                };
                (shown.render(lang), true)
            }
            Ok(None) => {
                let missing = number.map_or(QuoteText::NoQuotes, |number| QuoteText::NoSuchQuote {
                    number,
                });
                (missing.render(lang), false)
            }
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the quotes from the database: {error}");
                (Text::StorageFailed.render(lang), false)
            }
        }
    }

    /// Deletes the quote `number` of `guild`, but only if `author` wrote it, if given, and
    /// returns the answer, in `lang`.
    fn delete_quote(
        &self,
        guild: Id<GuildMarker>,
        number: u32,
        author: Option<u64>,
        lang: Lang,
    ) -> String {
        let text = match self.store.delete_quote(guild, number, author) {
            Ok(Deleted::Yes) => {
                tracing::info!(server = %guild, number, "deleted a quote");
                QuoteText::Deleted { number }
            }
            Ok(Deleted::NotTheirs) => QuoteText::NotYours { number },
            Ok(Deleted::Missing) => QuoteText::NoSuchQuote { number },
            Err(error) => {
                tracing::warn!(server = %guild, "can't delete a quote from the database: {error}");
                return Text::StorageFailed.render(lang);
            }
        };
        text.render(lang)
    }

    /// Announces `repo` on `guild` in `channel`, on `branch` or else the default branch, with its
    /// releases or not, if GitHub has it as a public repository, and returns the answer, in
    /// `lang`. What GitHub has now isn't announced, only what comes later.
    async fn github_add(
        &self,
        guild: Id<GuildMarker>,
        repo: &str,
        channel: Id<ChannelMarker>,
        branch: Option<&str>,
        releases: bool,
        lang: Lang,
    ) -> String {
        let Some(repo) = Repo::parse(repo) else {
            return GithubText::InvalidRepo.render(lang);
        };
        if branch.is_some_and(|branch| !is_valid_branch(branch)) {
            return GithubText::InvalidBranch.render(lang);
        }
        let name = repo.to_string();
        let info = match self.github.repo(&name).await {
            Ok(info) if info.public => info,
            Ok(_) | Err(github::Error::NotFound) => {
                return GithubText::NoSuchRepo { repo: &name }.render(lang);
            }
            Err(error) => return github_failed(&error, lang),
        };
        // As GitHub writes it.
        let repo = info.repo.to_string();
        let branch = branch.unwrap_or(&info.default_branch);
        let commits = match self.github.commits(&repo, branch, false).await {
            Ok(commits) => commits.unwrap_or_default(),
            Err(github::Error::NotFound) => {
                return GithubText::NoSuchBranch {
                    repo: &repo,
                    branch,
                }
                .render(lang);
            }
            Err(error) => return github_failed(&error, lang),
        };
        let release = if releases {
            match self.github.releases(&repo, false).await {
                Ok(found) => Some(newest_release(&found.unwrap_or_default())),
                Err(error) => return github_failed(&error, lang),
            }
        } else {
            None
        };
        let seen = Seen {
            commit: commits.first().map(|commit| commit.sha.clone()),
            release,
        };
        let watch = Watch {
            repo: repo.clone(),
            branch: branch.to_owned(),
            channel: channel.get(),
            releases,
        };
        match self.store.add_watch(guild, &watch, &seen) {
            Ok(Watched::Full) => GithubText::Full.render(lang),
            Ok(_) => {
                tracing::info!(server = %guild, repo, branch, "announcing a repository of GitHub");
                let channel = format!("<#{channel}>");
                GithubText::Added {
                    repo: &repo,
                    branch,
                    channel: &channel,
                    releases,
                }
                .render(lang)
            }
            Err(error) => {
                tracing::warn!(server = %guild, "can't save a repository to announce in the database: {error}");
                Text::StorageFailed.render(lang)
            }
        }
    }

    /// No longer announces `repo` on `guild`, and returns the answer, in `lang`.
    fn github_remove(&self, guild: Id<GuildMarker>, repo: &str, lang: Lang) -> String {
        let Some(repo) = Repo::parse(repo) else {
            return GithubText::InvalidRepo.render(lang);
        };
        let repo = repo.to_string();
        match self.store.remove_watch(guild, &repo) {
            Ok(true) => {
                tracing::info!(server = %guild, repo, "no longer announcing a repository of GitHub");
                GithubText::Removed { repo: &repo }.render(lang)
            }
            Ok(false) => GithubText::NotWatched { repo: &repo }.render(lang),
            Err(error) => {
                tracing::warn!(server = %guild, "can't remove a repository to announce from the database: {error}");
                Text::StorageFailed.render(lang)
            }
        }
    }

    /// The repositories that `guild` announces, in `lang`.
    fn github_list(&self, guild: Id<GuildMarker>, lang: Lang) -> String {
        let watches = match self.store.watches(Some(guild)) {
            Ok(watches) => watches,
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the repositories to announce from the database: {error}");
                return Text::StorageFailed.render(lang);
            }
        };
        let channels: Vec<String> = watches
            .iter()
            .map(|(_, watch)| format!("<#{}>", watch.channel))
            .collect();
        let shown: Vec<Shown<'_>> = watches
            .iter()
            .zip(&channels)
            .map(|((_, watch), channel)| Shown {
                repo: &watch.repo,
                branch: &watch.branch,
                channel,
                releases: watch.releases,
            })
            .collect();
        GithubText::Watched(&shown).render(lang)
    }

    /// Checks GitHub again and again, between the events: at once, and then after each
    /// interval, see [`github::interval`].
    async fn watch_github(&self) {
        loop {
            let requests = self.github_round().await;
            let wait = github::interval(self.interval, requests, self.github.has_token());
            tokio::time::sleep(wait).await;
        }
    }

    /// One check of the repositories that the servers announce. Returns how many requests to
    /// GitHub it took, about.
    async fn github_round(&self) -> usize {
        match self.store.watches(None) {
            Ok(watches) => self.check_github(&watches).await,
            Err(error) => {
                tracing::warn!(
                    "can't read the repositories to announce from the database: {error}"
                );
                0
            }
        }
    }

    /// Checks GitHub for `watches`, the repositories of the servers, and announces what is new,
    /// of the public ones. Returns how many requests that took, about.
    async fn check_github(&self, watches: &[(Id<GuildMarker>, Watch)]) -> usize {
        let mut branches: BTreeMap<(&str, &str), Vec<Place>> = BTreeMap::new();
        let mut releases: BTreeMap<&str, Vec<Place>> = BTreeMap::new();
        for (guild, watch) in watches {
            if let Some(channel) = Id::new_checked(watch.channel) {
                let place = (*guild, channel);
                let branch = (watch.repo.as_str(), watch.branch.as_str());
                branches.entry(branch).or_default().push(place);
                if watch.releases {
                    releases.entry(watch.repo.as_str()).or_default().push(place);
                }
            }
        }
        let repos: BTreeSet<&str> = branches.keys().map(|&(repo, _)| repo).collect();
        let mut public = BTreeSet::new();
        for &repo in &repos {
            if self.is_public(repo).await {
                public.insert(repo);
            }
        }
        for (&(repo, branch), places) in &branches {
            if public.contains(repo) {
                self.check_commits(repo, branch, places).await;
            }
        }
        for (&repo, places) in &releases {
            if public.contains(repo) {
                self.check_releases(repo, places).await;
            }
        }
        repos.len() + branches.len() + releases.len()
    }

    /// Whether `repo` is public, as GitHub said within the last hour. Without an answer, the
    /// last one counts, and at first, no.
    async fn is_public(&self, repo: &str) -> bool {
        let known = self.public.borrow().get(repo).copied();
        if let Some((asked, public)) = known
            && asked.elapsed() < PUBLIC_FOR
        {
            return public;
        }
        let public = match self.github.repo(repo).await {
            Ok(info) => info.public,
            Err(github::Error::NotFound) => false,
            Err(error) => {
                tracing::warn!(
                    repo,
                    "can't ask GitHub whether a repository is public: {error}"
                );
                return known.is_some_and(|(_, public)| public);
            }
        };
        if !public {
            tracing::warn!(repo, "not announcing a repository that isn't public");
        }
        self.public
            .borrow_mut()
            .insert(repo.to_owned(), (Instant::now(), public));
        public
    }

    /// Checks the commits of `branch` of `repo`, and announces the new ones in `places`.
    async fn check_commits(&self, repo: &str, branch: &str, places: &[Place]) {
        let commits = match self.github.commits(repo, branch, true).await {
            Ok(Some(commits)) => commits,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(repo, branch, "can't check the commits on GitHub: {error}");
                return;
            }
        };
        let Some(newest) = commits.first() else {
            return;
        };
        let last = match self.store.swap_commit(repo, branch, &newest.sha) {
            Ok(Some(last)) => last,
            // The first check notes only where the branch is.
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    repo,
                    branch,
                    "can't save the newest commit in the database: {error}"
                );
                return;
            }
        };
        let commits: Vec<Commit<'_>> = commits.iter().map(OwnedCommit::commit).collect();
        let new = new_commits(&commits, &last);
        if new.commits.is_empty() {
            return;
        }
        for &(guild, channel) in places {
            let text = GithubText::Pushed {
                repo,
                branch,
                commits: &new.commits,
                more: new.more,
                rewritten: new.rewritten,
            };
            self.announce(channel, &text.render(self.language(guild)))
                .await;
        }
    }

    /// Checks the releases of `repo`, and announces the new ones in `places`.
    async fn check_releases(&self, repo: &str, places: &[Place]) {
        let releases = match self.github.releases(repo, true).await {
            Ok(Some(releases)) => releases,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(repo, "can't check the releases on GitHub: {error}");
                return;
            }
        };
        let last = match self.store.swap_release(repo, newest_release(&releases)) {
            Ok(Some(last)) => last,
            // The first check notes only which release is the newest.
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    repo,
                    "can't save the newest release in the database: {error}"
                );
                return;
            }
        };
        let releases: Vec<Release<'_>> = releases.iter().map(OwnedRelease::release).collect();
        for release in new_releases(&releases, last) {
            for &(guild, channel) in places {
                let text = GithubText::Released { repo, release };
                self.announce(channel, &text.render(self.language(guild)))
                    .await;
            }
        }
    }

    /// Posts `content` in `channel`, as a message that mentions nobody.
    async fn announce(&self, channel: Id<ChannelMarker>, content: &str) {
        let nobody = AllowedMentions::default();
        let message = self
            .http
            .create_message(channel)
            .content(content)
            .allowed_mentions(Some(&nobody));
        if let Err(error) = message.await {
            tracing::warn!(channel = %channel, "can't post an announcement: {error}");
        }
    }

    /// Messages that are gone, on `guild`: their quotes go with them.
    fn forget(&self, guild: Option<Id<GuildMarker>>, messages: &[Id<MessageMarker>]) {
        let Some(guild) = guild.filter(|guild| self.guilds.contains(guild)) else {
            return;
        };
        for &message in messages {
            match self.store.forget_message(guild, message) {
                Ok(Some(number)) => {
                    tracing::info!(server = %guild, number, "deleted a quote, as its message is gone");
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(server = %guild, "can't delete the quote of a deleted message from the database: {error}");
                }
            }
        }
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
            TextCommand::Quote => self.show_quote(guild, arguments.parse().ok(), lang).0,
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
        match self.store.prefix(guild) {
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
        match self.store.language(guild) {
            Ok(chosen) => chosen.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(server = %guild, "can't read the language of the server from the database: {error}");
                Lang::default()
            }
        }
    }
}

/// Where PuppyBot announces something: the server, for its language, and the channel.
type Place = (Id<GuildMarker>, Id<ChannelMarker>);

/// The ID of the newest of `releases`, 0 if there are none.
fn newest_release(releases: &[OwnedRelease]) -> u64 {
    releases
        .iter()
        .map(|release| release.id)
        .max()
        .unwrap_or_default()
}

/// The answer when PuppyBot can't ask GitHub, as `error` says, in `lang`.
fn github_failed(error: &github::Error, lang: Lang) -> String {
    tracing::warn!("can't ask GitHub about a repository: {error}");
    GithubText::GithubFailed.render(lang)
}

/// The answer to saving a quote, as `saved` says.
const fn saved_text(saved: Saved) -> QuoteText<'static> {
    match saved {
        Saved::New(number) => QuoteText::Saved { number },
        Saved::Already(number) => QuoteText::AlreadySaved { number },
        Saved::Full => QuoteText::Full,
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
        GuildCreate, InteractionCreate, MessageCreate, MessageDelete, MessageDeleteBulk,
    };
    use twilight_model::guild::UnavailableGuild;

    use puppybot_core::i18n::Lang;

    use super::*;
    use crate::commands::tests::{GUILD, interaction};
    use crate::fake_github::{FakeGithub, commits_json, releases_json, repo_json};
    use crate::storage::{self, Quotes, Seen, Settings, Storage, Watch, Watched, Watches};

    /// Settings and quotes that can be neither read nor saved, e.g. on a full disk.
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

    impl Quotes for Broken {
        fn save_quote(&self, _: Id<GuildMarker>, _: &Quote) -> Result<Saved, storage::Error> {
            Err(disk_full())
        }

        fn quote(&self, _: Id<GuildMarker>, _: u32) -> Result<Option<Quote>, storage::Error> {
            Err(disk_full())
        }

        fn random_quote(
            &self,
            _: Id<GuildMarker>,
            _: &mut dyn FnMut(u32) -> Option<u32>,
        ) -> Result<Option<(u32, Quote)>, storage::Error> {
            Err(disk_full())
        }

        fn delete_quote(
            &self,
            _: Id<GuildMarker>,
            _: u32,
            _: Option<u64>,
        ) -> Result<Deleted, storage::Error> {
            Err(disk_full())
        }

        fn forget_message(
            &self,
            _: Id<GuildMarker>,
            _: Id<MessageMarker>,
        ) -> Result<Option<u32>, storage::Error> {
            Err(disk_full())
        }
    }

    impl Watches for Broken {
        fn watches(
            &self,
            _: Option<Id<GuildMarker>>,
        ) -> Result<Vec<(Id<GuildMarker>, Watch)>, storage::Error> {
            Err(disk_full())
        }

        fn add_watch(
            &self,
            _: Id<GuildMarker>,
            _: &Watch,
            _: &Seen,
        ) -> Result<Watched, storage::Error> {
            Err(disk_full())
        }

        fn remove_watch(&self, _: Id<GuildMarker>, _: &str) -> Result<bool, storage::Error> {
            Err(disk_full())
        }

        fn swap_commit(&self, _: &str, _: &str, _: &str) -> Result<Option<String>, storage::Error> {
            Err(disk_full())
        }

        fn swap_release(&self, _: &str, _: u64) -> Result<Option<u64>, storage::Error> {
            Err(disk_full())
        }
    }

    /// PuppyBot on the server [`GUILD`], with `store`, and an API that fails every request,
    /// as nothing listens on its port. The client starts its rate limiter in the runtime, as
    /// `Discord::new` does, so this must run in one.
    fn bot(store: Box<dyn Store>) -> Bot {
        Bot {
            http: Client::builder()
                .token("token".to_owned())
                .proxy("127.0.0.1:1".to_owned(), true)
                .build(),
            guilds: BTreeSet::from([Id::new(GUILD)]),
            store,
            mentions: OnceCell::new(),
            // Every die at its most.
            die: Some,
            games: RefCell::new(BTreeMap::new()),
            github: Github::new("http://127.0.0.1:1", None, github::TIMEOUT).unwrap(),
            interval: Duration::from_secs(300),
            public: RefCell::new(BTreeMap::new()),
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

    /// The quote of the message `message` with `text`, by the person 5, in the channel 4.
    fn quote_of(message: u64, text: &str) -> Quote {
        Quote {
            author: 5,
            name: "Puppy".to_owned(),
            channel: 4,
            message,
            text: text.to_owned(),
        }
    }

    #[test]
    fn saving_showing_and_deleting_quotes() {
        in_runtime(async {
            let bot = bot(Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let message = 1_300_000_000_000_000_010;
            let only_asker = |text: QuoteText<'_>| (text.render(Lang::En), Audience::Asker);
            let save = |quote| Request::SaveQuote(guild, quote);
            let delete = |asker, may_manage| Request::DeleteQuote {
                guild,
                number: 1,
                asker,
                may_manage,
            };
            let asked = [
                (
                    save(quote_of(message, " Wuff! ")),
                    QuoteText::Saved { number: 1 },
                ),
                (
                    save(quote_of(message, "Wuff!")),
                    QuoteText::AlreadySaved { number: 1 },
                ),
                (save(quote_of(2, "")), QuoteText::Empty),
                (save(quote_of(3, &"a".repeat(501))), QuoteText::TooLong),
                (
                    Request::ShowQuote(guild, Some(2)),
                    QuoteText::NoSuchQuote { number: 2 },
                ),
            ];
            for (request, answer) in asked {
                let said = said(bot.execute(request.clone(), Lang::En, None).await);
                assert_eq!(said, only_asker(answer), "{request:?}");
            }

            // For everyone, without the spaces around it.
            let link = format!("https://discord.com/channels/{GUILD}/4/{message}");
            let shown = QuoteText::Shown {
                number: 1,
                author: "Puppy",
                said: commands::created(message),
                text: "Wuff!",
                link: &link,
            };
            let everyone = (shown.render(Lang::En), Audience::Everyone);
            for number in [Some(1), None] {
                let said = said(
                    bot.execute(Request::ShowQuote(guild, number), Lang::En, None)
                        .await,
                );
                assert_eq!(said, everyone, "{number:?}");
            }

            // By the person quoted, or by someone who may manage messages.
            let asked = [
                (delete(6, false), QuoteText::NotYours { number: 1 }),
                (delete(6, true), QuoteText::Deleted { number: 1 }),
                (delete(5, false), QuoteText::NoSuchQuote { number: 1 }),
                (Request::ShowQuote(guild, None), QuoteText::NoQuotes),
                (
                    save(quote_of(message, "Wuff!")),
                    QuoteText::Saved { number: 2 },
                ),
                (save(quote_of(3, "Wau!")), QuoteText::Saved { number: 3 }),
            ];
            for (request, answer) in asked {
                let said = said(bot.execute(request.clone(), Lang::En, None).await);
                assert_eq!(said, only_asker(answer), "{request:?}");
            }
            let mine = Request::DeleteQuote {
                guild,
                number: 3,
                asker: 5,
                may_manage: false,
            };
            let said = said(bot.execute(mine, Lang::En, None).await);
            assert_eq!(said, only_asker(QuoteText::Deleted { number: 3 }));

            // Gone with their messages, on PuppyBot's servers.
            let gone = [Id::new(1), Id::new(message)];
            bot.forget(Some(Id::new(GUILD + 1)), &gone);
            bot.forget(None, &gone);
            assert!(bot.store.quote(guild, 2).unwrap().is_some());
            bot.forget(Some(guild), &gone);
            assert_eq!(bot.store.quote(guild, 2).unwrap(), None);
        });
    }

    #[test]
    fn quotes_that_fail() {
        in_runtime(async {
            let bot = bot(Box::new(Broken));
            let guild = Id::new(GUILD);
            for request in [
                Request::SaveQuote(guild, quote_of(1, "Wuff!")),
                Request::ShowQuote(guild, Some(1)),
                Request::ShowQuote(guild, None),
                Request::DeleteQuote {
                    guild,
                    number: 1,
                    asker: 5,
                    may_manage: false,
                },
            ] {
                assert_eq!(
                    said(bot.execute(request.clone(), Lang::De, None).await),
                    (Text::StorageFailed.render(Lang::De), Audience::Asker),
                    "{request:?}"
                );
            }
            // A warning in the log.
            bot.forget(Some(guild), &[Id::new(1)]);
        });
    }

    #[test]
    fn answers_to_saving_quotes() {
        assert_eq!(saved_text(Saved::New(3)), QuoteText::Saved { number: 3 });
        assert_eq!(
            saved_text(Saved::Already(3)),
            QuoteText::AlreadySaved { number: 3 }
        );
        assert_eq!(saved_text(Saved::Full), QuoteText::Full);
    }

    /// PuppyBot with `store`, and `fake` as GitHub.
    fn with_github(fake: &FakeGithub, store: Box<dyn Store>) -> Bot {
        let mut bot = bot(store);
        bot.github = Github::new(&fake.url, None, github::TIMEOUT).unwrap();
        bot
    }

    /// The path of the commits of `branch` of `repo` on GitHub.
    fn commits(repo: &str, branch: &str) -> String {
        format!("/repos/{repo}/commits?sha={branch}&per_page=30")
    }

    /// The path of the releases of `repo` on GitHub.
    fn releases(repo: &str) -> String {
        format!("/repos/{repo}/releases?per_page=10")
    }

    #[test]
    fn adding_repositories_of_github() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let bot = with_github(&fake, Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let add = |repo, branch, releases| Request::GithubAdd {
                guild,
                repo,
                channel: Id::new(4),
                branch,
                releases,
            };
            fake.answer("/repos/a/b", 200, &[], &repo_json("a/b", false));
            fake.answer(
                &commits("a/b", "main"),
                200,
                &[],
                &commits_json(&[("abcdef0", "Wuff")]),
            );
            fake.answer(&releases("a/b"), 200, &[], &releases_json(&[(7, "v1")]));
            fake.answer(&commits("a/b", "dev"), 500, &[], "");
            fake.answer("/repos/a/secret", 200, &[], &repo_json("a/secret", true));
            fake.answer("/repos/a/broken", 500, &[], "");
            fake.answer("/repos/a/c", 200, &[], &repo_json("a/c", false));
            fake.answer(&commits("a/c", "main"), 200, &[], "[]");
            fake.answer(&releases("a/c"), 500, &[], "");
            let added = |repo, releases| GithubText::Added {
                repo,
                branch: "main",
                channel: "<#4>",
                releases,
            };
            let asked = [
                (add("not a repository", None, true), GithubText::InvalidRepo),
                (add("a/b", Some("-x"), true), GithubText::InvalidBranch),
                (
                    add("a/missing", None, true),
                    GithubText::NoSuchRepo { repo: "a/missing" },
                ),
                (
                    add("a/secret", None, true),
                    GithubText::NoSuchRepo { repo: "a/secret" },
                ),
                (add("a/broken", None, true), GithubText::GithubFailed),
                (
                    add("a/b", Some("next"), true),
                    GithubText::NoSuchBranch {
                        repo: "a/b",
                        branch: "next",
                    },
                ),
                (add("a/b", Some("dev"), true), GithubText::GithubFailed),
                (add("a/c", None, true), GithubText::GithubFailed),
                // A branch without commits yet.
                (add("a/c", None, false), added("a/c", false)),
                (
                    add("https://github.com/a/b", None, true),
                    added("a/b", true),
                ),
                // Again, in the same channel.
                (add("a/b", None, true), added("a/b", true)),
            ];
            for (request, text) in asked {
                let said = said(bot.execute(request.clone(), Lang::En, None).await);
                assert_eq!(
                    said,
                    (text.render(Lang::En), Audience::Asker),
                    "{request:?}"
                );
            }
        });
    }

    #[test]
    fn listing_and_removing_repositories_of_github() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let bot = with_github(&fake, Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let add = |repo, releases| Request::GithubAdd {
                guild,
                repo,
                channel: Id::new(4),
                branch: None,
                releases,
            };
            fake.answer("/repos/a/b", 200, &[], &repo_json("a/b", false));
            fake.answer(
                &commits("a/b", "main"),
                200,
                &[],
                &commits_json(&[("abcdef0", "Wuff")]),
            );
            fake.answer(&releases("a/b"), 200, &[], &releases_json(&[(7, "v1")]));
            fake.answer("/repos/a/c", 200, &[], &repo_json("a/c", false));
            fake.answer(&commits("a/c", "main"), 200, &[], "[]");
            for request in [add("https://github.com/a/b", true), add("a/c", false)] {
                bot.execute(request, Lang::En, None).await;
            }
            // What there was then.
            let noted = bot.store.swap_commit("a/b", "main", "abcdef0").unwrap();
            assert_eq!(noted.as_deref(), Some("abcdef0"));
            assert_eq!(bot.store.swap_release("a/b", 7).unwrap(), Some(7));
            assert_eq!(bot.store.swap_release("a/c", 1).unwrap(), None);

            // The list.
            let shown = [
                Shown {
                    repo: "a/b",
                    branch: "main",
                    channel: "<#4>",
                    releases: true,
                },
                Shown {
                    repo: "a/c",
                    branch: "main",
                    channel: "<#4>",
                    releases: false,
                },
            ];
            let list = said(
                bot.execute(Request::GithubList(guild), Lang::De, None)
                    .await,
            );
            assert_eq!(list.0, GithubText::Watched(&shown).render(Lang::De));

            // Up to 10 on a server.
            for n in 0..8 {
                let repo = format!("a/r{n}");
                fake.answer(
                    &format!("/repos/{repo}"),
                    200,
                    &[],
                    &repo_json(&repo, false),
                );
                fake.answer(&commits(&repo, "main"), 200, &[], "[]");
                let request = Request::GithubAdd {
                    guild,
                    repo: &repo,
                    channel: Id::new(4),
                    branch: None,
                    releases: false,
                };
                let text = GithubText::Added {
                    repo: &repo,
                    branch: "main",
                    channel: "<#4>",
                    releases: false,
                };
                let said = said(bot.execute(request, Lang::En, None).await);
                assert_eq!(said.0, text.render(Lang::En));
            }
            fake.answer("/repos/a/d", 200, &[], &repo_json("a/d", false));
            fake.answer(&commits("a/d", "main"), 200, &[], "[]");
            let full = said(bot.execute(add("a/d", false), Lang::En, None).await);
            assert_eq!(full.0, GithubText::Full.render(Lang::En));

            // Removing, in any case.
            let asked = [
                (
                    Request::GithubRemove(guild, "A/B"),
                    GithubText::Removed { repo: "A/B" },
                ),
                (
                    Request::GithubRemove(guild, "a/b"),
                    GithubText::NotWatched { repo: "a/b" },
                ),
                (Request::GithubRemove(guild, "?"), GithubText::InvalidRepo),
            ];
            for (request, text) in asked {
                let said = said(bot.execute(request.clone(), Lang::En, None).await);
                assert_eq!(said.0, text.render(Lang::En), "{request:?}");
            }
        });
    }

    #[test]
    fn repositories_of_github_that_fail() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let bot = with_github(&fake, Box::new(Broken));
            let guild = Id::new(GUILD);
            fake.answer("/repos/a/b", 200, &[], &repo_json("a/b", false));
            fake.answer(
                &commits("a/b", "main"),
                200,
                &[],
                &commits_json(&[("bbbbbbb", "Wau")]),
            );
            fake.answer(&releases("a/b"), 200, &[], &releases_json(&[(8, "v2")]));
            let add = Request::GithubAdd {
                guild,
                repo: "a/b",
                channel: Id::new(4),
                branch: None,
                releases: false,
            };
            for request in [
                add,
                Request::GithubRemove(guild, "a/b"),
                Request::GithubList(guild),
            ] {
                assert_eq!(
                    said(bot.execute(request.clone(), Lang::De, None).await),
                    (Text::StorageFailed.render(Lang::De), Audience::Asker),
                    "{request:?}"
                );
            }
            // Checks: warnings in the log, and nothing is announced.
            assert_eq!(bot.github_round().await, 0);
            let watch = Watch {
                repo: "a/b".to_owned(),
                branch: "main".to_owned(),
                channel: 4,
                releases: true,
            };
            assert_eq!(bot.check_github(&[(guild, watch)]).await, 3);
        });
    }

    #[test]
    fn checks_of_github() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let bot = with_github(&fake, Box::new(Storage::in_memory()));
            let guild = Id::new(GUILD);
            let watch = |repo: &str, releases| Watch {
                repo: repo.to_owned(),
                branch: "main".to_owned(),
                channel: 4,
                releases,
            };
            let seen = Seen {
                commit: Some("aaaaaaa".to_owned()),
                release: Some(7),
            };
            for (repo, seen) in [
                ("a/b", &seen),
                ("a/new", &Seen::default()),
                ("a/secret", &seen),
                ("a/down", &seen),
                ("a/empty", &seen),
            ] {
                bot.store
                    .add_watch(guild, &watch(repo, true), seen)
                    .unwrap();
            }
            // Of no channel, which Discord doesn't send: none.
            let nowhere = Watch {
                channel: 0,
                ..watch("a/b", false)
            };
            for repo in ["a/b", "a/new", "a/down", "a/empty"] {
                fake.answer(&format!("/repos/{repo}"), 200, &[], &repo_json(repo, false));
            }
            fake.answer("/repos/a/secret", 200, &[], &repo_json("a/secret", true));
            let newer = commits_json(&[
                ("ccccccc", "Third"),
                ("bbbbbbb", "Second"),
                ("aaaaaaa", "First"),
            ]);
            fake.answer(&commits("a/b", "main"), 200, &[("etag", "\"1\"")], &newer);
            fake.answer(
                &releases("a/b"),
                200,
                &[("etag", "\"2\"")],
                &releases_json(&[(9, "v3"), (8, "v2"), (7, "v1")]),
            );
            fake.answer(
                &commits("a/new", "main"),
                200,
                &[],
                &commits_json(&[("ddddddd", "New")]),
            );
            fake.answer(&releases("a/new"), 200, &[], "[]");
            fake.answer(&commits("a/down", "main"), 500, &[], "");
            fake.answer(&releases("a/down"), 500, &[], "");
            fake.answer(&commits("a/empty", "main"), 200, &[], "[]");
            fake.answer(&releases("a/empty"), 200, &[], &releases_json(&[(7, "v1")]));
            let mut watches = bot.store.watches(None).unwrap();
            watches.push((guild, nowhere));
            // The repositories, their branches, and their releases.
            assert_eq!(bot.check_github(&watches).await, 15);

            // The newest, now, of the public ones; the first check of one only notes it.
            let swap = |repo, sha| bot.store.swap_commit(repo, "main", sha).unwrap();
            assert_eq!(swap("a/b", "ccccccc").as_deref(), Some("ccccccc"));
            assert_eq!(bot.store.swap_release("a/b", 9).unwrap(), Some(9));
            assert_eq!(swap("a/new", "ddddddd").as_deref(), Some("ddddddd"));
            assert_eq!(bot.store.swap_release("a/new", 0).unwrap(), Some(0));
            assert_eq!(swap("a/secret", "aaaaaaa").as_deref(), Some("aaaaaaa"));
            assert_eq!(swap("a/down", "aaaaaaa").as_deref(), Some("aaaaaaa"));

            // Again: GitHub says "not modified", or the same, and whether a repository is
            // public comes from the last hour.
            let asked = fake.got().len();
            fake.answer(
                &commits("a/new", "main"),
                200,
                &[],
                &commits_json(&[("ddddddd", "New")]),
            );
            assert_eq!(bot.github_round().await, 15);
            let paths: Vec<String> = fake
                .got()
                .into_iter()
                .skip(asked)
                .map(|got| got.path)
                .collect();
            assert!(!paths.iter().any(|path| path == "/repos/a/b"), "{paths:?}");

            // When GitHub doesn't answer, the last answer counts; at first, it is no.
            let stale = Instant::now().checked_sub(PUBLIC_FOR * 2).unwrap();
            bot.public
                .borrow_mut()
                .insert("a/b".to_owned(), (stale, true));
            bot.public
                .borrow_mut()
                .insert("a/down".to_owned(), (stale, false));
            fake.answer("/repos/a/b", 500, &[], "");
            fake.answer("/repos/a/down", 500, &[], "");
            assert!(bot.is_public("a/b").await);
            assert!(!bot.is_public("a/down").await);
            assert!(!bot.is_public("a/missing").await);

            // Again and again, until the test stops.
            let watching = tokio::time::timeout(Duration::from_millis(200), bot.watch_github());
            assert!(watching.await.is_err());
        });
    }

    #[test]
    fn announcements() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let mut bot = bot(Box::new(Storage::in_memory()));
            // As Discord's API, which takes the message.
            bot.http = Client::builder()
                .token("token".to_owned())
                .proxy(fake.url.trim_start_matches("http://").to_owned(), true)
                .build();
            fake.answer("/api/v10/channels/4/messages", 200, &[], "{}");
            bot.announce(Id::new(4), "Wuff").await;
            let got = fake.got();
            assert_eq!(got[0].path, "/api/v10/channels/4/messages");
            // And one that fails: a warning in the log.
            bot.announce(Id::new(5), "Wuff").await;
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
            let gone = MessageDelete {
                channel_id: Id::new(4),
                guild_id: Some(Id::new(GUILD)),
                id: Id::new(10),
            };
            bot.handle(Event::MessageDelete(gone)).await;
            let gone = MessageDeleteBulk {
                channel_id: Id::new(4),
                guild_id: Some(Id::new(GUILD)),
                ids: vec![Id::new(10), Id::new(11)],
            };
            bot.handle(Event::MessageDeleteBulk(gone)).await;
            // Not wanted, see EVENTS.
            bot.handle(Event::Resumed).await;
        });
    }
}
