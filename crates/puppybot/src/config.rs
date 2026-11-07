//! The configuration file, in TOML, see Configuration in the README.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::files;
use crate::logging::LogConfig;

/// How large the configuration file may be, in bytes.
pub const MAX_SIZE: usize = 1024 * 1024;

/// The configuration of PuppyBot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The table `[discord]`.
    pub discord: DiscordConfig,
    /// The table `[log]`, which may be left out.
    #[serde(default)]
    pub log: LogConfig,
}

/// The table `[discord]` of the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DiscordConfig {
    /// The file with the token of the bot, e.g. a Docker secret. After [`parse`], a relative
    /// path in the file is relative to the directory of the configuration file.
    pub token_file: PathBuf,
}

/// A configuration file that PuppyBot can't use.
#[derive(Debug)]
pub enum Error {
    /// The file can't be read: it is missing, PuppyBot may not read it, or it is too large or not
    /// UTF-8.
    Read {
        /// The configuration file.
        path: PathBuf,
        /// Why it can't be read.
        source: io::Error,
    },
    /// The file isn't a valid configuration.
    Invalid {
        /// The configuration file.
        path: PathBuf,
        /// Where the file is wrong, and why.
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    f,
                    "can't read the configuration {}: {source}",
                    path.display()
                )
            }
            Self::Invalid { path, message } => {
                write!(
                    f,
                    "the configuration {} is invalid: {message}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// Reads the configuration file at `path`, see [`parse`].
///
/// # Errors
///
/// [`Error`] if the file can't be read, or isn't a valid configuration.
pub fn load(path: &Path) -> Result<Config, Error> {
    let text = files::read_limited(path, MAX_SIZE)
        .and_then(|bytes| {
            String::from_utf8(bytes)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-8"))
        })
        .map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })?;
    let dir = path.parent().unwrap_or(Path::new(""));
    parse(&text, dir).map_err(|message| Error::Invalid {
        path: path.to_owned(),
        message,
    })
}

/// Reads a configuration from its text. Relative paths in it become relative to `dir`, the
/// directory of the configuration file.
///
/// # Errors
///
/// A message that says where the text is wrong and why, e.g. a key that doesn't exist.
pub fn parse(text: &str, dir: &Path) -> Result<Config, String> {
    let mut config: Config = toml::from_str(text).map_err(|error| error.to_string())?;
    if config.discord.token_file.as_os_str().is_empty() {
        return Err("token-file in [discord] is empty".to_owned());
    }
    config.discord.token_file = dir.join(&config.discord.token_file);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::logging::{Format, Level};

    #[test]
    fn the_smallest_configuration() {
        let config = parse(
            "[discord]\ntoken-file = \"token\"\n",
            Path::new("/etc/puppybot"),
        );
        assert_eq!(
            config,
            Ok(Config {
                discord: DiscordConfig {
                    token_file: PathBuf::from("/etc/puppybot/token"),
                },
                log: LogConfig {
                    level: Level::Info,
                    format: Format::Text,
                },
            })
        );
    }

    #[test]
    fn every_setting() {
        let text = "\
[discord]
token-file = \"/run/secrets/discord_token\"

[log]
level = \"debug\"
format = \"json\"
";
        let config = parse(text, Path::new("/etc/puppybot")).unwrap();
        // An absolute path stays as it is.
        assert_eq!(
            config.discord.token_file,
            PathBuf::from("/run/secrets/discord_token")
        );
        assert_eq!(
            config.log,
            LogConfig {
                level: Level::Debug,
                format: Format::Json,
            }
        );
    }

    #[test]
    fn the_example_of_docker_compose() {
        let text = include_str!("../../../docker/example/config.toml");
        let config = parse(text, Path::new("/etc/puppybot")).unwrap();
        assert_eq!(
            config,
            Config {
                discord: DiscordConfig {
                    token_file: PathBuf::from("/run/secrets/discord_token"),
                },
                log: LogConfig::default(),
            }
        );
    }

    #[test]
    fn mistakes_name_their_place() {
        for (text, mistake) in [
            ("", "missing field `discord`"),
            ("[discord]\n", "missing field `token-file`"),
            (
                "[discord]\ntokn-file = \"t\"\n",
                "unknown field `tokn-file`, expected `token-file`",
            ),
            (
                "[discord]\ntoken-file = \"t\"\n[logs]\n",
                "unknown field `logs`, expected `discord` or `log`",
            ),
            (
                "[discord]\ntoken-file = \"t\"\n[log]\nlevel = \"loud\"\n",
                "unknown variant `loud`, expected one of `error`, `warn`, `info`, `debug`, `trace`",
            ),
            (
                "[discord]\ntoken-file = \"t\"\n[log]\nformat = \"xml\"\n",
                "unknown variant `xml`, expected `text` or `json`",
            ),
            ("[discord]\ntoken-file = 3\n", "invalid type: integer `3`"),
            ("[discord\n", "unclosed table, expected `]`"),
            (
                "[discord]\ntoken-file = \"\"\n",
                "token-file in [discord] is empty",
            ),
        ] {
            let message = parse(text, Path::new("/etc")).unwrap_err();
            assert!(message.contains(mistake), "{text:?}: {message}");
        }
        // The place, for the mistakes that the parser finds.
        let message = parse("[discord]\ntokn-file = \"t\"\n", Path::new("/etc")).unwrap_err();
        assert!(message.contains("line 2"), "{message}");
    }

    #[test]
    fn loads_a_file_with_paths_relative_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[discord]\ntoken-file = \"secrets/token\"\n").unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.discord.token_file, dir.path().join("secrets/token"));
        // A file in the working directory: relative to that.
        assert_eq!(
            parse("[discord]\ntoken-file = \"token\"\n", Path::new(""))
                .unwrap()
                .discord
                .token_file,
            PathBuf::from("token")
        );
    }

    #[test]
    fn files_that_can_not_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let error = load(&missing).unwrap_err().to_string();
        assert!(
            error.starts_with(&format!(
                "can't read the configuration {}: ",
                missing.display()
            )),
            "{error}"
        );

        let not_utf8 = dir.path().join("latin1.toml");
        fs::write(&not_utf8, b"# K\xf6ln\n").unwrap();
        assert_eq!(
            load(&not_utf8).unwrap_err().to_string(),
            format!(
                "can't read the configuration {}: not UTF-8",
                not_utf8.display()
            )
        );

        let large = dir.path().join("large.toml");
        fs::write(&large, vec![b'#'; MAX_SIZE + 1]).unwrap();
        assert_eq!(
            load(&large).unwrap_err().to_string(),
            format!(
                "can't read the configuration {}: larger than 1048576 bytes",
                large.display()
            )
        );
    }

    #[test]
    fn invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[discord]\n").unwrap();
        let error = load(&path).unwrap_err().to_string();
        assert!(
            error.starts_with(&format!(
                "the configuration {} is invalid: ",
                path.display()
            )),
            "{error}"
        );
        assert!(error.contains("missing field `token-file`"), "{error}");
    }
}
