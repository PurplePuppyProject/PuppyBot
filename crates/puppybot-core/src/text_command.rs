//! Text commands: messages that start with the prefix of a server, e.g. `!info`, or with a
//! mention of PuppyBot, e.g. `@PuppyBot info`.
//!
//! The platform says what the prefix is and what a mention of PuppyBot looks like, so the same
//! parser serves Discord and, later, Signal, which has no slash commands.
//!
//! ```
//! use puppybot_core::text_command::{TextCommand, parse};
//!
//! let mentions = ["<@1234>"];
//! assert_eq!(parse("!Hilfe", "!", &mentions), Some((TextCommand::Help, "")));
//! assert_eq!(parse("<@1234> info", "!", &mentions), Some((TextCommand::Info, "")));
//! // A mention alone asks for help, and unknown commands, e.g. of other bots, for nothing.
//! assert_eq!(parse("<@1234>", "!", &mentions), Some((TextCommand::Help, "")));
//! assert_eq!(parse("!play song", "!", &mentions), None);
//! ```

/// The prefix of a server for which nobody chose one.
pub const DEFAULT_PREFIX: &str = "!";

/// How many characters a prefix may have at most.
pub const MAX_PREFIX: u8 = 5;

/// The characters that a prefix may not hold besides spaces: Discord uses them for mentions
/// (`<@1234>`), channels (`#general`), emoji (`:dog:`), slash commands (`/info`) and code (`` ` ``).
pub const NOT_IN_PREFIX: [char; 7] = ['<', '>', '@', '#', ':', '/', '`'];

/// A text command that PuppyBot knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCommand {
    /// What PuppyBot is, and which version runs.
    Info,
    /// The list of the commands.
    Help,
    /// Dice, see [`crate::dice`].
    Dice,
}

impl TextCommand {
    /// All text commands.
    pub const ALL: [Self; 3] = [Self::Info, Self::Help, Self::Dice];

    /// The names of the command: in English, and in German if it differs.
    #[must_use]
    pub const fn names(self) -> &'static [&'static str] {
        match self {
            Self::Info => &["info"],
            Self::Help => &["help", "hilfe"],
            Self::Dice => &["dice", "roll", "würfel"],
        }
    }

    /// The command with the name `name`, in any case, e.g. `Hilfe`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        let name = name.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|command| command.names().contains(&name.as_str()))
    }
}

/// The text command in `message`, and its arguments, the rest of the message without the spaces
/// around it: if `message` starts with `prefix` and directly the name of a command, e.g. `!info`,
/// or with one of `mentions` of PuppyBot, e.g. `<@1234>`, and the name, after spaces or not.
///
/// A mention alone asks for [`TextCommand::Help`]. A name that PuppyBot doesn't know asks for
/// nothing: other bots often use the same prefix.
#[must_use]
pub fn parse<'a>(
    message: &'a str,
    prefix: &str,
    mentions: &[&str],
) -> Option<(TextCommand, &'a str)> {
    let rest = if let Some(rest) = message.strip_prefix(prefix).filter(|_| !prefix.is_empty()) {
        rest
    } else {
        let rest = mentions
            .iter()
            .filter(|mention| !mention.is_empty())
            .find_map(|mention| message.strip_prefix(mention))?
            .trim_start();
        if rest.is_empty() {
            return Some((TextCommand::Help, ""));
        }
        rest
    };
    let (name, arguments) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    Some((TextCommand::named(name)?, arguments.trim()))
}

/// Whether someone may choose `prefix`: 1 to [`MAX_PREFIX`] characters of ASCII that one can
/// see, e.g. `!` or `pb?`, but none of [`NOT_IN_PREFIX`]. So no spaces, no characters that one
/// can't see, e.g. a zero-width space, and no letters of other alphabets that look like others.
#[must_use]
pub fn is_valid_prefix(prefix: &str) -> bool {
    (1..=usize::from(MAX_PREFIX)).contains(&prefix.len())
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !NOT_IN_PREFIX.contains(&char::from(byte)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENTIONS: [&str; 2] = ["<@1234>", "<@!1234>"];

    #[test]
    fn commands_after_the_prefix() {
        for (message, prefix, expected) in [
            ("!info", "!", Some((TextCommand::Info, ""))),
            ("!INFO", "!", Some((TextCommand::Info, ""))),
            ("!help", "!", Some((TextCommand::Help, ""))),
            ("!hilfe me", "!", Some((TextCommand::Help, "me"))),
            (
                "pb?info  with  spaces  ",
                "pb?",
                Some((TextCommand::Info, "with  spaces")),
            ),
            (
                "!info\nnext line",
                "!",
                Some((TextCommand::Info, "next line")),
            ),
            ("ü!info", "ü!", Some((TextCommand::Info, ""))),
            // Not a command.
            ("! info", "!", None),
            ("!", "!", None),
            ("info", "!", None),
            ("?info", "!", None),
            ("!infos", "!", None),
            ("!dance", "!", None),
            (" !info", "!", None),
            ("", "!", None),
            // Without a prefix, only mentions.
            ("info", "", None),
        ] {
            assert_eq!(
                parse(message, prefix, &MENTIONS),
                expected,
                "{message:?} {prefix:?}"
            );
        }
    }

    #[test]
    fn commands_after_a_mention() {
        for (message, expected) in [
            ("<@1234> info", Some((TextCommand::Info, ""))),
            ("<@1234>info", Some((TextCommand::Info, ""))),
            ("<@!1234>   Hilfe bitte", Some((TextCommand::Help, "bitte"))),
            // A mention alone asks for help.
            ("<@1234>", Some((TextCommand::Help, ""))),
            ("<@!1234> \n ", Some((TextCommand::Help, ""))),
            // Of someone else, or not at the start.
            ("<@5678> info", None),
            ("<@12345> info", None),
            ("hi <@1234> info", None),
            ("<@1234> dance", None),
        ] {
            assert_eq!(parse(message, "!", &MENTIONS), expected, "{message:?}");
        }
        // No mentions, or an empty one, don't make everything a mention.
        assert_eq!(parse("info", "!", &[]), None);
        assert_eq!(parse("info", "!", &[""]), None);
    }

    #[test]
    fn names_in_english_and_german() {
        for command in TextCommand::ALL {
            for name in command.names() {
                assert_eq!(TextCommand::named(name), Some(command));
                assert_eq!(TextCommand::named(&name.to_uppercase()), Some(command));
                assert_eq!(name.to_lowercase(), *name);
            }
        }
        assert_eq!(TextCommand::named(""), None);
    }

    #[test]
    fn prefixes() {
        for prefix in ["!", "?", "pb!", "12345", "$$", "-", ".", "~"] {
            assert!(is_valid_prefix(prefix), "{prefix:?}");
        }
        for prefix in [
            "",
            "123456",
            " ",
            "! ",
            "a b",
            "\t",
            "\u{0}",
            "\u{200b}!",
            "€",
            "ü!",
            "\u{430}",
            "<",
            ">",
            "@",
            "#",
            ":",
            "/",
            "`",
            "<@1>",
            "!:",
        ] {
            assert!(!is_valid_prefix(prefix), "{prefix:?}");
        }
        assert!(is_valid_prefix(DEFAULT_PREFIX));
    }
}
