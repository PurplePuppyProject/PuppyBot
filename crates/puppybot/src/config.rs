//! The configuration file, in TOML, see Configuration in the README.

use std::fmt;
use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::files;
use crate::logging::LogConfig;

/// How large the configuration file may be, in bytes.
pub const MAX_SIZE: usize = 1024 * 1024;

/// The database, where the Docker image has its volume, see the README.
pub const DEFAULT_DATABASE: &str = "/var/lib/puppybot/puppybot.redb";

/// The hosts that `gateway-url` and `api-url` may name: only this machine, so someone who may
/// change the configuration but not read the token file can't send the token elsewhere.
const LOOPBACK: [&str; 3] = ["127.0.0.1", "[::1]", "localhost"];

/// The configuration of PuppyBot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The table `[discord]`.
    pub discord: DiscordConfig,
    /// The table `[storage]`, which may be left out.
    #[serde(default)]
    pub storage: StorageConfig,
    /// The table `[log]`, which may be left out.
    #[serde(default)]
    pub log: LogConfig,
}

/// The table `[storage]` of the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct StorageConfig {
    /// The file of the database, which PuppyBot creates if it is missing. After [`parse`], a
    /// relative path in the file is relative to the directory of the configuration file.
    pub path: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from(DEFAULT_DATABASE),
        }
    }
}

/// The table `[discord]` of the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DiscordConfig {
    /// The file with the token of the bot, e.g. a Docker secret. After [`parse`], a relative
    /// path in the file is relative to the directory of the configuration file.
    pub token_file: PathBuf,
    /// The IDs of the servers that PuppyBot works on, at least one. It leaves all others.
    pub guilds: Vec<NonZeroU64>,
    /// For tests: Discord's gateway on this machine, e.g. `ws://127.0.0.1:1234`, instead of
    /// Discord's own.
    pub gateway_url: Option<String>,
    /// For tests: Discord's HTTP API on this machine, e.g. `http://127.0.0.1:1234`, instead of
    /// Discord's own.
    pub api_url: Option<String>,
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
    let discord = &mut config.discord;
    if discord.token_file.as_os_str().is_empty() {
        return Err("token-file in [discord] is empty".to_owned());
    }
    if discord.guilds.is_empty() {
        return Err("guilds in [discord] is empty: PuppyBot would work on no server".to_owned());
    }
    if let Some(url) = &discord.gateway_url {
        loopback("gateway-url", url, "ws")?;
    }
    if let Some(url) = &discord.api_url {
        loopback("api-url", url, "http")?;
    }
    discord.token_file = dir.join(&discord.token_file);
    let storage = &mut config.storage;
    if storage.path.as_os_str().is_empty() {
        return Err("path in [storage] is empty".to_owned());
    }
    storage.path = dir.join(&storage.path);
    Ok(config)
}

/// Whether `url`, the value of `key`, is `scheme://host` or `scheme://host:port` with a host of
/// [`LOOPBACK`].
fn loopback(key: &str, url: &str, scheme: &str) -> Result<(), String> {
    let address = url
        .strip_prefix(scheme)
        .and_then(|rest| rest.strip_prefix("://"))
        .unwrap_or_default();
    let on_this_machine = LOOPBACK
        .iter()
        .any(|host| match address.strip_prefix(host) {
            Some("") => true,
            Some(port) => port.strip_prefix(':').is_some_and(|port| {
                port.bytes().all(|byte| byte.is_ascii_digit()) && port.parse::<u16>().is_ok()
            }),
            None => false,
        });
    if on_this_machine {
        Ok(())
    } else {
        Err(format!(
            "{key} in [discord] is for tests, so it must be {scheme}:// with 127.0.0.1, [::1] or localhost, and a port or none, not {url:?}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::logging::{Format, Level};

    /// The `[discord]` of the tests, with the server 1.
    const DISCORD: &str = "[discord]\ntoken-file = \"token\"\nguilds = [1]\n";

    fn ids(ids: &[u64]) -> Vec<NonZeroU64> {
        ids.iter().filter_map(|&id| NonZeroU64::new(id)).collect()
    }

    #[test]
    fn the_smallest_configuration() {
        assert_eq!(
            parse(DISCORD, Path::new("/etc/puppybot")),
            Ok(Config {
                discord: DiscordConfig {
                    token_file: PathBuf::from("/etc/puppybot/token"),
                    guilds: ids(&[1]),
                    gateway_url: None,
                    api_url: None,
                },
                storage: StorageConfig {
                    path: PathBuf::from("/var/lib/puppybot/puppybot.redb"),
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
guilds = [1100000000000000001, 1100000000000000002]
gateway-url = \"ws://127.0.0.1:4000\"
api-url = \"http://localhost:4001\"

[storage]
path = \"data/puppybot.redb\"

[log]
level = \"debug\"
format = \"json\"
";
        let config = parse(text, Path::new("/etc/puppybot")).unwrap();
        assert_eq!(
            config.discord,
            DiscordConfig {
                // An absolute path stays as it is.
                token_file: PathBuf::from("/run/secrets/discord_token"),
                guilds: ids(&[1_100_000_000_000_000_001, 1_100_000_000_000_000_002]),
                gateway_url: Some("ws://127.0.0.1:4000".to_owned()),
                api_url: Some("http://localhost:4001".to_owned()),
            }
        );
        // A relative path is relative to the directory of the configuration file.
        assert_eq!(
            config.storage.path,
            PathBuf::from("/etc/puppybot/data/puppybot.redb")
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
            config.discord.token_file,
            PathBuf::from("/run/secrets/discord_token")
        );
        assert_eq!(config.discord.guilds.len(), 1);
        assert_eq!(
            (config.discord.gateway_url, config.discord.api_url),
            (None, None)
        );
        assert_eq!(config.storage, StorageConfig::default());
        assert_eq!(config.log, LogConfig::default());
    }

    #[test]
    fn addresses_for_tests_on_this_machine() {
        for (key, url) in [
            ("gateway-url", "ws://127.0.0.1"),
            ("gateway-url", "ws://[::1]:9"),
            ("gateway-url", "ws://localhost:65535"),
            ("api-url", "http://127.0.0.1:080"),
        ] {
            let text = format!("{DISCORD}{key} = {url:?}\n");
            assert!(parse(&text, Path::new("/etc")).is_ok(), "{text}");
        }
    }

    #[test]
    fn addresses_for_tests_elsewhere_are_errors() {
        for (key, url) in [
            ("gateway-url", "wss://127.0.0.1:9"),
            ("gateway-url", "ws://gateway.discord.gg"),
            ("gateway-url", "ws://127.0.0.10"),
            ("gateway-url", "ws://localhost.example.com"),
            ("gateway-url", "ws://127.0.0.1@example.com"),
            ("gateway-url", "ws://[::1]:9/path"),
            ("gateway-url", "ws://127.0.0.1:"),
            ("gateway-url", "ws://127.0.0.1:65536"),
            ("gateway-url", "127.0.0.1:9"),
            ("api-url", "https://127.0.0.1:9"),
            ("api-url", "ws://127.0.0.1:9"),
            ("api-url", "http://discord.com"),
        ] {
            let text = format!("{DISCORD}{key} = {url:?}\n");
            let message = parse(&text, Path::new("/etc")).unwrap_err();
            assert!(
                message.starts_with(&format!("{key} in [discord] is for tests, so it must be ")),
                "{text}: {message}"
            );
            assert!(message.ends_with(&format!("not {url:?}")), "{message}");
        }
    }

    #[test]
    fn mistakes_name_their_place() {
        for (text, mistake) in [
            ("", "missing field `discord`"),
            ("[discord]\nguilds = [1]\n", "missing field `token-file`"),
            ("[discord]\ntoken-file = \"t\"\n", "missing field `guilds`"),
            (
                "[discord]\ntokn-file = \"t\"\nguilds = [1]\n",
                "unknown field `tokn-file`, expected one of `token-file`, `guilds`, `gateway-url`, `api-url`",
            ),
            (
                &format!("{DISCORD}[logs]\n"),
                "unknown field `logs`, expected one of `discord`, `storage`, `log`",
            ),
            (
                &format!("{DISCORD}[storage]\nfile = \"db\"\n"),
                "unknown field `file`, expected `path`",
            ),
            (
                &format!("{DISCORD}[storage]\npath = \"\"\n"),
                "path in [storage] is empty",
            ),
            (
                &format!("{DISCORD}[log]\nlevel = \"loud\"\n"),
                "unknown variant `loud`, expected one of `error`, `warn`, `info`, `debug`, `trace`",
            ),
            (
                &format!("{DISCORD}[log]\nformat = \"xml\"\n"),
                "unknown variant `xml`, expected `text` or `json`",
            ),
            (
                "[discord]\ntoken-file = 3\nguilds = [1]\n",
                "invalid type: integer `3`",
            ),
            ("[discord]\ntoken-file = \"t\"\nguilds = [0]\n", "nonzero"),
            (
                "[discord]\ntoken-file = \"t\"\nguilds = [-1]\n",
                "invalid value",
            ),
            (
                "[discord]\ntoken-file = \"t\"\nguilds = \"1\"\n",
                "invalid type",
            ),
            ("[discord\n", "unclosed table, expected `]`"),
            (
                "[discord]\ntoken-file = \"\"\nguilds = [1]\n",
                "token-file in [discord] is empty",
            ),
            (
                "[discord]\ntoken-file = \"t\"\nguilds = []\n",
                "guilds in [discord] is empty: PuppyBot would work on no server",
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
        fs::write(
            &path,
            "[discord]\ntoken-file = \"secrets/token\"\nguilds = [1]\n",
        )
        .unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.discord.token_file, dir.path().join("secrets/token"));
        // A file in the working directory: relative to that.
        assert_eq!(
            parse(DISCORD, Path::new("")).unwrap().discord.token_file,
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
