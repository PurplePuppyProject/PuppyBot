//! Entry points for the fuzz targets in `fuzz/`, compiled with `--cfg fuzzing`, which cargo fuzz
//! sets, and for the tests, which run each of them on a few inputs: a smoke test that needs
//! neither nightly Rust nor cargo-fuzz. See `fuzz/README.md`.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use secrecy::ExposeSecret;

use crate::cli;

/// The command line, as arguments separated by NUL bytes, which arguments can't hold.
///
/// Checks beyond not crashing: the command line of what [`cli::parse`] understood is understood
/// the same, and [`crate::run`] prints the version or the help with the exit status 0, or an error
/// with 2. It doesn't run the bot, which would read the files the input names.
///
/// # Panics
///
/// If one of these checks fails, which is what the fuzz target looks for.
pub fn cli_args(data: &[u8]) {
    let args: Vec<OsString> = if data.is_empty() {
        Vec::new()
    } else {
        data.split(|&byte| byte == 0)
            .map(|arg| OsString::from_vec(arg.to_vec()))
            .collect()
    };
    let expected = match cli::parse(args.clone()) {
        Ok(action @ cli::Action::Run { .. }) => {
            assert_eq!(cli::parse(action.args()), Ok(action));
            return;
        }
        Ok(action) => {
            assert_eq!(cli::parse(action.args()), Ok(action));
            crate::EXIT_SUCCESS
        }
        Err(_) => crate::EXIT_USAGE,
    };
    assert_eq!(crate::run(args, &mut io::sink(), &mut io::sink()), expected);
}

/// A configuration file, as its text.
///
/// Checks beyond not crashing: a configuration that [`crate::config::parse`] accepts names a
/// token file, at an absolute path, as the directory of the configuration file is.
///
/// # Panics
///
/// If the check fails, which is what the fuzz target looks for.
pub fn config(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(config) = crate::config::parse(text, Path::new("/etc/puppybot")) {
            assert!(config.discord.token_file.is_absolute());
        }
    }
}

/// What a file with a secret holds.
///
/// Checks beyond not crashing: the secret that [`crate::secret::parse`] reads is printable ASCII
/// and the start of the input, and only line breaks follow it.
///
/// # Panics
///
/// If one of these checks fails, which is what the fuzz target looks for.
pub fn token_file(data: &[u8]) {
    if let Ok(secret) = crate::secret::parse(data) {
        let secret = secret.expose_secret().as_bytes();
        assert!(!secret.is_empty() && secret.iter().all(u8::is_ascii_graphic));
        let rest = data.strip_prefix(secret);
        assert!(rest.is_some_and(|rest| rest.iter().all(|byte| matches!(byte, b'\n' | b'\r'))));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_args_smoke_test() {
        let inputs: [&[u8]; 10] = [
            b"",
            b"--version",
            b"-h",
            b"--help\0--version",
            b"--bogus",
            b"\0",
            b"-V\xff",
            b"\xff\xfe\0\0-h",
            b"--config\0/nonexistent/puppybot.toml",
            b"--config=",
        ];
        for input in inputs {
            super::cli_args(input);
        }
    }

    #[test]
    fn config_smoke_test() {
        let inputs: [&[u8]; 6] = [
            b"[discord]\ntoken-file = \"token\"\n",
            b"[discord]\ntoken-file = \"/run/secrets/token\"\n[log]\nformat = \"json\"\n",
            b"[discord]\ntoken-file = \"\"\n",
            b"[discord]\ntokn-file = \"x\"\n",
            b"[[[",
            b"\xff",
        ];
        for input in inputs {
            super::config(input);
        }
    }

    #[test]
    fn token_file_smoke_test() {
        let inputs: [&[u8]; 6] = [b"token", b"token\r\n", b"", b"\n", b"to ken", b"\xff\n"];
        for input in inputs {
            super::token_file(input);
        }
    }
}
