//! Entry points for the fuzz targets in `fuzz/`, compiled with `--cfg fuzzing`, which cargo fuzz
//! sets, and for the tests, which run each of them on a few inputs: a smoke test that needs
//! neither nightly Rust nor cargo-fuzz. See `fuzz/README.md`.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use puppybot_core::dice::{self, Dice};
use puppybot_core::i18n::Lang;
use puppybot_core::poll::{self, Poll};
use puppybot_core::text::Text;
use puppybot_core::text_command;
use secrecy::ExposeSecret;
use twilight_model::application::interaction::Interaction;
use twilight_model::channel::message::{AllowedMentions, MessageFlags};
use twilight_model::id::Id;

use crate::cli;
use crate::commands::{Audience, Request};

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
/// Checks beyond not crashing: each answer that what [`crate::commands::request`] reads may
/// get, with what the database may hold, is a message that only the person who asked sees,
/// mentions nobody, and fits into Discord's limit of 2000 characters.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn interaction(data: &[u8]) {
    let Ok(interaction) = serde_json::from_slice::<Interaction>(data) else {
        return;
    };
    let guilds = BTreeSet::from([Id::new(1)]);
    let Some(asked) = crate::commands::request(&interaction, &guilds) else {
        return;
    };
    let lang = asked.lang;
    let only_asker = |texts: &[Text<'_>]| {
        texts
            .iter()
            .map(|text| (text.render(lang), Audience::Asker))
            .collect::<Vec<_>>()
    };
    let answers = match asked.request {
        Request::Answer(text) => only_asker(&[text]),
        Request::ShowLanguage(_) => only_asker(&[
            Text::LanguageIs { chosen: None },
            Text::LanguageIs {
                chosen: Some(Lang::De),
            },
            Text::StorageFailed,
        ]),
        Request::SetLanguage(_, new) => {
            only_asker(&[Text::LanguageSet { lang: new }, Text::StorageFailed])
        }
        Request::ShowPrefix(_) => only_asker(&[
            Text::PrefixIs { chosen: None },
            Text::PrefixIs {
                chosen: Some("pb!"),
            },
            Text::StorageFailed,
        ]),
        Request::SetPrefix(_, prefix) => {
            assert!(text_command::is_valid_prefix(prefix), "{prefix:?}");
            only_asker(&[Text::PrefixSet { prefix }, Text::StorageFailed])
        }
        Request::ServerInfo(_) => only_asker(&[
            Text::ServerInfo {
                name: &"x".repeat(100),
                created: i64::MAX,
                members: Some(u64::MAX),
            },
            Text::InfoFailed,
        ]),
        Request::Poll(poll) => {
            check_poll(&poll);
            only_asker(&[Text::PollPosted, Text::PollFailed])
        }
        Request::Roll { dice, secret } => {
            let audience = if secret {
                Audience::Asker
            } else {
                Audience::Everyone
            };
            // Every die at its most, which writes the longest numbers.
            let roll = dice
                .roll(&mut |sides| Some(sides))
                .map(|roll| roll.to_string())
                .unwrap_or_default();
            assert!(!roll.is_empty(), "no roll of {dice}");
            let rolled = Text::Rolled {
                dice: &dice.to_string(),
                roll: &roll,
            };
            vec![
                (rolled.render(lang), audience),
                (Text::DiceFailed.render(lang), audience),
            ]
        }
    };
    for (content, audience) in answers {
        // An answer without a message has none of these.
        let data = crate::commands::response(content, audience)
            .data
            .unwrap_or_default();
        let ephemeral = (audience == Audience::Asker).then_some(MessageFlags::EPHEMERAL);
        assert_eq!(data.flags, ephemeral, "{data:?}");
        assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
        assert!(
            data.content
                .is_some_and(|content| content.chars().count() <= 2000)
        );
    }
}

/// A poll as a text command writes it: the question and the answers, separated by `|`.
///
/// Checks beyond not crashing: a poll that [`Poll::parse`] reads keeps the limits of
/// [`poll`], and has no `|` and no spaces around its question and answers.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn poll(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(poll) = Poll::parse(text) else {
        return;
    };
    check_poll(&poll);
    let parts = std::iter::once(poll.question()).chain(poll.answers().iter().map(String::as_str));
    for part in parts {
        assert!(!part.contains(poll::SEPARATOR), "{text:?}");
    }
}

/// Checks that `poll` keeps the limits of [`poll`], and has no spaces around its question and
/// answers.
///
/// # Panics
///
/// If it doesn't.
fn check_poll(poll: &Poll) {
    let question = poll.question();
    assert!((1..=poll::MAX_QUESTION).contains(&question.chars().count()));
    assert!((poll::MIN_ANSWERS..=poll::MAX_ANSWERS).contains(&poll.answers().len()));
    assert!((1..=poll::MAX_HOURS).contains(&poll.hours()));
    let answers = poll.answers().iter().map(String::as_str);
    for part in std::iter::once(question).chain(answers) {
        assert_eq!(part, part.trim());
        assert!(!part.is_empty() && part.chars().count() <= poll::MAX_QUESTION);
    }
    for answer in poll.answers() {
        assert!(answer.chars().count() <= poll::MAX_ANSWER, "{answer:?}");
    }
}

/// An expression of dice.
///
/// Checks beyond not crashing: an expression that [`Dice::parse`] accepts rolls at most
/// [`dice::MAX_DICE`] dice, reads the same when written again, if that is short enough, and
/// its roll, with every die at its most or its least, fits into a message of Discord.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn dice(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(dice) = Dice::parse(text) else {
        return;
    };
    let written = dice.to_string();
    if written.len() <= usize::from(dice::MAX_LENGTH) {
        assert_eq!(Dice::parse(&written).as_ref(), Ok(&dice), "{text:?}");
    }
    let most: fn(u32) -> Option<u32> = Some;
    let least: fn(u32) -> Option<u32> = |_| Some(1);
    for side in [most, least] {
        let mut rolled = 0;
        let roll = dice
            .roll(&mut |sides| {
                rolled += 1;
                side(sides)
            })
            .map(|roll| roll.to_string())
            .unwrap_or_default();
        assert!(rolled <= dice::MAX_DICE, "{text:?}");
        let answer = Text::Rolled {
            dice: &written,
            roll: &roll,
        };
        for lang in Lang::ALL {
            assert!(answer.render(lang).chars().count() <= 2000, "{text:?}");
        }
    }
}

/// A prefix and a message, as the first line of the input and the rest.
///
/// Checks beyond not crashing: [`text_command::parse`] finds a command only after the prefix,
/// if it isn't empty, or after a mention of PuppyBot, and its arguments are a part of the
/// message without spaces around them. [`text_command::is_valid_prefix`] accepts only 1 to 5
/// characters of ASCII that one can see.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn text_command(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (prefix, message) = text.split_once('\n').unwrap_or((text, ""));
    let mentions = ["<@1234>", "<@!1234>"];
    if let Some((command, arguments)) = text_command::parse(message, prefix, &mentions) {
        let after_prefix = !prefix.is_empty() && message.starts_with(prefix);
        let after_mention = mentions.iter().any(|mention| message.starts_with(mention));
        assert!(after_prefix || after_mention, "{command:?}");
        assert_eq!(arguments, arguments.trim());
        assert!(message.contains(arguments));
    }
    if text_command::is_valid_prefix(prefix) {
        assert!((1..=5).contains(&prefix.len()));
        assert!(prefix.bytes().all(|byte| byte.is_ascii_graphic()));
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
                crate::commands::request(&interaction, &guilds).is_some(),
                interaction.kind == InteractionType::ApplicationCommand
            );
            super::interaction(&seed);
            seeds += 1;
        }
        assert_ne!(seeds, 0, "no seeds in {}", dir.display());
    }

    #[test]
    fn poll_smoke_test() {
        let inputs: [&[u8]; 7] = [
            b"Pizza or pasta? | Pizza | Pasta",
            b"Wer? |a|b|c|d|e|f|g|h|i|j",
            b"Wer? |a|b|c|d|e|f|g|h|i|j|k",
            b"Pizza?",
            b" | a | b",
            b"",
            b"\xff",
        ];
        for input in inputs {
            super::poll(input);
        }
    }

    #[test]
    fn dice_smoke_test() {
        let inputs: [&[u8]; 9] = [
            b"2d6+3",
            b"4d6kh3",
            b"100d1000",
            b"d20 - 1d4 + 5",
            b"01d06kl1",
            b"1000000",
            b"2d",
            b"101d6",
            b"\xff",
        ];
        for input in inputs {
            super::dice(input);
        }
        // 99 characters, which `kh` instead of `k` makes 119 when written again: too long to
        // read again, but rolled.
        super::dice(["d6k1"; 20].join("+").as_bytes());
    }

    #[test]
    fn text_command_smoke_test() {
        let inputs: [&[u8]; 9] = [
            b"!\n!info",
            b"!\n!hilfe now",
            b"\n<@1234> info",
            b"\n<@!1234>",
            b"pb?\npb?help me",
            b"",
            b"!",
            b"a b\n<@1234>",
            b"\xff\n!info",
        ];
        for input in inputs {
            super::text_command(input);
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
