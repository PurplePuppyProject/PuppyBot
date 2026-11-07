//! Logging to stderr, which Docker collects: a line of text or a JSON object per message, as
//! detailed as the level in the configuration, or as `RUST_LOG`, which takes precedence.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, IsTerminal};

use serde::Deserialize;
use tracing_subscriber::EnvFilter;

/// The table `[log]` of the configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    /// The most detailed level that is logged.
    #[serde(default)]
    pub level: Level,
    /// How each message is written.
    #[serde(default)]
    pub format: Format,
}

/// How detailed the log is, from errors only to everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Only errors.
    Error,
    /// Errors and warnings.
    Warn,
    /// Also what happens, e.g. that the bot started.
    #[default]
    Info,
    /// Also details for finding a bug.
    Debug,
    /// Everything.
    Trace,
}

impl Level {
    /// The level as `RUST_LOG` writes it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// How each message is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// A line of text, with colors on a terminal.
    #[default]
    Text,
    /// A JSON object per line, for log collectors.
    Json,
}

/// A `RUST_LOG` that PuppyBot can't use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The variable isn't UTF-8.
    NotUtf8,
    /// The variable isn't a filter of `tracing`, e.g. `info,puppybot=debug`.
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 => f.write_str("RUST_LOG isn't UTF-8"),
            Self::Invalid(reason) => write!(f, "RUST_LOG isn't a valid filter: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// What is logged: what `rust_log`, the value of `RUST_LOG`, says if it is set, e.g.
/// `info,puppybot=debug`, which can set the level per module; else everything up to `level`.
///
/// # Errors
///
/// [`Error`] if `rust_log` isn't UTF-8 or no valid filter.
pub fn filter(level: Level, rust_log: Option<OsString>) -> Result<EnvFilter, Error> {
    let Some(rust_log) = rust_log else {
        return Ok(EnvFilter::new(level.as_str()));
    };
    let rust_log = rust_log.into_string().map_err(|_| Error::NotUtf8)?;
    EnvFilter::try_new(rust_log).map_err(|error| Error::Invalid(error.to_string()))
}

/// Sets up logging to stderr as `config` says, unless `rust_log` sets what is logged, see
/// [`filter`].
///
/// # Errors
///
/// [`Error`] if `rust_log` isn't UTF-8 or no valid filter.
pub fn init(config: LogConfig, rust_log: Option<OsString>) -> Result<(), Error> {
    let filter = filter(config.level, rust_log)?;
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal());
    // This fails only if logging was set up before, which then goes on.
    let _ = match config.format {
        Format::Text => builder.try_init(),
        Format::Json => builder.json().flatten_event(true).try_init(),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    #[test]
    fn the_level_of_the_configuration() {
        for (level, name) in [
            (Level::Error, "error"),
            (Level::Warn, "warn"),
            (Level::Info, "info"),
            (Level::Debug, "debug"),
            (Level::Trace, "trace"),
        ] {
            assert_eq!(level.as_str(), name);
            assert_eq!(filter(level, None).unwrap().to_string(), name);
        }
    }

    #[test]
    fn rust_log_takes_precedence() {
        let filter = filter(Level::Error, Some("info,puppybot=trace".into())).unwrap();
        assert_eq!(filter.to_string(), "puppybot=trace,info");
    }

    #[test]
    fn rust_log_that_is_invalid() {
        assert_eq!(
            filter(Level::Info, Some(OsString::from_vec(b"inf\xff".to_vec()))).unwrap_err(),
            Error::NotUtf8
        );
        let error = filter(Level::Info, Some("puppybot=loud".into())).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)), "{error:?}");
        assert!(
            error
                .to_string()
                .starts_with("RUST_LOG isn't a valid filter: "),
            "{error}"
        );
        assert_eq!(Error::NotUtf8.to_string(), "RUST_LOG isn't UTF-8");
    }
}
