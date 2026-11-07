//! Secrets, e.g. the token of the Discord bot: read from files that the configuration names,
//! kept in a type that doesn't print them, and never logged.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use secrecy::SecretString;
use secrecy::zeroize::Zeroizing;

use crate::config::Config;
use crate::files;

/// How large a file with a secret may be, in bytes. A token of Discord has about 70 characters.
pub const MAX_SIZE: usize = 4096;

/// The secrets of PuppyBot.
#[derive(Debug)]
pub struct Secrets {
    discord_token: SecretString,
}

impl Secrets {
    /// Reads the secrets from the files that `config` names, and warns of files that users other
    /// than their owner may read or change.
    ///
    /// # Errors
    ///
    /// [`Error`] if a file can't be read or holds no valid secret.
    pub fn load(config: &Config) -> Result<Self, Error> {
        Ok(Self {
            discord_token: read("the Discord token", &config.discord.token_file)?,
        })
    }

    /// The token of the Discord bot.
    #[must_use]
    pub fn discord_token(&self) -> &SecretString {
        &self.discord_token
    }
}

/// A file with a secret that PuppyBot can't use. It names the secret, but never what the file
/// holds.
#[derive(Debug)]
pub enum Error {
    /// The file can't be read.
    Read {
        /// Which secret, e.g. "the Discord token".
        secret: &'static str,
        /// The file.
        path: PathBuf,
        /// Why it can't be read.
        source: io::Error,
    },
    /// The file holds no valid secret.
    Invalid {
        /// Which secret, e.g. "the Discord token".
        secret: &'static str,
        /// The file.
        path: PathBuf,
        /// What is wrong with what it holds.
        problem: Problem,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read {
                secret,
                path,
                source,
            } => write!(f, "can't read {secret} from {}: {source}", path.display()),
            Self::Invalid {
                secret,
                path,
                problem,
            } => write!(f, "{secret} in {} {problem}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// What is wrong with what a file with a secret holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Nothing, or only line breaks.
    Empty,
    /// A character that isn't printable ASCII, e.g. a space, or a line break before the end.
    NotPrintable,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "is empty",
            Self::NotPrintable => {
                "holds a character that isn't printable ASCII, e.g. a space, or a line break before its end"
            }
        })
    }
}

/// Reads `secret` from the file at `path`, see [`parse`].
fn read(secret: &'static str, path: &Path) -> Result<SecretString, Error> {
    // Wiped from memory when dropped, also after errors.
    let contents =
        Zeroizing::new(
            files::read_limited(path, MAX_SIZE).map_err(|source| Error::Read {
                secret,
                path: path.to_owned(),
                source,
            })?,
        );
    if let Some(mode) = files::open_to_others(path) {
        tracing::warn!(
            "{secret} in {} can be read or changed by other users (mode {mode:o}): let only the user of PuppyBot read it, e.g. with chmod 400",
            path.display()
        );
    }
    parse(&contents).map_err(|problem| Error::Invalid {
        secret,
        path: path.to_owned(),
        problem,
    })
}

/// The secret in what a file holds: all of it but the line breaks at its end, which editors add.
/// A secret is printable ASCII, as it goes e.g. into an HTTP header, which a line break would
/// end.
///
/// # Errors
///
/// [`Problem`] if there is no secret, or a character that isn't printable ASCII.
pub fn parse(contents: &[u8]) -> Result<SecretString, Problem> {
    let end = contents
        .iter()
        .rposition(|byte| !matches!(byte, b'\n' | b'\r'))
        .map_or(0, |last| last + 1);
    let secret = std::str::from_utf8(contents.get(..end).unwrap_or_default())
        .map_err(|_| Problem::NotPrintable)?;
    if secret.is_empty() {
        return Err(Problem::Empty);
    }
    if !secret.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(Problem::NotPrintable);
    }
    Ok(SecretString::from(secret))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use secrecy::ExposeSecret;

    use super::*;
    use crate::config::DiscordConfig;
    use crate::logging::LogConfig;

    #[test]
    fn the_secret_without_the_line_breaks_at_its_end() {
        for contents in [
            &b"abc.DEF-123_x"[..],
            b"abc.DEF-123_x\n",
            b"abc.DEF-123_x\r\n\n",
        ] {
            assert_eq!(
                parse(contents).unwrap().expose_secret(),
                "abc.DEF-123_x",
                "{contents:?}"
            );
        }
    }

    #[test]
    fn files_without_a_secret() {
        for contents in [&b""[..], b"\n", b"\r\n\r\n"] {
            assert_eq!(parse(contents).unwrap_err(), Problem::Empty, "{contents:?}");
        }
    }

    #[test]
    fn secrets_are_printable_ascii() {
        for contents in [
            &b"two words"[..],
            b"tab\there",
            b"line\nbreak",
            b" leading",
            b"trailing \n",
            b"caf\xc3\xa9",
            b"\xff\xfe",
            b"nul\0",
        ] {
            assert_eq!(
                parse(contents).unwrap_err(),
                Problem::NotPrintable,
                "{contents:?}"
            );
        }
    }

    /// A configuration whose token file is `path`.
    fn config(path: &Path) -> Config {
        Config {
            discord: DiscordConfig {
                token_file: path.to_owned(),
            },
            log: LogConfig::default(),
        }
    }

    #[test]
    fn loads_the_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "abc.DEF\n").unwrap();
        for mode in [0o600, 0o644] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            let secrets = Secrets::load(&config(&path)).unwrap();
            assert_eq!(secrets.discord_token().expose_secret(), "abc.DEF");
            // Debug shows no secret.
            assert!(!format!("{secrets:?}").contains("abc.DEF"));
        }
    }

    #[test]
    fn errors_name_the_secret_and_the_file_but_not_what_it_holds() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        let error = Secrets::load(&config(&missing)).unwrap_err().to_string();
        assert!(
            error.starts_with(&format!(
                "can't read the Discord token from {}: ",
                missing.display()
            )),
            "{error}"
        );

        let path = dir.path().join("token");
        fs::write(&path, "my secret\n").unwrap();
        let error = Secrets::load(&config(&path)).unwrap_err().to_string();
        assert_eq!(
            error,
            format!(
                "the Discord token in {} holds a character that isn't printable ASCII, e.g. a space, or a line break before its end",
                path.display()
            )
        );
        assert!(!error.contains("my secret"));

        fs::write(&path, "\n").unwrap();
        let error = Secrets::load(&config(&path)).unwrap_err().to_string();
        assert_eq!(
            error,
            format!("the Discord token in {} is empty", path.display())
        );
    }
}
