//! The command line: `puppybot` runs the bot with the configuration file that `--config` names,
//! or with [`DEFAULT_CONFIG`]; `--version` and `--help` stand alone.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// The configuration file without `--config`: where the Docker image expects it.
pub const DEFAULT_CONFIG: &str = "/etc/puppybot/config.toml";

/// What `--help` prints, and the end of the message about a wrong command line.
pub const USAGE: &str = "\
Usage: puppybot [--config <file>]
       puppybot --version | --help

Runs the bot with the configuration in <file>, or in /etc/puppybot/config.toml.

Options:
  -c, --config <file>  The configuration file
  -V, --version        Print the version and exit
  -h, --help           Print this help and exit
";

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Run the bot with this configuration file.
    Run {
        /// The configuration file.
        config: PathBuf,
    },
    /// Print the version and exit.
    Version,
    /// Print the usage and exit.
    Help,
}

impl Action {
    /// The command line that asks for this action, which [`parse`] reads as it.
    #[must_use]
    pub fn args(&self) -> Vec<OsString> {
        match self {
            Self::Run { config } => vec!["--config".into(), config.clone().into_os_string()],
            Self::Version => vec!["--version".into()],
            Self::Help => vec!["--help".into()],
        }
    }
}

/// A command line that PuppyBot doesn't understand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// An argument that isn't an option of PuppyBot. Bytes that aren't UTF-8 are replaced.
    UnknownArgument(String),
    /// `--config` without a file, or with an empty one.
    MissingConfig,
    /// More arguments than the options take: `--version` and `--help` stand alone, and
    /// `--config` comes once.
    TooManyArguments,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Quoted and escaped, so control characters in the argument can't reach the
            // terminal.
            Self::UnknownArgument(argument) => write!(f, "unknown argument {argument:?}"),
            Self::MissingConfig => f.write_str("--config needs a file"),
            Self::TooManyArguments => f.write_str("too many arguments"),
        }
    }
}

impl std::error::Error for Error {}

/// Reads the arguments after the name of the program.
///
/// # Errors
///
/// [`Error`] if an argument isn't an option of PuppyBot, `--config` has no file, or more
/// arguments follow than the options take.
pub fn parse<I>(args: I) -> Result<Action, Error>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Action::Run {
            config: PathBuf::from(DEFAULT_CONFIG),
        });
    };
    let action = match first.as_bytes() {
        b"-V" | b"--version" => Action::Version,
        b"-h" | b"--help" => Action::Help,
        b"-c" | b"--config" => Action::Run {
            config: config_file(args.next())?,
        },
        bytes => match bytes.strip_prefix(b"--config=") {
            Some(file) => Action::Run {
                config: config_file(Some(OsStr::from_bytes(file).to_owned()))?,
            },
            None => return Err(Error::UnknownArgument(first.to_string_lossy().into_owned())),
        },
    };
    if args.next().is_some() {
        return Err(Error::TooManyArguments);
    }
    Ok(action)
}

/// The configuration file that follows `--config`, if there is one.
fn config_file(arg: Option<OsString>) -> Result<PathBuf, Error> {
    match arg {
        Some(file) if !file.is_empty() => Ok(PathBuf::from(file)),
        _ => Err(Error::MissingConfig),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Action, Error> {
        parse(args.iter().map(OsString::from))
    }

    fn run(config: &str) -> Action {
        Action::Run {
            config: PathBuf::from(config),
        }
    }

    #[test]
    fn no_arguments_run_the_bot_with_the_default_configuration() {
        assert_eq!(parse_strs(&[]), Ok(run("/etc/puppybot/config.toml")));
    }

    #[test]
    fn a_configuration_file() {
        assert_eq!(parse_strs(&["--config", "bot.toml"]), Ok(run("bot.toml")));
        assert_eq!(
            parse_strs(&["-c", "/srv/bot.toml"]),
            Ok(run("/srv/bot.toml"))
        );
        assert_eq!(parse_strs(&["--config=a b.toml"]), Ok(run("a b.toml")));
        // What follows --config is the file, whatever it looks like.
        assert_eq!(parse_strs(&["--config", "--help"]), Ok(run("--help")));
    }

    #[test]
    fn a_configuration_file_that_is_not_utf8() {
        let file = OsString::from_vec(b"bot\xff.toml".to_vec());
        let expected = Ok(Action::Run {
            config: PathBuf::from(file.clone()),
        });
        assert_eq!(parse([OsString::from("--config"), file.clone()]), expected);
        let mut joined = b"--config=".to_vec();
        joined.extend_from_slice(file.as_bytes());
        assert_eq!(parse([OsString::from_vec(joined)]), expected);
    }

    #[test]
    fn config_needs_a_file() {
        for args in [
            &["--config"][..],
            &["-c"],
            &["--config="],
            &["--config", ""],
        ] {
            assert_eq!(parse_strs(args), Err(Error::MissingConfig), "{args:?}");
        }
    }

    #[test]
    fn options_in_long_and_short_form() {
        assert_eq!(parse_strs(&["--version"]), Ok(Action::Version));
        assert_eq!(parse_strs(&["-V"]), Ok(Action::Version));
        assert_eq!(parse_strs(&["--help"]), Ok(Action::Help));
        assert_eq!(parse_strs(&["-h"]), Ok(Action::Help));
    }

    #[test]
    fn unknown_arguments_are_errors() {
        for argument in [
            "",
            "-v",
            "--Version",
            "version",
            "--help=1",
            "-hV",
            "-c=x",
            "--conf",
        ] {
            assert_eq!(
                parse_strs(&[argument]),
                Err(Error::UnknownArgument(argument.to_owned()))
            );
        }
        // The first argument that is wrong, also before an option.
        assert_eq!(
            parse_strs(&["run", "--help"]),
            Err(Error::UnknownArgument("run".to_owned()))
        );
    }

    #[test]
    fn arguments_that_are_not_utf8_are_errors() {
        let argument = OsString::from_vec(b"--ver\xffsion".to_vec());
        assert_eq!(
            parse([argument]),
            Err(Error::UnknownArgument("--ver\u{fffd}sion".to_owned()))
        );
    }

    #[test]
    fn options_take_no_more_arguments() {
        for args in [
            &["--version", "--help"][..],
            &["-h", "extra"],
            &["--config", "a.toml", "--config", "b.toml"],
            &["--config=a.toml", "b.toml"],
        ] {
            assert_eq!(parse_strs(args), Err(Error::TooManyArguments), "{args:?}");
        }
    }

    #[test]
    fn each_action_has_a_command_line() {
        for action in [
            Action::Run {
                config: PathBuf::from(DEFAULT_CONFIG),
            },
            Action::Run {
                config: PathBuf::from("-c"),
            },
            Action::Version,
            Action::Help,
        ] {
            assert_eq!(parse(action.args()), Ok(action));
        }
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert_eq!(
            Error::UnknownArgument("--colour\u{1b}[31m".to_owned()).to_string(),
            "unknown argument \"--colour\\u{1b}[31m\""
        );
        assert_eq!(Error::MissingConfig.to_string(), "--config needs a file");
        assert_eq!(Error::TooManyArguments.to_string(), "too many arguments");
    }
}
