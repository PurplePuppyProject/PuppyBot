//! The texts of PuppyBot that users see, each in German and English, see [`crate::i18n`].

use crate::i18n::Lang;

/// Where the code of PuppyBot is, from `Cargo.toml`.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// A text that users see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text<'a> {
    /// The description of the command /info in the list of commands.
    InfoDescription,
    /// The answer to /info.
    Info {
        /// The version of PuppyBot.
        version: &'a str,
    },
    /// The answer to a command that PuppyBot doesn't know, e.g. one of an older version.
    UnknownCommand,
    /// The answer to a command outside the servers of PuppyBot.
    NotHere,
}

impl Text<'_> {
    /// All texts, with an example for each value they hold, e.g. for tests that each exists in
    /// every language.
    pub const ALL: [Text<'static>; 4] = [
        Text::InfoDescription,
        Text::Info { version: "1.2.3" },
        Text::UnknownCommand,
        Text::NotHere,
    ];

    /// The text in `lang`.
    #[must_use]
    pub fn render(&self, lang: Lang) -> String {
        match lang {
            Lang::De => self.de(),
            Lang::En => self.en(),
        }
    }

    fn de(&self) -> String {
        match self {
            Self::InfoDescription => "Zeigt, was PuppyBot ist und welche Version läuft".to_owned(),
            Self::Info { version } => format!(
                "PuppyBot {version}, ein Discord-Bot des Purple Puppy Project. Sein Code: <{REPOSITORY}>"
            ),
            Self::UnknownCommand => "Diesen Befehl kennt PuppyBot nicht (mehr).".to_owned(),
            Self::NotHere => "PuppyBot arbeitet nur auf seinen Servern.".to_owned(),
        }
    }

    fn en(&self) -> String {
        match self {
            Self::InfoDescription => "Shows what PuppyBot is and which version runs".to_owned(),
            Self::Info { version } => format!(
                "PuppyBot {version}, a Discord bot of the Purple Puppy Project. Its code: <{REPOSITORY}>"
            ),
            Self::UnknownCommand => "PuppyBot doesn't know this command (any more).".to_owned(),
            Self::NotHere => "PuppyBot works only on its servers.".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_in_every_language() {
        for text in Text::ALL {
            let texts = Lang::ALL.map(|lang| text.render(lang));
            for rendered in &texts {
                assert!(!rendered.is_empty(), "{text:?}");
                // No mentions, which would ping someone.
                assert!(!rendered.contains('@'), "{text:?}: {rendered}");
            }
            assert_ne!(texts[0], texts[1], "{text:?} isn't translated");
        }
    }

    #[test]
    fn descriptions_fit_into_discord() {
        // Discord allows at most 100 characters.
        for lang in Lang::ALL {
            assert!(Text::InfoDescription.render(lang).chars().count() <= 100);
        }
    }

    #[test]
    fn the_answer_to_info() {
        assert_eq!(
            Text::Info { version: "0.1.0" }.render(Lang::En),
            "PuppyBot 0.1.0, a Discord bot of the Purple Puppy Project. Its code: <https://github.com/PurplePuppyProject/PuppyBot>"
        );
        assert_eq!(
            Text::Info { version: "0.1.0" }.render(Lang::De),
            "PuppyBot 0.1.0, ein Discord-Bot des Purple Puppy Project. Sein Code: <https://github.com/PurplePuppyProject/PuppyBot>"
        );
    }
}
