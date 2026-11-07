//! PuppyBot, a Discord bot of the Purple Puppy Project.
//!
//! So far a skeleton: the program understands `--version` and `--help`, and the CI around it
//! checks, tests, fuzzes and publishes it. `src/main.rs` only hands over to [`run`], so the tests
//! reach everything the program does.

use std::ffi::OsString;
use std::io::Write;

pub mod cli;
#[cfg(all(unix, any(test, fuzzing)))]
pub mod fuzz;

/// The version of PuppyBot, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The exit status when the program did what it was asked.
pub const EXIT_SUCCESS: u8 = 0;
/// The exit status when the program couldn't do what it was asked.
pub const EXIT_FAILURE: u8 = 1;
/// The exit status for a command line that PuppyBot doesn't understand.
pub const EXIT_USAGE: u8 = 2;

/// Runs PuppyBot with the arguments after the name of the program, writes what it prints to
/// `out` and its errors to `err`, and returns the exit status.
///
/// `out` and `err` are trait objects, not generic: the coverage of a generic function counts
/// only its best instantiation, so the tests' writers would have to cover all of it alone.
#[must_use]
pub fn run<I>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = OsString>,
{
    match cli::parse(args) {
        Ok(cli::Action::Version) => print(out, &format!("puppybot {VERSION}\n")),
        Ok(cli::Action::Help) => print(out, cli::USAGE),
        Ok(cli::Action::Run) => {
            // If the error can't be written either, there is nothing left to tell it to.
            let _ = writeln!(
                err,
                "puppybot: the bot itself doesn't exist yet, only --version and --help do"
            );
            EXIT_FAILURE
        }
        Err(error) => {
            let _ = write!(err, "puppybot: {error}\n\n{}", cli::USAGE);
            EXIT_USAGE
        }
    }
}

/// Prints `text` and returns the exit status: a failure if that didn't work, e.g. into a pipe
/// that its reader closed.
fn print(out: &mut dyn Write, text: &str) -> u8 {
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => EXIT_SUCCESS,
        Err(_) => EXIT_FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    /// Runs PuppyBot and returns the exit status with what it printed and its errors.
    fn run_strs(args: &[&str]) -> (u8, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = run(args.iter().map(OsString::from), &mut out, &mut err);
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn version() {
        assert_eq!(
            run_strs(&["--version"]),
            (EXIT_SUCCESS, format!("puppybot {VERSION}\n"), String::new())
        );
    }

    #[test]
    fn help() {
        assert_eq!(
            run_strs(&["-h"]),
            (EXIT_SUCCESS, cli::USAGE.to_owned(), String::new())
        );
    }

    #[test]
    fn the_bot_does_not_run_yet() {
        let (status, out, err) = run_strs(&[]);
        assert_eq!((status, out.as_str()), (EXIT_FAILURE, ""));
        assert!(err.contains("doesn't exist yet"), "{err}");
    }

    #[test]
    fn a_wrong_command_line_shows_the_usage() {
        assert_eq!(
            run_strs(&["--bogus"]),
            (
                EXIT_USAGE,
                String::new(),
                format!("puppybot: unknown argument \"--bogus\"\n\n{}", cli::USAGE)
            )
        );
    }

    /// Takes everything but can't flush it, like a pipe that its reader closed.
    struct Unflushable;

    impl Write for Unflushable {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn printing_that_fails_is_a_failure() {
        let status = run(
            [OsString::from("--version")],
            &mut Unflushable,
            &mut io::sink(),
        );
        assert_eq!(status, EXIT_FAILURE);
    }
}
