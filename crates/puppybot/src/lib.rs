//! PuppyBot, a Discord bot of the Purple Puppy Project: the program.
//!
//! It reads its configuration and the token of the bot, opens its database, connects to Discord,
//! answers the slash commands on its servers, and runs until a signal stops it. `src/main.rs` only hands over to
//! [`run`], so the tests reach everything the program does.

#[cfg(not(unix))]
compile_error!("PuppyBot runs on Linux and other Unix systems");

use std::error::Error;
use std::ffi::OsString;
use std::fmt::Display;
use std::io::Write;
use std::path::Path;

pub mod cli;
pub mod commands;
pub mod config;
pub mod discord;
pub mod files;
#[cfg(any(test, fuzzing))]
pub mod fuzz;
pub mod logging;
pub mod secret;
pub mod shutdown;
pub mod storage;

/// The version of PuppyBot, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The exit status when the program did what it was asked, or a signal stopped it.
pub const EXIT_SUCCESS: u8 = 0;
/// The exit status when the program couldn't run, e.g. as the token file is missing.
pub const EXIT_FAILURE: u8 = 1;
/// The exit status for a command line, configuration file or `RUST_LOG` that PuppyBot can't
/// use.
pub const EXIT_USAGE: u8 = 2;

/// Runs PuppyBot with the arguments after the name of the program, writes what it prints to
/// `out` and its errors to `err`, and returns the exit status. Once logging is set up, errors go
/// to the log instead.
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
        Ok(cli::Action::Run { config }) => start(&config, err),
        Err(error) => {
            // If the error can't be written either, there is nothing left to tell it to.
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

/// Starts the bot with the configuration file at `path`, and returns the exit status once it
/// stopped.
fn start(path: &Path, err: &mut dyn Write) -> u8 {
    let config = match config::load(path) {
        Ok(config) => config,
        Err(error) => return usage_error(err, &error),
    };
    if let Err(error) = logging::init(config.log, std::env::var_os("RUST_LOG")) {
        return usage_error(err, &error);
    }
    match serve(&config, path) {
        Ok(stop) => {
            tracing::info!(signal = stop.name(), "PuppyBot stopped");
            EXIT_SUCCESS
        }
        Err(error) => {
            tracing::error!("PuppyBot can't run: {error}");
            EXIT_FAILURE
        }
    }
}

/// Writes an error about the command line or the configuration, before logging is set up.
fn usage_error(err: &mut dyn Write, error: &dyn Display) -> u8 {
    let _ = writeln!(err, "puppybot: {error}");
    EXIT_USAGE
}

/// Reads the secrets, opens the database, and runs on Discord until a signal stops PuppyBot.
fn serve(config: &config::Config, path: &Path) -> Result<shutdown::Stop, Box<dyn Error>> {
    let secrets = secret::Secrets::load(config)?;
    discord::prepare_tls()?;
    let database = &config.storage.path;
    let storage = storage::Storage::open(database)
        .map_err(|error| format!("can't use the database {}: {error}", database.display()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        // Caught before saying that PuppyBot started, which tells that it may be stopped.
        let mut signals = shutdown::Signals::new()?;
        let discord =
            discord::Discord::new(&config.discord, secrets.discord_token(), Box::new(storage));
        drop(secrets);
        tracing::info!(
            version = VERSION,
            config = %path.display(),
            "PuppyBot started, connecting to Discord"
        );
        Ok(discord.run(&mut signals).await?)
    })
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

    #[test]
    fn a_configuration_that_can_not_be_read() {
        // Fails before logging is set up, so the error goes to err. Running the bot itself is
        // tested by tests/run.rs, with the program in processes of their own.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let path = path.to_str().unwrap();
        let (status, out, err) = run_strs(&["--config", path]);
        assert_eq!((status, out.as_str()), (EXIT_USAGE, ""));
        assert!(
            err.starts_with(&format!("puppybot: can't read the configuration {path}: ")),
            "{err}"
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
