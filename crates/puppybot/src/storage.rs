//! The database of PuppyBot: the settings and the quotes of each server, and the repositories of
//! GitHub that it announces, in one file of
//! [redb](https://www.redb.org), e.g. in a Docker volume.
//!
//! Its data have a [`VERSION`], so that a later PuppyBot can store them differently, and convert
//! the data of an earlier one when it opens them. A PuppyBot refuses data of a version it doesn't
//! know, e.g. after going back to an older release, instead of misreading them.
//!
//! The calls are short and the settings change rarely, so they run on the thread of the bot's
//! runtime; a write returns once its data are on the disk.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use puppybot_core::github::MAX_REPOS;
use puppybot_core::i18n::Lang;
use puppybot_core::quotes::MAX_QUOTES;
use puppybot_core::text_command;
use redb::{
    Builder, Database, ReadableDatabase as _, ReadableTable as _, TableDefinition, WriteTransaction,
};
use twilight_model::id::Id;
use twilight_model::id::marker::{GuildMarker, MessageMarker};

/// The version of the stored data. A PuppyBot that stores them differently raises it, and
/// converts the data of the versions before when it opens them.
pub const VERSION: u64 = 1;

/// What the database holds about itself, under [`VERSION_KEY`].
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

/// The key of the [`VERSION`] of the data in [`META`].
const VERSION_KEY: &str = "version";

/// The language chosen for each server, by the server's ID, as its [`Lang::code`].
const LANGUAGES: TableDefinition<u64, &str> = TableDefinition::new("languages");

/// The prefix of the text commands chosen for each server, by the server's ID.
const PREFIXES: TableDefinition<u64, &str> = TableDefinition::new("prefixes");

/// The quotes of each server, by the server's ID and their number: the IDs of the person who wrote
/// it, of the channel and of the message, the person's name, and the text, see [`Quote`].
type QuoteRecord<'a> = (u64, u64, u64, &'a str, &'a str);

/// The quotes of each server, see [`QuoteRecord`].
const QUOTES: TableDefinition<(u64, u32), QuoteRecord<'_>> = TableDefinition::new("quotes");

/// The number of the quote of each quoted message, by the IDs of the server and the message.
const QUOTED: TableDefinition<(u64, u64), u32> = TableDefinition::new("quoted");

/// The last number that each server gave a quote, so that no number comes twice.
const QUOTE_NUMBERS: TableDefinition<u64, u32> = TableDefinition::new("quote_numbers");

/// The repositories of GitHub that each server announces, by the server's ID, the repository and
/// the branch: the ID of the channel, and whether the releases are announced, too.
const WATCHES: TableDefinition<(u64, &str, &str), (u64, bool)> =
    TableDefinition::new("github_watches");

/// The newest commit that PuppyBot saw of each branch, by the repository and the branch.
const SEEN_COMMITS: TableDefinition<(&str, &str), &str> = TableDefinition::new("github_commits");

/// The ID of the newest release that PuppyBot saw of each repository.
const SEEN_RELEASES: TableDefinition<&str, u64> = TableDefinition::new("github_releases");

/// The settings of the servers. [`Storage`] keeps them; tests put in settings that fail.
pub trait Settings {
    /// The language chosen for `guild`, none if nobody chose one yet.
    ///
    /// # Errors
    ///
    /// [`Error`] if the settings can't be read.
    fn language(&self, guild: Id<GuildMarker>) -> Result<Option<Lang>, Error>;

    /// Chooses `lang` for `guild`.
    ///
    /// # Errors
    ///
    /// [`Error`] if the settings can't be saved, e.g. as the disk is full.
    fn set_language(&self, guild: Id<GuildMarker>, lang: Lang) -> Result<(), Error>;

    /// The prefix of the text commands chosen for `guild`, none if nobody chose one yet.
    ///
    /// # Errors
    ///
    /// [`Error`] if the settings can't be read.
    fn prefix(&self, guild: Id<GuildMarker>) -> Result<Option<String>, Error>;

    /// Chooses `prefix` for the text commands on `guild`, which
    /// [`text_command::is_valid_prefix`] accepts.
    ///
    /// # Errors
    ///
    /// [`Error`] if the settings can't be saved, e.g. as the disk is full.
    fn set_prefix(&self, guild: Id<GuildMarker>, prefix: &str) -> Result<(), Error>;
}

/// A message that someone saved as a quote, with what PuppyBot shows of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quote {
    /// The ID of the person who wrote it.
    pub author: u64,
    /// Their name when it was saved.
    pub name: String,
    /// The ID of the channel of the message.
    pub channel: u64,
    /// The ID of the message.
    pub message: u64,
    /// Its text, see [`puppybot_core::quotes::quote`].
    pub text: String,
}

/// What saving a quote did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// The message is the quote with this number now.
    New(u32),
    /// The message is the quote with this number already.
    Already(u32),
    /// The server has [`MAX_QUOTES`] quotes, or gave all numbers.
    Full,
}

/// What deleting a quote did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deleted {
    /// It is gone.
    Yes,
    /// Someone else wrote it.
    NotTheirs,
    /// There is no quote with this number.
    Missing,
}

/// The quotes of the servers. [`Storage`] keeps them; tests put in quotes that fail.
pub trait Quotes {
    /// Saves `quote` on `guild`, with the next number.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be saved, e.g. as the disk is full.
    fn save_quote(&self, guild: Id<GuildMarker>, quote: &Quote) -> Result<Saved, Error>;

    /// The quote `number` of `guild`, none if it has none.
    ///
    /// # Errors
    ///
    /// [`Error`] if the quotes can't be read.
    fn quote(&self, guild: Id<GuildMarker>, number: u32) -> Result<Option<Quote>, Error>;

    /// A quote of `guild`, with its number, none if it has none: `random(n)` chooses one of the
    /// `n`, from 1, and without randomness it is the first.
    ///
    /// # Errors
    ///
    /// [`Error`] if the quotes can't be read.
    fn random_quote(
        &self,
        guild: Id<GuildMarker>,
        random: &mut dyn FnMut(u32) -> Option<u32>,
    ) -> Result<Option<(u32, Quote)>, Error>;

    /// Deletes the quote `number` of `guild`, but only if `author` wrote it, if given.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be deleted, e.g. as the disk is full.
    fn delete_quote(
        &self,
        guild: Id<GuildMarker>,
        number: u32,
        author: Option<u64>,
    ) -> Result<Deleted, Error>;

    /// Deletes the quote of the message `message` on `guild`, as the message is gone, and
    /// returns its number, if there was one.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be deleted, e.g. as the disk is full.
    fn forget_message(
        &self,
        guild: Id<GuildMarker>,
        message: Id<MessageMarker>,
    ) -> Result<Option<u32>, Error>;
}

/// A repository of GitHub that a server announces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    /// The repository, `owner/name`, as GitHub writes it.
    pub repo: String,
    /// The branch.
    pub branch: String,
    /// The ID of the channel.
    pub channel: u64,
    /// Whether its releases are announced, too.
    pub releases: bool,
}

/// What PuppyBot saw of a branch and its repository when someone added it: from there on, it
/// announces what is new.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// The hash of the newest commit of the branch, none if it has none.
    pub commit: Option<String>,
    /// The ID of the newest release of the repository, 0 if it has none, or none if its
    /// releases aren't announced.
    pub release: Option<u64>,
}

/// What adding a repository did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watched {
    /// The server announces it now.
    New,
    /// The server announced it already, and now in that channel, with releases or without.
    Changed,
    /// The server announces [`MAX_REPOS`] already.
    Full,
}

/// The repositories of GitHub that the servers announce. [`Storage`] keeps them; tests put in
/// some that fail.
pub trait Watches {
    /// The repositories that `guild` announces, or all servers', if it is none, with the
    /// servers.
    ///
    /// # Errors
    ///
    /// [`Error`] if they can't be read.
    fn watches(
        &self,
        guild: Option<Id<GuildMarker>>,
    ) -> Result<Vec<(Id<GuildMarker>, Watch)>, Error>;

    /// Announces `watch` on `guild`, or changes the channel and the releases of the same
    /// repository and branch there, and notes what PuppyBot saw of it now, `seen`, unless it
    /// noted something already, e.g. for another server.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be saved, e.g. as the disk is full.
    fn add_watch(
        &self,
        guild: Id<GuildMarker>,
        watch: &Watch,
        seen: &Seen,
    ) -> Result<Watched, Error>;

    /// Stops announcing `repo`, in any case, on all its branches on `guild`, and returns whether
    /// it did. What PuppyBot saw of it goes, too, if no server announces it any more.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be saved, e.g. as the disk is full.
    fn remove_watch(&self, guild: Id<GuildMarker>, repo: &str) -> Result<bool, Error>;

    /// Notes `commit` as the newest of `branch` of `repo`, and returns the one noted before.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be saved, e.g. as the disk is full.
    fn swap_commit(&self, repo: &str, branch: &str, commit: &str) -> Result<Option<String>, Error>;

    /// Notes the release `id` as the newest of `repo`, and returns the one noted before.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be saved, e.g. as the disk is full.
    fn swap_release(&self, repo: &str, id: u64) -> Result<Option<u64>, Error>;
}

/// All that PuppyBot stores: the settings, the quotes and the repositories that it announces.
pub trait Store: Settings + Quotes + Watches {}

impl<T: Settings + Quotes + Watches> Store for T {}

/// The database.
pub struct Storage {
    database: Database,
}

/// Why the database can't be used.
#[derive(Debug)]
pub enum Error {
    /// It can't be opened, e.g. as another PuppyBot has it open, as PuppyBot may not write the
    /// file, or as the file isn't a database or was damaged.
    Open(redb::DatabaseError),
    /// Its data have a version that this PuppyBot doesn't know, e.g. of a newer PuppyBot.
    Version(u64),
    /// Reading or writing failed.
    Access(redb::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(error) => write!(f, "can't open it: {error}"),
            Self::Version(version) => write!(
                f,
                "its data have the version {version}, probably of a newer PuppyBot: this one knows only the version {VERSION}"
            ),
            Self::Access(error) => write!(f, "can't read or write it: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl Storage {
    /// Opens the database in the file at `path`, and creates it if the file is missing or empty.
    /// The file must be on a disk of this machine: two programs can't share it.
    ///
    /// # Errors
    ///
    /// [`Error`] if it can't be opened, or if its data have a version that this PuppyBot doesn't
    /// know.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let database = Builder::new()
            // A damaged file is an error, which a plain open may not notice and panic later.
            .set_check_integrity_on_open(true)
            .create(path)
            .map_err(Error::Open)?;
        Self::prepare(database)
    }

    /// A database in memory, for tests.
    ///
    /// # Panics
    ///
    /// If redb can't create it, which would be a bug.
    #[cfg(test)]
    #[must_use]
    pub fn in_memory() -> Self {
        let backend = redb::backends::InMemoryBackend::new();
        Self::prepare(Database::builder().create_with_backend(backend).unwrap()).unwrap()
    }

    /// Checks the version of the data in `database`, and stores it with the tables if the
    /// database is new.
    fn prepare(database: Database) -> Result<Self, Error> {
        match prepare(&database).map_err(Error::Access)? {
            VERSION => Ok(Self { database }),
            other => Err(Error::Version(other)),
        }
    }
}

impl Settings for Storage {
    fn language(&self, guild: Id<GuildMarker>) -> Result<Option<Lang>, Error> {
        language(&self.database, guild.get()).map_err(Error::Access)
    }

    fn set_language(&self, guild: Id<GuildMarker>, lang: Lang) -> Result<(), Error> {
        set(&self.database, LANGUAGES, guild.get(), lang.code()).map_err(Error::Access)
    }

    fn prefix(&self, guild: Id<GuildMarker>) -> Result<Option<String>, Error> {
        prefix(&self.database, guild.get()).map_err(Error::Access)
    }

    fn set_prefix(&self, guild: Id<GuildMarker>, prefix: &str) -> Result<(), Error> {
        set(&self.database, PREFIXES, guild.get(), prefix).map_err(Error::Access)
    }
}

impl Quotes for Storage {
    fn save_quote(&self, guild: Id<GuildMarker>, quote: &Quote) -> Result<Saved, Error> {
        save_quote(&self.database, guild.get(), quote, MAX_QUOTES).map_err(Error::Access)
    }

    fn quote(&self, guild: Id<GuildMarker>, number: u32) -> Result<Option<Quote>, Error> {
        quote(&self.database, guild.get(), number).map_err(Error::Access)
    }

    fn random_quote(
        &self,
        guild: Id<GuildMarker>,
        random: &mut dyn FnMut(u32) -> Option<u32>,
    ) -> Result<Option<(u32, Quote)>, Error> {
        random_quote(&self.database, guild.get(), random).map_err(Error::Access)
    }

    fn delete_quote(
        &self,
        guild: Id<GuildMarker>,
        number: u32,
        author: Option<u64>,
    ) -> Result<Deleted, Error> {
        delete_quote(&self.database, guild.get(), number, author).map_err(Error::Access)
    }

    fn forget_message(
        &self,
        guild: Id<GuildMarker>,
        message: Id<MessageMarker>,
    ) -> Result<Option<u32>, Error> {
        forget_message(&self.database, guild.get(), message.get()).map_err(Error::Access)
    }
}

impl Watches for Storage {
    fn watches(
        &self,
        guild: Option<Id<GuildMarker>>,
    ) -> Result<Vec<(Id<GuildMarker>, Watch)>, Error> {
        watches(&self.database, guild.map(Id::get)).map_err(Error::Access)
    }

    fn add_watch(
        &self,
        guild: Id<GuildMarker>,
        watch: &Watch,
        seen: &Seen,
    ) -> Result<Watched, Error> {
        add_watch(&self.database, guild.get(), watch, seen, MAX_REPOS).map_err(Error::Access)
    }

    fn remove_watch(&self, guild: Id<GuildMarker>, repo: &str) -> Result<bool, Error> {
        remove_watch(&self.database, guild.get(), repo).map_err(Error::Access)
    }

    fn swap_commit(&self, repo: &str, branch: &str, commit: &str) -> Result<Option<String>, Error> {
        swap_commit(&self.database, repo, branch, commit).map_err(Error::Access)
    }

    fn swap_release(&self, repo: &str, id: u64) -> Result<Option<u64>, Error> {
        swap_release(&self.database, repo, id).map_err(Error::Access)
    }
}

/// The version of the data in `database`. A new database gets [`VERSION`] and the tables, so
/// that reading finds them; one of another version stays as it is.
fn prepare(database: &Database) -> Result<u64, redb::Error> {
    let write = database.begin_write()?;
    let stored = write
        .open_table(META)?
        .get(VERSION_KEY)?
        .map(|version| version.value());
    match stored {
        Some(VERSION) => {}
        Some(other) => {
            write.abort()?;
            return Ok(other);
        }
        None => {
            write.open_table(META)?.insert(VERSION_KEY, VERSION)?;
        }
    }
    // Also in a database of an earlier PuppyBot of this version, which didn't have all yet.
    write.open_table(LANGUAGES)?;
    write.open_table(PREFIXES)?;
    write.open_table(QUOTES)?;
    write.open_table(QUOTED)?;
    write.open_table(QUOTE_NUMBERS)?;
    write.open_table(WATCHES)?;
    write.open_table(SEEN_COMMITS)?;
    write.open_table(SEEN_RELEASES)?;
    write.commit()?;
    Ok(VERSION)
}

/// The language chosen for the server `guild`. A code that this PuppyBot doesn't know counts as
/// none.
fn language(database: &Database, guild: u64) -> Result<Option<Lang>, redb::Error> {
    let read = database.begin_read()?;
    let languages = read.open_table(LANGUAGES)?;
    let code = languages.get(guild)?;
    Ok(code.and_then(|code| Lang::from_code(code.value())))
}

/// The prefix chosen for the server `guild`. One that this PuppyBot wouldn't take counts as none.
fn prefix(database: &Database, guild: u64) -> Result<Option<String>, redb::Error> {
    let read = database.begin_read()?;
    let prefixes = read.open_table(PREFIXES)?;
    let prefix = prefixes.get(guild)?;
    Ok(prefix
        .map(|prefix| prefix.value().to_owned())
        .filter(|prefix| text_command::is_valid_prefix(prefix)))
}

/// The quotes of the server `guild`, from the first to the last.
fn of_server(guild: u64) -> std::ops::RangeInclusive<(u64, u32)> {
    (guild, 0)..=(guild, u32::MAX)
}

/// The quote of a record of [`QUOTES`].
fn quote_of((author, channel, message, name, text): QuoteRecord<'_>) -> Quote {
    Quote {
        author,
        name: name.to_owned(),
        channel,
        message,
        text: text.to_owned(),
    }
}

/// Saves `quote` on the server `guild`, with the next number, if it has fewer than `most`.
fn save_quote(
    database: &Database,
    guild: u64,
    quote: &Quote,
    most: u32,
) -> Result<Saved, redb::Error> {
    let write = database.begin_write()?;
    let saved = add_quote(&write, guild, quote, most)?;
    write.commit()?;
    Ok(saved)
}

/// Adds `quote` to the quotes of `guild` in `write`, see [`save_quote`].
fn add_quote(
    write: &WriteTransaction,
    guild: u64,
    quote: &Quote,
    most: u32,
) -> Result<Saved, redb::Error> {
    let mut by_message = write.open_table(QUOTED)?;
    if let Some(number) = by_message.get((guild, quote.message))? {
        return Ok(Saved::Already(number.value()));
    }
    let mut quotes = write.open_table(QUOTES)?;
    let count = quotes.range(of_server(guild))?.count();
    let mut numbers = write.open_table(QUOTE_NUMBERS)?;
    let last = numbers.get(guild)?.map_or(0, |last| last.value());
    let room = usize::try_from(most).is_ok_and(|most| count < most);
    let Some(number) = last.checked_add(1).filter(|_| room) else {
        return Ok(Saved::Full);
    };
    numbers.insert(guild, number)?;
    let record = (
        quote.author,
        quote.channel,
        quote.message,
        quote.name.as_str(),
        quote.text.as_str(),
    );
    quotes.insert((guild, number), record)?;
    by_message.insert((guild, quote.message), number)?;
    Ok(Saved::New(number))
}

/// The quote `number` of the server `guild`.
fn quote(database: &Database, guild: u64, number: u32) -> Result<Option<Quote>, redb::Error> {
    let read = database.begin_read()?;
    let quotes = read.open_table(QUOTES)?;
    let quote = quotes.get((guild, number))?;
    Ok(quote.map(|quote| quote_of(quote.value())))
}

/// A quote of the server `guild` that `random` chooses, see [`Quotes::random_quote`].
fn random_quote(
    database: &Database,
    guild: u64,
    random: &mut dyn FnMut(u32) -> Option<u32>,
) -> Result<Option<(u32, Quote)>, redb::Error> {
    let read = database.begin_read()?;
    let quotes = read.open_table(QUOTES)?;
    let count = quotes.range(of_server(guild))?.count();
    let Some(count) = u32::try_from(count).ok().filter(|&count| count > 0) else {
        return Ok(None);
    };
    let chosen = random(count).unwrap_or(1).clamp(1, count) - 1;
    let entry = quotes
        .range(of_server(guild))?
        .nth(usize::try_from(chosen).unwrap_or_default())
        .transpose()?;
    Ok(entry.map(|(key, record)| (key.value().1, quote_of(record.value()))))
}

/// Deletes the quote `number` of the server `guild`, see [`Quotes::delete_quote`].
fn delete_quote(
    database: &Database,
    guild: u64,
    number: u32,
    author: Option<u64>,
) -> Result<Deleted, redb::Error> {
    let write = database.begin_write()?;
    let deleted = remove_quote(&write, guild, number, author)?;
    write.commit()?;
    Ok(deleted)
}

/// Removes the quote `number` of `guild` in `write`, see [`Quotes::delete_quote`].
fn remove_quote(
    write: &WriteTransaction,
    guild: u64,
    number: u32,
    author: Option<u64>,
) -> Result<Deleted, redb::Error> {
    let mut quotes = write.open_table(QUOTES)?;
    let found = quotes.get((guild, number))?.map(|record| {
        let (by, _, message, _, _) = record.value();
        (by, message)
    });
    let Some((by, message)) = found else {
        return Ok(Deleted::Missing);
    };
    if author.is_some_and(|author| author != by) {
        return Ok(Deleted::NotTheirs);
    }
    quotes.remove((guild, number))?;
    write.open_table(QUOTED)?.remove((guild, message))?;
    Ok(Deleted::Yes)
}

/// Deletes the quote of the message `message` on the server `guild`, and returns its number.
fn forget_message(
    database: &Database,
    guild: u64,
    message: u64,
) -> Result<Option<u32>, redb::Error> {
    // Most messages that go aren't quotes: that needs only a read, which doesn't write to the disk.
    let read = database.begin_read()?;
    let number = read
        .open_table(QUOTED)?
        .get((guild, message))?
        .map(|number| number.value());
    let Some(number) = number else {
        return Ok(None);
    };
    let write = database.begin_write()?;
    write.open_table(QUOTED)?.remove((guild, message))?;
    write.open_table(QUOTES)?.remove((guild, number))?;
    write.commit()?;
    Ok(Some(number))
}

/// The repositories that `guild` announces, or all, see [`Watches::watches`]. A server's ID of 0,
/// which Discord doesn't give, counts as none.
fn watches(
    database: &Database,
    guild: Option<u64>,
) -> Result<Vec<(Id<GuildMarker>, Watch)>, redb::Error> {
    let read = database.begin_read()?;
    let table = read.open_table(WATCHES)?;
    let mut found = Vec::new();
    for entry in table.iter()? {
        let (key, value) = entry?;
        let ((server, repo, branch), (channel, releases)) = (key.value(), value.value());
        let wanted = guild.is_none_or(|guild| guild == server);
        if let Some(server) = Id::new_checked(server).filter(|_| wanted) {
            let watch = Watch {
                repo: repo.to_owned(),
                branch: branch.to_owned(),
                channel,
                releases,
            };
            found.push((server, watch));
        }
    }
    Ok(found)
}

/// Announces `watch` on `guild`, see [`Watches::add_watch`], if it has fewer than `most`.
fn add_watch(
    database: &Database,
    guild: u64,
    watch: &Watch,
    seen: &Seen,
    most: usize,
) -> Result<Watched, redb::Error> {
    let write = database.begin_write()?;
    let watched = add_watch_in(&write, guild, watch, seen, most)?;
    write.commit()?;
    Ok(watched)
}

/// Adds `watch` to the repositories of `guild` in `write`, see [`add_watch`].
fn add_watch_in(
    write: &WriteTransaction,
    guild: u64,
    watch: &Watch,
    seen: &Seen,
    most: usize,
) -> Result<Watched, redb::Error> {
    let mut table = write.open_table(WATCHES)?;
    let key = (guild, watch.repo.as_str(), watch.branch.as_str());
    let known = table.get(key)?.is_some();
    let count = table.range(of_guild(guild))?.count();
    let watched = match (known, count < most) {
        (true, _) => Watched::Changed,
        (false, true) => Watched::New,
        (false, false) => return Ok(Watched::Full),
    };
    table.insert(key, (watch.channel, watch.releases))?;
    let mut commits = write.open_table(SEEN_COMMITS)?;
    let branch = (watch.repo.as_str(), watch.branch.as_str());
    let noted = commits.get(branch)?.is_some();
    if let Some(commit) = seen.commit.as_deref().filter(|_| !noted) {
        commits.insert(branch, commit)?;
    }
    let mut releases = write.open_table(SEEN_RELEASES)?;
    let noted = releases.get(watch.repo.as_str())?.is_some();
    if let Some(release) = seen.release.filter(|_| !noted) {
        releases.insert(watch.repo.as_str(), release)?;
    }
    Ok(watched)
}

/// The keys of the repositories of `guild` in [`WATCHES`].
fn of_guild(guild: u64) -> std::ops::Range<(u64, &'static str, &'static str)> {
    (guild, "", "")..(guild.saturating_add(1), "", "")
}

/// Stops announcing `repo` on `guild`, see [`Watches::remove_watch`].
fn remove_watch(database: &Database, guild: u64, repo: &str) -> Result<bool, redb::Error> {
    let write = database.begin_write()?;
    let removed = remove_watch_in(&write, guild, repo)?;
    write.commit()?;
    Ok(removed)
}

/// Removes the watches of `repo` on `guild` in `write`, see [`remove_watch`].
fn remove_watch_in(write: &WriteTransaction, guild: u64, repo: &str) -> Result<bool, redb::Error> {
    let mut table = write.open_table(WATCHES)?;
    let mut gone = Vec::new();
    for entry in table.range(of_guild(guild))? {
        let (key, _) = entry?;
        let (_, name, branch) = key.value();
        if name.eq_ignore_ascii_case(repo) {
            gone.push((name.to_owned(), branch.to_owned()));
        }
    }
    for (name, branch) in &gone {
        table.remove((guild, name.as_str(), branch.as_str()))?;
    }
    // What PuppyBot saw of what no server announces any more.
    let mut branches = BTreeSet::new();
    let mut with_releases = BTreeSet::new();
    for entry in table.iter()? {
        let (key, value) = entry?;
        let (_, name, branch) = key.value();
        branches.insert((name.to_owned(), branch.to_owned()));
        if value.value().1 {
            with_releases.insert(name.to_owned());
        }
    }
    let mut commits = write.open_table(SEEN_COMMITS)?;
    let mut releases = write.open_table(SEEN_RELEASES)?;
    for (name, branch) in &gone {
        if !branches.contains(&(name.clone(), branch.clone())) {
            commits.remove((name.as_str(), branch.as_str()))?;
        }
        if !with_releases.contains(name) {
            releases.remove(name.as_str())?;
        }
    }
    Ok(!gone.is_empty())
}

/// Notes `commit` as the newest of `branch` of `repo`, see [`Watches::swap_commit`]. An unchanged
/// one needs no write to the disk.
fn swap_commit(
    database: &Database,
    repo: &str,
    branch: &str,
    commit: &str,
) -> Result<Option<String>, redb::Error> {
    let read = database.begin_read()?;
    let before = read
        .open_table(SEEN_COMMITS)?
        .get((repo, branch))?
        .map(|sha| sha.value().to_owned());
    if before.as_deref() != Some(commit) {
        let write = database.begin_write()?;
        write
            .open_table(SEEN_COMMITS)?
            .insert((repo, branch), commit)?;
        write.commit()?;
    }
    Ok(before)
}

/// Notes the release `id` as the newest of `repo`, see [`Watches::swap_release`]. An unchanged
/// one needs no write to the disk.
fn swap_release(database: &Database, repo: &str, id: u64) -> Result<Option<u64>, redb::Error> {
    let read = database.begin_read()?;
    let before = read
        .open_table(SEEN_RELEASES)?
        .get(repo)?
        .map(|id| id.value());
    if before != Some(id) {
        let write = database.begin_write()?;
        write.open_table(SEEN_RELEASES)?.insert(repo, id)?;
        write.commit()?;
    }
    Ok(before)
}

/// Stores `value` for the server `guild` in `table`.
fn set(
    database: &Database,
    table: TableDefinition<u64, &str>,
    guild: u64,
    value: &str,
) -> Result<(), redb::Error> {
    let write = database.begin_write()?;
    write.open_table(table)?.insert(guild, value)?;
    write.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const GUILD: Id<GuildMarker> = Id::new(1_100_000_000_000_000_001);

    #[test]
    fn languages_of_servers() {
        let storage = Storage::in_memory();
        assert_eq!(storage.language(GUILD).unwrap(), None);
        storage.set_language(GUILD, Lang::De).unwrap();
        assert_eq!(storage.language(GUILD).unwrap(), Some(Lang::De));
        storage.set_language(GUILD, Lang::En).unwrap();
        assert_eq!(storage.language(GUILD).unwrap(), Some(Lang::En));
        // Each server has its own.
        assert_eq!(storage.language(Id::new(2)).unwrap(), None);
    }

    #[test]
    fn prefixes_of_servers() {
        let storage = Storage::in_memory();
        assert_eq!(storage.prefix(GUILD).unwrap(), None);
        storage.set_prefix(GUILD, "pb?").unwrap();
        assert_eq!(storage.prefix(GUILD).unwrap().as_deref(), Some("pb?"));
        assert_eq!(storage.prefix(Id::new(2)).unwrap(), None);
        // One that this PuppyBot wouldn't take, e.g. from a damaged file, counts as none.
        storage.set_prefix(GUILD, "a b").unwrap();
        assert_eq!(storage.prefix(GUILD).unwrap(), None);
    }

    /// The quote of the message `message`, by the person `author`.
    fn quote_by(author: u64, message: u64) -> Quote {
        Quote {
            author,
            name: "Puppy".to_owned(),
            channel: 4,
            message,
            text: format!("Wuff {message}!"),
        }
    }

    #[test]
    fn quotes_of_servers() {
        let storage = Storage::in_memory();
        assert_eq!(storage.quote(GUILD, 1).unwrap(), None);
        assert_eq!(storage.random_quote(GUILD, &mut |_| Some(1)).unwrap(), None);
        for (message, number) in [(10, 1), (20, 2), (30, 3)] {
            let saved = storage.save_quote(GUILD, &quote_by(5, message)).unwrap();
            assert_eq!(saved, Saved::New(number));
        }
        // A message once.
        let again = storage.save_quote(GUILD, &quote_by(6, 20)).unwrap();
        assert_eq!(again, Saved::Already(2));
        assert_eq!(storage.quote(GUILD, 2).unwrap(), Some(quote_by(5, 20)));
        // Chosen at random, from 1; the first without randomness, the last for too large a number.
        for (chosen, number) in [(Some(2), 2), (None, 1), (Some(99), 3)] {
            let random = storage.random_quote(GUILD, &mut |count| {
                assert_eq!(count, 3);
                chosen
            });
            assert_eq!(random.unwrap().map(|(number, _)| number), Some(number));
        }
        // Each server has its own.
        assert_eq!(storage.quote(Id::new(2), 1).unwrap(), None);
        assert_eq!(
            storage.save_quote(Id::new(2), &quote_by(5, 20)).unwrap(),
            Saved::New(1)
        );

        // Only by the person who wrote it, or by anyone who may.
        let delete = |number, author| storage.delete_quote(GUILD, number, author).unwrap();
        assert_eq!(delete(2, Some(6)), Deleted::NotTheirs);
        assert_eq!(delete(2, Some(5)), Deleted::Yes);
        assert_eq!(delete(2, None), Deleted::Missing);
        assert_eq!(delete(3, None), Deleted::Yes);
        assert_eq!(storage.quote(GUILD, 2).unwrap(), None);
        // The number stays given, and the message can be a quote again.
        assert_eq!(
            storage.save_quote(GUILD, &quote_by(5, 20)).unwrap(),
            Saved::New(4)
        );
        // The message is gone, and its quote with it.
        assert_eq!(storage.forget_message(GUILD, Id::new(10)).unwrap(), Some(1));
        assert_eq!(storage.forget_message(GUILD, Id::new(10)).unwrap(), None);
        assert_eq!(storage.quote(GUILD, 1).unwrap(), None);
        assert_eq!(storage.forget_message(GUILD, Id::new(99)).unwrap(), None);
    }

    #[test]
    fn the_most_quotes_of_a_server() {
        let storage = Storage::in_memory();
        let guild = GUILD.get();
        for message in [1, 2] {
            let saved = save_quote(&storage.database, guild, &quote_by(5, message), 2).unwrap();
            assert!(matches!(saved, Saved::New(_)), "{saved:?}");
        }
        let full = save_quote(&storage.database, guild, &quote_by(5, 3), 2).unwrap();
        assert_eq!(full, Saved::Full);
        // Or all numbers are given.
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(QUOTE_NUMBERS)
            .unwrap()
            .insert(guild, u32::MAX)
            .unwrap();
        write.commit().unwrap();
        assert_eq!(
            storage.save_quote(GUILD, &quote_by(5, 3)).unwrap(),
            Saved::Full
        );
    }

    /// The watch of `repo` on `branch` in the channel 4.
    fn watch(repo: &str, branch: &str, releases: bool) -> Watch {
        Watch {
            repo: repo.to_owned(),
            branch: branch.to_owned(),
            channel: 4,
            releases,
        }
    }

    /// What PuppyBot saw when someone added a repository: the commit `commit`, and the
    /// release 7.
    fn seen(commit: &str) -> Seen {
        Seen {
            commit: Some(commit.to_owned()),
            release: Some(7),
        }
    }

    #[test]
    fn repositories_that_servers_announce() {
        let storage = Storage::in_memory();
        let other = Id::new(2);
        assert_eq!(storage.watches(None).unwrap(), []);
        let puppybot = watch("PurplePuppyProject/PuppyBot", "main", true);
        let add =
            |guild, watch: &Watch, seen: &Seen| storage.add_watch(guild, watch, seen).unwrap();
        assert_eq!(add(GUILD, &puppybot, &seen("aaaaaaa")), Watched::New);
        assert_eq!(
            add(GUILD, &watch("a/b", "dev", false), &Seen::default()),
            Watched::New
        );
        // For another server: what PuppyBot saw stays.
        assert_eq!(add(other, &puppybot, &seen("bbbbbbb")), Watched::New);
        // The same repository and branch again: in another channel now.
        let moved = Watch {
            channel: 5,
            ..puppybot.clone()
        };
        assert_eq!(add(GUILD, &moved, &seen("ccccccc")), Watched::Changed);
        let watches = storage.watches(Some(GUILD)).unwrap();
        assert_eq!(
            watches,
            [(GUILD, moved), (GUILD, watch("a/b", "dev", false))]
        );
        assert_eq!(storage.watches(None).unwrap().len(), 3);

        // What PuppyBot saw, and sees now.
        let repo = "PurplePuppyProject/PuppyBot";
        let swap = |sha| storage.swap_commit(repo, "main", sha).unwrap();
        assert_eq!(swap("ddddddd").as_deref(), Some("aaaaaaa"));
        assert_eq!(swap("ddddddd").as_deref(), Some("ddddddd"));
        assert_eq!(storage.swap_release(repo, 9).unwrap(), Some(7));
        assert_eq!(storage.swap_release(repo, 9).unwrap(), Some(9));
        assert_eq!(storage.swap_commit("a/b", "dev", "eeeeeee").unwrap(), None);
        assert_eq!(storage.swap_release("a/b", 1).unwrap(), None);

        // In any case; what no server announces any more goes, what one still does stays.
        assert!(storage.remove_watch(GUILD, "a/B").unwrap());
        assert!(!storage.remove_watch(GUILD, "a/b").unwrap());
        assert_eq!(storage.swap_commit("a/b", "dev", "fffffff").unwrap(), None);
        assert_eq!(storage.swap_release("a/b", 2).unwrap(), None);
        assert!(
            storage
                .remove_watch(GUILD, "purplepuppyproject/puppybot")
                .unwrap()
        );
        assert_eq!(swap("ddddddd").as_deref(), Some("ddddddd"));
        assert_eq!(storage.watches(None).unwrap(), [(other, puppybot)]);
    }

    #[test]
    fn the_most_repositories_of_a_server() {
        let storage = Storage::in_memory();
        let guild = GUILD.get();
        let add = |repo, releases| {
            let seen = seen("aaaaaaa");
            add_watch(
                &storage.database,
                guild,
                &watch(repo, "main", releases),
                &seen,
                2,
            )
            .unwrap()
        };
        assert_eq!(add("a/b", false), Watched::New);
        assert_eq!(add("a/c", false), Watched::New);
        assert_eq!(add("a/d", false), Watched::Full);
        // Nothing noted of it.
        assert_eq!(storage.swap_commit("a/d", "main", "bbbbbbb").unwrap(), None);
        // A change of one that it has is no new one.
        assert_eq!(add("a/b", true), Watched::Changed);
        // A server's ID of 0 is no server.
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(WATCHES)
            .unwrap()
            .insert((0, "a/e", "main"), (4, false))
            .unwrap();
        write.commit().unwrap();
        assert_eq!(storage.watches(None).unwrap().len(), 2);
    }

    #[test]
    fn a_database_without_all_tables_gets_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("puppybot.redb");
        // As the first PuppyBot with a database stored it: the version, and only the languages.
        let database = Database::create(&path).unwrap();
        let write = database.begin_write().unwrap();
        write
            .open_table(META)
            .unwrap()
            .insert(VERSION_KEY, VERSION)
            .unwrap();
        write
            .open_table(LANGUAGES)
            .unwrap()
            .insert(GUILD.get(), "de")
            .unwrap();
        write.commit().unwrap();
        drop(database);

        let storage = Storage::open(&path).unwrap();
        assert_eq!(storage.language(GUILD).unwrap(), Some(Lang::De));
        assert_eq!(storage.prefix(GUILD).unwrap(), None);
        assert_eq!(storage.quote(GUILD, 1).unwrap(), None);
        assert_eq!(storage.watches(None).unwrap(), []);
        assert_eq!(storage.swap_release("a/b", 1).unwrap(), None);
    }

    #[test]
    fn the_file_keeps_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("puppybot.redb");
        Storage::open(&path)
            .unwrap()
            .set_language(GUILD, Lang::De)
            .unwrap();
        let storage = Storage::open(&path).unwrap();
        assert_eq!(storage.language(GUILD).unwrap(), Some(Lang::De));
        // Only one program at a time.
        let error = Storage::open(&path).err().unwrap().to_string();
        assert!(error.starts_with("can't open it: "), "{error}");
    }

    #[test]
    fn an_empty_file_becomes_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("puppybot.redb");
        fs::write(&path, b"").unwrap();
        let storage = Storage::open(&path).unwrap();
        assert_eq!(storage.language(GUILD).unwrap(), None);
    }

    #[test]
    fn files_that_are_no_database() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("notes.txt");
        fs::write(
            &other,
            b"not a database, but long enough to hold a header of one",
        )
        .unwrap();
        let missing = dir.path().join("missing/puppybot.redb");
        for path in [other.as_path(), missing.as_path(), dir.path()] {
            let error = Storage::open(path).err().unwrap().to_string();
            assert!(error.starts_with("can't open it: "), "{path:?}: {error}");
        }
    }

    #[test]
    fn data_of_another_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("puppybot.redb");
        // As a newer PuppyBot would store them.
        let database = Database::create(&path).unwrap();
        let write = database.begin_write().unwrap();
        write
            .open_table(META)
            .unwrap()
            .insert(VERSION_KEY, VERSION + 1)
            .unwrap();
        write.commit().unwrap();
        drop(database);

        let error = Storage::open(&path).err().unwrap().to_string();
        assert_eq!(
            error,
            "its data have the version 2, probably of a newer PuppyBot: this one knows only the version 1"
        );
        // Not changed: the newer PuppyBot still finds its own version, and no new tables.
        let database = Database::create(&path).unwrap();
        let read = database.begin_read().unwrap();
        let meta = read.open_table(META).unwrap();
        let version = meta.get(VERSION_KEY).unwrap();
        assert_eq!(version.map(|version| version.value()), Some(VERSION + 1));
        assert!(read.open_table(LANGUAGES).is_err());
    }

    #[test]
    fn codes_that_this_puppybot_does_not_know() {
        let storage = Storage::in_memory();
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(LANGUAGES)
            .unwrap()
            .insert(GUILD.get(), "tlh")
            .unwrap();
        write.commit().unwrap();
        assert_eq!(storage.language(GUILD).unwrap(), None);
    }

    #[test]
    fn errors_say_why() {
        let error = Error::Access(redb::Error::DatabaseAlreadyOpen).to_string();
        assert!(error.starts_with("can't read or write it: "), "{error}");
    }
}
