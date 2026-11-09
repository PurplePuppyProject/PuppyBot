//! Entry points for the fuzz targets in `fuzz/`, compiled with `--cfg fuzzing`, which cargo fuzz
//! sets, and for the tests, which run each of them on a few inputs: a smoke test that needs
//! neither nightly Rust nor cargo-fuzz. See `fuzz/README.md`.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use puppybot_core::dice::{self, Dice};
use puppybot_core::games::{Coin, EIGHT_BALL, GameText, Hint};
use puppybot_core::github::{
    Commit, GithubText, MAX_BRANCH, MAX_REPOS, Repo, Watch as Shown, is_valid_branch, new_commits,
};
use puppybot_core::i18n::Lang;
use puppybot_core::poll::{self, Poll};
use puppybot_core::quotes::{self, MAX_QUOTE, QuoteText};
use puppybot_core::text::Text;
use puppybot_core::text_command;
use secrecy::ExposeSecret;
use twilight_model::application::interaction::Interaction;
use twilight_model::channel::message::{AllowedMentions, MessageFlags};
use twilight_model::id::Id;

use crate::cli;
use crate::commands::{Answer, Audience, Request};
use crate::github::{self, OwnedCommit};
use crate::storage::Quote;

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
/// get, with what the database may hold, is ephemeral exactly when it is only for the person who
/// asked, mentions nobody, not even in its text, and fits into Discord's limit of 2000
/// characters.
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
    let answers = match asked.request {
        Request::Answer(text) => texts(lang, &[text], Audience::Asker),
        Request::ShowLanguage(_) => texts(lang, &LANGUAGES, Audience::Asker),
        Request::SetLanguage(_, new) => texts(
            lang,
            &[Text::LanguageSet { lang: new }, Text::StorageFailed],
            Audience::Asker,
        ),
        Request::ShowPrefix(_) => texts(lang, &PREFIXES, Audience::Asker),
        Request::SetPrefix(_, prefix) => {
            assert!(text_command::is_valid_prefix(prefix), "{prefix:?}");
            texts(
                lang,
                &[Text::PrefixSet { prefix }, Text::StorageFailed],
                Audience::Asker,
            )
        }
        Request::ServerInfo(_) => texts(
            lang,
            &[
                Text::ServerInfo {
                    name: &"x".repeat(1000),
                    created: i64::MAX,
                    members: Some(u64::MAX),
                },
                Text::InfoFailed,
            ],
            Audience::Asker,
        ),
        Request::Poll(poll) => {
            check_poll(&poll);
            texts(lang, &[Text::PollPosted, Text::PollFailed], Audience::Asker)
        }
        Request::Roll { dice, secret } => rolls(lang, &dice, secret),
        Request::SaveQuote(_, quote) => saving(lang, &quote),
        Request::ShowQuote(_, number) => showing(lang, number),
        Request::DeleteQuote { number, .. } => deleting(lang, number),
        Request::GithubAdd { repo, branch, .. } => adding(lang, repo, branch),
        Request::GithubRemove(_, repo) => removing(lang, repo),
        Request::GithubList(_) => listing(lang),
        Request::Coin => games(lang, &COINS),
        Request::EightBall(question) => games(
            lang,
            &[
                GameText::EightBall {
                    question,
                    answer: EIGHT_BALL - 1,
                },
                GameText::GameFailed,
            ],
        ),
        Request::RpsChoose => vec![Answer {
            buttons: crate::commands::rps_buttons(lang),
            ..Answer::new(GameText::RpsChoose.render(lang), Audience::Everyone)
        }],
        Request::RpsPlay { hand, player } => games(
            lang,
            &[
                GameText::RpsPlayed {
                    player,
                    hand,
                    bot: hand,
                },
                GameText::GameFailed,
            ],
        ),
        Request::Guess {
            number,
            max,
            player,
        } => games(
            lang,
            &[
                GameText::Guessed {
                    player,
                    number: number.unwrap_or(u32::MAX),
                    hint: Hint::Right { tries: u32::MAX },
                    max: max.unwrap_or(u32::MAX),
                },
                GameText::GuessStarted {
                    max: max.unwrap_or(u32::MAX),
                },
                GameText::GameFailed,
            ],
        ),
    };
    answers.into_iter().for_each(check);
}

/// What `/dice` may answer to `dice`, in `lang`, for everyone or, if `secret`, for the person who
/// asked: a roll, of every die at its most, which writes the longest numbers, or a failure.
fn rolls(lang: Lang, dice: &Dice, secret: bool) -> Vec<Answer> {
    let audience = if secret {
        Audience::Asker
    } else {
        Audience::Everyone
    };
    let roll = dice
        .roll(&mut |sides| Some(sides))
        .map(|roll| roll.to_string())
        .unwrap_or_default();
    assert!(!roll.is_empty(), "no roll of {dice}");
    let rolled = Text::Rolled {
        dice: &dice.to_string(),
        roll: &roll,
    };
    texts(lang, &[rolled, Text::DiceFailed], audience)
}

/// What saving `quote` may answer, in `lang`, and the quote, as /quote would show it.
fn saving(lang: Lang, quote: &Quote) -> Vec<Answer> {
    let saved = [
        QuoteText::Saved { number: u32::MAX },
        QuoteText::AlreadySaved { number: u32::MAX },
        QuoteText::Empty,
        QuoteText::TooLong,
        QuoteText::Full,
    ];
    let mut answers = quote_texts(lang, &saved, Audience::Asker);
    answers.extend(texts(lang, &[Text::StorageFailed], Audience::Asker));
    if let Ok(text) = quotes::quote(&quote.text) {
        answers.push(shown(lang, u32::MAX, &quote.name, text));
    }
    answers
}

/// What /quote show may answer to `number`, in `lang`: also the longest quotes.
fn showing(lang: Lang, number: Option<u32>) -> Vec<Answer> {
    let number = number.unwrap_or(u32::MAX);
    let author = "*".repeat(1000);
    let longest = [
        "\n".repeat(MAX_QUOTE),
        "*".repeat(MAX_QUOTE),
        "*\n".repeat(MAX_QUOTE / 2),
    ];
    let mut answers: Vec<_> = longest
        .iter()
        .map(|text| shown(lang, number, &author, text))
        .collect();
    let missing = [QuoteText::NoQuotes, QuoteText::NoSuchQuote { number }];
    answers.extend(quote_texts(lang, &missing, Audience::Asker));
    answers.extend(texts(lang, &[Text::StorageFailed], Audience::Asker));
    answers
}

/// What /quote delete may answer to `number`, in `lang`.
fn deleting(lang: Lang, number: u32) -> Vec<Answer> {
    let deleted = [
        QuoteText::Deleted { number },
        QuoteText::NotYours { number },
        QuoteText::NoSuchQuote { number },
    ];
    let mut answers = quote_texts(lang, &deleted, Audience::Asker);
    answers.extend(texts(lang, &[Text::StorageFailed], Audience::Asker));
    answers
}

/// The answer that shows `text` as the quote `number` by `author`, in `lang`, with the longest
/// link and date.
fn shown(lang: Lang, number: u32, author: &str, text: &str) -> Answer {
    let link = format!("https://discord.com/channels/{0}/{0}/{0}", u64::MAX);
    let shown = QuoteText::Shown {
        number,
        author,
        said: i64::MIN,
        text,
        link: &link,
    };
    Answer::new(shown.render(lang), Audience::Everyone)
}

/// What /github add may answer to `repo` and `branch`, as the person wrote them, in `lang`.
fn adding(lang: Lang, repo: &str, branch: Option<&str>) -> Vec<Answer> {
    let failed = [
        GithubText::InvalidRepo,
        GithubText::InvalidBranch,
        GithubText::Full,
        GithubText::GithubFailed,
    ];
    let mut answers = github_texts(lang, &failed);
    answers.extend(texts(lang, &[Text::StorageFailed], Audience::Asker));
    if let Some(repo) = Repo::parse(repo) {
        let repo = repo.to_string();
        let branch = branch
            .filter(|branch| is_valid_branch(branch))
            .unwrap_or("main");
        let channel = format!("<#{}>", u64::MAX);
        let added = [
            GithubText::Added {
                repo: &repo,
                branch,
                channel: &channel,
                releases: true,
            },
            GithubText::NoSuchRepo { repo: &repo },
            GithubText::NoSuchBranch {
                repo: &repo,
                branch,
            },
        ];
        answers.extend(github_texts(lang, &added));
    }
    answers
}

/// What /github remove may answer to `repo`, as the person wrote it, in `lang`.
fn removing(lang: Lang, repo: &str) -> Vec<Answer> {
    let mut answers = github_texts(lang, &[GithubText::InvalidRepo]);
    answers.extend(texts(lang, &[Text::StorageFailed], Audience::Asker));
    if let Some(repo) = Repo::parse(repo) {
        let repo = repo.to_string();
        let removed = [
            GithubText::Removed { repo: &repo },
            GithubText::NotWatched { repo: &repo },
        ];
        answers.extend(github_texts(lang, &removed));
    }
    answers
}

/// What /github list may answer, in `lang`: also the longest list.
fn listing(lang: Lang) -> Vec<Answer> {
    let repo = format!("{}/{}", "a".repeat(39), "_".repeat(100));
    let branch = "b".repeat(MAX_BRANCH);
    let channel = format!("<#{}>", u64::MAX);
    let watch = Shown {
        repo: &repo,
        branch: &branch,
        channel: &channel,
        releases: true,
    };
    let watches = [watch; MAX_REPOS];
    let lists = [GithubText::Watched(&[]), GithubText::Watched(&watches)];
    let mut answers = github_texts(lang, &lists);
    answers.extend(texts(lang, &[Text::StorageFailed], Audience::Asker));
    answers
}

/// An answer for the person who asked with each of the texts of GitHub, `texts`, in `lang`.
fn github_texts(lang: Lang, texts: &[GithubText<'_>]) -> Vec<Answer> {
    texts
        .iter()
        .map(|text| Answer::new(text.render(lang), Audience::Asker))
        .collect()
}

/// An answer for `audience` with each of the quotes' `texts`, in `lang`.
fn quote_texts(lang: Lang, texts: &[QuoteText<'_>], audience: Audience) -> Vec<Answer> {
    texts
        .iter()
        .map(|text| Answer::new(text.render(lang), audience))
        .collect()
}

/// An answer for `audience` with each of `texts`, in `lang`.
fn texts(lang: Lang, texts: &[Text<'_>], audience: Audience) -> Vec<Answer> {
    texts
        .iter()
        .map(|text| Answer::new(text.render(lang), audience))
        .collect()
}

/// An answer for everyone with each of the games' `texts`, in `lang`.
fn games(lang: Lang, texts: &[GameText<'_>]) -> Vec<Answer> {
    texts
        .iter()
        .map(|text| Answer::new(text.render(lang), Audience::Everyone))
        .collect()
}

/// What `/settings language` may answer, with what the database may hold.
const LANGUAGES: [Text<'static>; 3] = [
    Text::LanguageIs { chosen: None },
    Text::LanguageIs {
        chosen: Some(Lang::De),
    },
    Text::StorageFailed,
];

/// What `/settings prefix` may answer, with what the database may hold.
const PREFIXES: [Text<'static>; 3] = [
    Text::PrefixIs { chosen: None },
    Text::PrefixIs {
        chosen: Some("pb!"),
    },
    Text::StorageFailed,
];

/// What `/coin` may answer.
const COINS: [GameText<'static>; 3] = [
    GameText::Coin(Coin::Heads),
    GameText::Coin(Coin::Tails),
    GameText::GameFailed,
];

/// The checks of [`interaction`] of `answer`.
fn check(answer: Answer) {
    let audience = answer.audience;
    // An answer without a message has none of these.
    let data = crate::commands::response(answer).data.unwrap_or_default();
    let ephemeral = (audience == Audience::Asker).then_some(MessageFlags::EPHEMERAL);
    assert_eq!(data.flags, ephemeral, "{data:?}");
    assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
    let content = data.content.unwrap_or_default();
    assert!((1..=2000).contains(&content.chars().count()), "{content}");
    // Not even in the text: what others wrote, e.g. a name, appears as it is.
    assert!(!content.contains("<@"), "{content}");
}

/// An answer of GitHub's API, as JSON: a repository, commits or releases.
///
/// Checks beyond not crashing: a repository's default branch is one that PuppyBot takes, each
/// commit's hash has 7 to 64 hexadecimal digits, and what PuppyBot would post of the commits and
/// releases fits into Discord's limit of 2000 characters and mentions nobody.
///
/// # Panics
///
/// If a check fails, which is what the fuzz target looks for.
pub fn github_api(data: &[u8]) {
    if let Some(info) = github::parse_repo(data) {
        assert!(is_valid_branch(&info.default_branch), "{info:?}");
    }
    let mut messages = Vec::new();
    if let Some(commits) = github::parse_commits(data) {
        for commit in &commits {
            let hex = commit.sha.bytes().all(|byte| byte.is_ascii_hexdigit());
            assert!(hex && (7..=64).contains(&commit.sha.len()), "{commit:?}");
        }
        let commits: Vec<Commit<'_>> = commits.iter().map(OwnedCommit::commit).collect();
        // All of them new, as after a force push.
        let new = new_commits(&commits, "");
        for lang in Lang::ALL {
            let pushed = GithubText::Pushed {
                repo: "a/b",
                branch: "main",
                commits: &new.commits,
                more: new.more,
                rewritten: new.rewritten,
            };
            messages.push(pushed.render(lang));
        }
    }
    for release in github::parse_releases(data).unwrap_or_default() {
        for lang in Lang::ALL {
            let released = GithubText::Released {
                repo: "a/b",
                release: release.release(),
            };
            messages.push(released.render(lang));
        }
    }
    for message in messages {
        assert!(message.chars().count() <= 2000, "{message}");
        assert!(!message.contains("<@"), "{message}");
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

    use twilight_model::application::interaction::{Interaction, InteractionData, InteractionType};
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

    /// The seeds of the fuzz target, in `fuzz/seeds/github_api`, are answers of GitHub.
    #[test]
    fn github_api_seeds() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/github_api");
        let mut seeds = 0;
        for entry in fs::read_dir(dir).unwrap() {
            super::github_api(&fs::read(entry.unwrap().path()).unwrap());
            seeds += 1;
        }
        assert_eq!(seeds, 3);
        for input in [&b""[..], b"[]", b"{}", b"\xff"] {
            super::github_api(input);
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
            // Slash commands get an answer, and the buttons of rock, paper, scissors.
            let answered = match &interaction.data {
                Some(InteractionData::MessageComponent(data)) => {
                    data.custom_id.starts_with(crate::commands::RPS)
                }
                _ => interaction.kind == InteractionType::ApplicationCommand,
            };
            assert_eq!(
                crate::commands::request(&interaction, &guilds).is_some(),
                answered
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
