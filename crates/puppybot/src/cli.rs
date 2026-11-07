//! The command line: without arguments, `puppybot` runs the bot; `--version` and `--help` stand
//! alone.

use std::ffi::OsString;
use std::fmt;

/// What `--help` prints, and the end of the message about a wrong command line.
pub const USAGE: &str = "\
Usage: puppybot [--version | --help]

Without options, PuppyBot runs the bot.

Options:
  -V, --version  Print the version and exit
  -h, --help     Print this help and exit
";

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Run the bot.
    Run,
    /// Print the version and exit.
    Version,
    /// Print the usage and exit.
    Help,
}

impl Action {
    /// The shortest command line that asks for this action, which [`parse`] reads as it.
    #[must_use]
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Run => &[],
            Self::Version => &["--version"],
            Self::Help => &["--help"],
        }
    }
}

/// A command line that PuppyBot doesn't understand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// An argument that isn't an option of PuppyBot. Bytes that aren't UTF-8 are replaced.
    UnknownArgument(String),
    /// An option followed by more arguments: each option stands alone.
    TooManyArguments,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Quoted and escaped, so control characters in the argument can't reach the
            // terminal.
            Self::UnknownArgument(argument) => write!(f, "unknown argument {argument:?}"),
            Self::TooManyArguments => f.write_str("an option must stand alone"),
        }
    }
}

impl std::error::Error for Error {}

/// Reads the arguments after the name of the program.
///
/// # Errors
///
/// [`Error`] if an argument isn't an option of PuppyBot, or more arguments follow an option.
pub fn parse<I>(args: I) -> Result<Action, Error>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Action::Run);
    };
    let action = match first.to_str() {
        Some("-V" | "--version") => Action::Version,
        Some("-h" | "--help") => Action::Help,
        _ => return Err(Error::UnknownArgument(first.to_string_lossy().into_owned())),
    };
    if args.next().is_some() {
        return Err(Error::TooManyArguments);
    }
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Action, Error> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_run_the_bot() {
        assert_eq!(parse_strs(&[]), Ok(Action::Run));
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
        for argument in ["", "-v", "--Version", "version", "--help=1", "-hV"] {
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

    #[cfg(unix)]
    #[test]
    fn arguments_that_are_not_utf8_are_errors() {
        use std::os::unix::ffi::OsStringExt;

        let argument = OsString::from_vec(b"--ver\xffsion".to_vec());
        assert_eq!(
            parse([argument]),
            Err(Error::UnknownArgument("--ver\u{fffd}sion".to_owned()))
        );
    }

    #[test]
    fn options_stand_alone() {
        assert_eq!(
            parse_strs(&["--version", "--help"]),
            Err(Error::TooManyArguments)
        );
        assert_eq!(parse_strs(&["-h", "extra"]), Err(Error::TooManyArguments));
    }

    #[test]
    fn each_action_has_a_command_line() {
        for action in [Action::Run, Action::Version, Action::Help] {
            assert_eq!(parse_strs(action.args()), Ok(action));
        }
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert_eq!(
            Error::UnknownArgument("--colour\u{1b}[31m".to_owned()).to_string(),
            "unknown argument \"--colour\\u{1b}[31m\""
        );
        assert_eq!(
            Error::TooManyArguments.to_string(),
            "an option must stand alone"
        );
    }
}
