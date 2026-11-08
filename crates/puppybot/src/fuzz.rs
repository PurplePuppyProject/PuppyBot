//! Entry points for the fuzz targets in `fuzz/`, compiled with `--cfg fuzzing`, which cargo fuzz
//! sets, and for the tests, which run each of them on a few inputs: a smoke test that needs
//! neither nightly Rust nor cargo-fuzz. See `fuzz/README.md`.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use secrecy::ExposeSecret;
use twilight_model::application::interaction::Interaction;
use twilight_model::channel::message::{AllowedMentions, MessageFlags};
use twilight_model::id::Id;

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
/// token file, at an absolute path, as the directory of the configuration file is, and at least
/// one server.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn config(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data)
        && let Ok(config) = crate::config::parse(text, Path::new("/etc/puppybot"))
    {
        assert!(config.discord.token_file.is_absolute());
        assert_ne!(config.discord.guilds.len(), 0);
    }
}

/// An interaction, as JSON from Discord's gateway.
///
/// Checks beyond not crashing: an answer of [`crate::commands::answer`], if there is one, is a
/// message that only the person who asked sees, mentions nobody, and fits into Discord's limit
/// of 2000 characters.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn interaction(data: &[u8]) {
    let Ok(interaction) = serde_json::from_slice::<Interaction>(data) else {
        return;
    };
    let guilds = BTreeSet::from([Id::new(1)]);
    if let Some(response) = crate::commands::answer(&interaction, &guilds) {
        // An answer without a message has none of these.
        let data = response.data.unwrap_or_default();
        assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL), "{data:?}");
        assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
        assert!(
            data.content
                .is_some_and(|content| content.chars().count() <= 2000)
        );
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
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use twilight_model::application::interaction::{Interaction, InteractionType};
    use twilight_model::id::Id;

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
            b"[discord]\ntoken-file = \"token\"\nguilds = [1]\n",
            b"[discord]\ntoken-file = \"/run/secrets/token\"\nguilds = [1, 2]\n[log]\nformat = \"json\"\n",
            b"[discord]\ntoken-file = \"\"\nguilds = [1]\n",
            b"[discord]\ntokn-file = \"x\"\n",
            b"[[[",
            b"\xff",
        ];
        for input in inputs {
            super::config(input);
        }
    }

    #[test]
    fn interaction_smoke_test() {
        let command = |kind: u8, name: &str, guild: &str| {
            format!(
                r#"{{"id": "3", "application_id": "2", "type": {kind}, "token": "t", "version": 1,
                "locale": "de", "attachment_size_limit": 1, "authorizing_integration_owners": {{}},
                "entitlements": [], {guild} "data": {{"id": "4", "name": "{name}", "type": 1}}}}"#
            )
        };
        let inputs = [
            command(2, "info", r#""guild_id": "1","#),
            command(2, "dance", r#""guild_id": "1","#),
            command(2, "info", ""),
            command(4, "info", r#""guild_id": "1","#),
            "{}".to_owned(),
            "\u{0}".to_owned(),
        ];
        for input in inputs {
            super::interaction(input.as_bytes());
        }
    }

    /// The seeds of the fuzz target, in `fuzz/seeds/interaction`, are interactions that
    /// PuppyBot reads, so libFuzzer starts from input that gets far, and only its slash commands
    /// get an answer.
    #[test]
    fn interaction_seeds() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/interaction");
        let guilds = BTreeSet::from([Id::new(1)]);
        let mut seeds = 0;
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            // The output of a test that fails shows which seed it was.
            println!("{}", path.display());
            let seed = fs::read(&path).unwrap();
            let interaction: Interaction = serde_json::from_slice(&seed).unwrap();
            assert_eq!(
                crate::commands::answer(&interaction, &guilds).is_some(),
                interaction.kind == InteractionType::ApplicationCommand
            );
            super::interaction(&seed);
            seeds += 1;
        }
        assert_ne!(seeds, 0, "no seeds in {}", dir.display());
    }

    #[test]
    fn token_file_smoke_test() {
        let inputs: [&[u8]; 6] = [b"token", b"token\r\n", b"", b"\n", b"to ken", b"\xff\n"];
        for input in inputs {
            super::token_file(input);
        }
    }
}
