//! The database of PuppyBot: the settings of each server, in one file of
//! [redb](https://www.redb.org), e.g. in a Docker volume.
//!
//! Its data have a [`VERSION`], so that a later PuppyBot can store them differently, and convert
//! the data of an earlier one when it opens them. A PuppyBot refuses data of a version it doesn't
//! know, e.g. after going back to an older release, instead of misreading them.
//!
//! The calls are short and the settings change rarely, so they run on the thread of the bot's
//! runtime; a write returns once its data are on the disk.

use std::fmt;
use std::path::Path;

use puppybot_core::i18n::Lang;
use puppybot_core::text_command;
use redb::{Builder, Database, ReadableDatabase as _, ReadableTable as _, TableDefinition};
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;

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
