//! The texts of PuppyBot that users see, each in German and English, see [`crate::i18n`].

use crate::i18n::Lang;
use crate::text_command::DEFAULT_PREFIX;

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
    /// The description of the command /settings in the list of commands.
    SettingsDescription,
    /// The description of /settings language.
    LanguageDescription,
    /// The description of the option `to` of /settings language.
    LanguageToDescription,
    /// The name of a language, e.g. as a choice of /settings language.
    LanguageName {
        /// The language.
        lang: Lang,
    },
    /// The answer to /settings language without `to`: the language in which PuppyBot writes to
    /// everyone on the server.
    LanguageIs {
        /// The language chosen for the server, none if nobody chose one yet.
        chosen: Option<Lang>,
    },
    /// The answer to /settings language with `to`.
    LanguageSet {
        /// The new language of the server.
        lang: Lang,
    },
    /// The answer to /settings for someone who may not manage the server.
    NotAllowed,
    /// The answer when PuppyBot can't read or save its settings, e.g. as the disk is full.
    StorageFailed,
    /// The description of /settings prefix.
    PrefixDescription,
    /// The description of the option `to` of /settings prefix.
    PrefixToDescription,
    /// The answer to /settings prefix without `to`: how text commands start on the server.
    PrefixIs {
        /// The prefix chosen for the server, none if nobody chose one yet.
        chosen: Option<&'a str>,
    },
    /// The answer to /settings prefix with `to`.
    PrefixSet {
        /// The new prefix of the server.
        prefix: &'a str,
    },
    /// The answer to /settings prefix with a prefix that PuppyBot doesn't take.
    PrefixInvalid,
    /// The answer to the text command help: the commands.
    Help {
        /// The prefix of the server.
        prefix: &'a str,
    },
    /// The description of /dice.
    DiceDescription,
    /// The description of the option `expression` of /dice.
    DiceExpressionDescription,
    /// The description of the option `secret` of /dice.
    DiceSecretDescription,
    /// A roll of dice.
    Rolled {
        /// The expression, as [`crate::dice::Dice`] writes it, e.g. `2d6+3`.
        dice: &'a str,
        /// What it showed, as [`crate::dice::Roll`] writes it, e.g. `[4, 2] + 3 = **9**`.
        roll: &'a str,
    },
    /// The answer to an expression of dice that isn't one.
    DiceInvalid,
    /// The answer to an expression of dice beyond the limits of [`crate::dice`].
    DiceTooLarge,
    /// The answer when PuppyBot can't roll, as the system has no randomness.
    DiceFailed,
}

impl Text<'_> {
    /// The answer to an expression of dice that PuppyBot doesn't roll, as of `error`.
    #[must_use]
    pub const fn dice_error(error: crate::dice::Error) -> Text<'static> {
        match error {
            crate::dice::Error::Invalid => Text::DiceInvalid,
            crate::dice::Error::TooLarge => Text::DiceTooLarge,
        }
    }

    /// All texts, with an example for each value they hold, e.g. for tests that each exists in
    /// every language.
    pub const ALL: [Text<'static>; 29] = [
        Text::InfoDescription,
        Text::Info { version: "1.2.3" },
        Text::UnknownCommand,
        Text::NotHere,
        Text::SettingsDescription,
        Text::LanguageDescription,
        Text::LanguageToDescription,
        Text::LanguageName { lang: Lang::De },
        Text::LanguageName { lang: Lang::En },
        Text::LanguageIs { chosen: None },
        Text::LanguageIs {
            chosen: Some(Lang::De),
        },
        Text::LanguageIs {
            chosen: Some(Lang::En),
        },
        Text::LanguageSet { lang: Lang::De },
        Text::NotAllowed,
        Text::StorageFailed,
        Text::PrefixDescription,
        Text::PrefixToDescription,
        Text::PrefixIs { chosen: None },
        Text::PrefixIs { chosen: Some("?") },
        Text::PrefixSet { prefix: "pb!" },
        Text::PrefixInvalid,
        Text::Help { prefix: "!" },
        Text::DiceDescription,
        Text::DiceExpressionDescription,
        Text::DiceSecretDescription,
        Text::Rolled {
            dice: "2d6+3",
            roll: "[4, 2] + 3 = **9**",
        },
        Text::DiceInvalid,
        Text::DiceTooLarge,
        Text::DiceFailed,
    ];

    /// The descriptions in the list of commands, which Discord limits to 100 characters.
    pub const DESCRIPTIONS: [Text<'static>; 9] = [
        Text::InfoDescription,
        Text::SettingsDescription,
        Text::LanguageDescription,
        Text::LanguageToDescription,
        Text::PrefixDescription,
        Text::PrefixToDescription,
        Text::DiceDescription,
        Text::DiceExpressionDescription,
        Text::DiceSecretDescription,
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
            Self::SettingsDescription => {
                "Die Einstellungen von PuppyBot für diesen Server".to_owned()
            }
            Self::LanguageDescription => {
                "Zeigt oder ändert die Sprache, in der PuppyBot hier allen schreibt".to_owned()
            }
            Self::LanguageToDescription => {
                "Die neue Sprache; ohne sie zeigt PuppyBot die jetzige".to_owned()
            }
            Self::LanguageName { lang } => match lang {
                Lang::De => "Deutsch",
                Lang::En => "Englisch",
            }
            .to_owned(),
            Self::LanguageIs { chosen: Some(lang) } => format!(
                "Auf diesem Server schreibt PuppyBot allen auf {}.",
                Self::LanguageName { lang: *lang }.de()
            ),
            Self::LanguageIs { chosen: None } => format!(
                "Auf diesem Server schreibt PuppyBot allen auf {}, da noch niemand eine Sprache gewählt hat.",
                Self::LanguageName {
                    lang: Lang::default()
                }
                .de()
            ),
            Self::LanguageSet { lang } => format!(
                "Ab jetzt schreibt PuppyBot allen auf diesem Server auf {}. Antworten wie diese sind in der Sprache der Person, die fragt.",
                Self::LanguageName { lang: *lang }.de()
            ),
            Self::NotAllowed => {
                "Die Einstellungen von PuppyBot dürfen nur Leute nutzen, die diesen Server verwalten dürfen."
                    .to_owned()
            }
            Self::StorageFailed => {
                "PuppyBot kommt gerade nicht an seine Einstellungen. Bitte später noch einmal versuchen."
                    .to_owned()
            }
            Self::PrefixDescription => {
                "Zeigt oder ändert, womit die Textbefehle von PuppyBot hier beginnen, z. B. ! für !info"
                    .to_owned()
            }
            Self::PrefixToDescription => {
                "Das neue Präfix: 1 bis 5 Zeichen, z. B. ? oder pb!; ohne es zeigt PuppyBot das jetzige"
                    .to_owned()
            }
            Self::PrefixIs {
                chosen: Some(prefix),
            } => format!(
                "Auf diesem Server beginnen die Textbefehle von PuppyBot mit `{prefix}`, z. B. `{prefix}info`, oder mit einer Erwähnung von PuppyBot."
            ),
            Self::PrefixIs { chosen: None } => format!(
                "Auf diesem Server beginnen die Textbefehle von PuppyBot mit `{DEFAULT_PREFIX}`, z. B. `{DEFAULT_PREFIX}info`, da noch niemand ein Präfix gewählt hat, oder mit einer Erwähnung von PuppyBot."
            ),
            Self::PrefixSet { prefix } => format!(
                "Ab jetzt beginnen die Textbefehle von PuppyBot auf diesem Server mit `{prefix}`, z. B. `{prefix}info`."
            ),
            Self::PrefixInvalid => {
                "Ein Präfix hat 1 bis 5 sichtbare ASCII-Zeichen, z. B. `!` oder `pb?`, aber keine Leerzeichen, keins von < > @ # : / und keine Backticks."
                    .to_owned()
            }
            Self::Help { prefix } => format!(
                "Die Textbefehle von PuppyBot beginnen hier mit `{prefix}` oder mit einer Erwähnung von PuppyBot:\n\
                 - `{prefix}info`: was PuppyBot ist und welche Version läuft\n\
                 - `{prefix}würfel 2W6+3`: würfelt, z. B. auch `W20` oder `4W6kh3`\n\
                 - `{prefix}hilfe`: diese Befehle\n\
                 Seine Slash-Befehle, z. B. `/info`, stehen in Discords Liste der Befehle."
            ),
            Self::DiceDescription => "Würfelt, z. B. 2W6+3, W20 oder 4W6kh3".to_owned(),
            Self::DiceExpressionDescription => {
                "Was gewürfelt wird, z. B. 2W6+3; ohne es ein Würfel mit sechs Seiten".to_owned()
            }
            Self::DiceSecretDescription => "Nur du siehst das Ergebnis".to_owned(),
            Self::Rolled { dice, roll } => format!("`{dice}` ergibt {roll}"),
            Self::DiceInvalid => {
                "PuppyBot würfelt Ausdrücke wie `2W6+3` oder `2d6+3`: die Zahl der Würfel, W oder d, und ihre Seiten, z. B. `W20`, dazu plus oder minus Zahlen oder weitere Würfel. `4W6kh3` zählt die höchsten 3 von 4 Würfeln, `kl` die niedrigsten."
                    .to_owned()
            }
            Self::DiceTooLarge => {
                "PuppyBot würfelt höchstens 100 Würfel auf einmal, mit bis zu 1000 Seiten, in höchstens 20 Teilen und mit Zahlen bis 1000000."
                    .to_owned()
            }
            Self::DiceFailed => {
                "PuppyBot kann gerade nicht würfeln. Bitte später noch einmal versuchen.".to_owned()
            }
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
            Self::SettingsDescription => "PuppyBot's settings for this server".to_owned(),
            Self::LanguageDescription => {
                "Shows or changes the language in which PuppyBot writes to everyone here".to_owned()
            }
            Self::LanguageToDescription => {
                "The new language; without it, PuppyBot shows the current one".to_owned()
            }
            Self::LanguageName { lang } => match lang {
                Lang::De => "German",
                Lang::En => "English",
            }
            .to_owned(),
            Self::LanguageIs { chosen: Some(lang) } => format!(
                "On this server, PuppyBot writes to everyone in {}.",
                Self::LanguageName { lang: *lang }.en()
            ),
            Self::LanguageIs { chosen: None } => format!(
                "On this server, PuppyBot writes to everyone in {}, as nobody chose a language yet.",
                Self::LanguageName {
                    lang: Lang::default()
                }
                .en()
            ),
            Self::LanguageSet { lang } => format!(
                "From now on, PuppyBot writes to everyone on this server in {}. Answers like this one are in the language of the person who asks.",
                Self::LanguageName { lang: *lang }.en()
            ),
            Self::NotAllowed => {
                "Only people who may manage this server can use PuppyBot's settings.".to_owned()
            }
            Self::StorageFailed => {
                "PuppyBot can't get at its settings right now. Please try again later.".to_owned()
            }
            Self::PrefixDescription => {
                "Shows or changes how PuppyBot's text commands start here, e.g. ! for !info".to_owned()
            }
            Self::PrefixToDescription => {
                "The new prefix: 1 to 5 characters, e.g. ? or pb!; without it, PuppyBot shows the current one"
                    .to_owned()
            }
            Self::PrefixIs {
                chosen: Some(prefix),
            } => format!(
                "On this server, PuppyBot's text commands start with `{prefix}`, e.g. `{prefix}info`, or with a mention of PuppyBot."
            ),
            Self::PrefixIs { chosen: None } => format!(
                "On this server, PuppyBot's text commands start with `{DEFAULT_PREFIX}`, e.g. `{DEFAULT_PREFIX}info`, as nobody chose a prefix yet, or with a mention of PuppyBot."
            ),
            Self::PrefixSet { prefix } => format!(
                "From now on, PuppyBot's text commands start with `{prefix}` on this server, e.g. `{prefix}info`."
            ),
            Self::PrefixInvalid => {
                "A prefix has 1 to 5 visible ASCII characters, e.g. `!` or `pb?`, but no spaces, none of < > @ # : / and no backticks."
                    .to_owned()
            }
            Self::Help { prefix } => format!(
                "PuppyBot's text commands start with `{prefix}` here, or with a mention of PuppyBot:\n\
                 - `{prefix}info`: what PuppyBot is, and which version runs\n\
                 - `{prefix}dice 2d6+3`: rolls dice, e.g. also `d20` or `4d6kh3`\n\
                 - `{prefix}help`: these commands\n\
                 Its slash commands, e.g. `/info`, are in Discord's list of commands."
            ),
            Self::DiceDescription => "Rolls dice, e.g. 2d6+3, d20 or 4d6kh3".to_owned(),
            Self::DiceExpressionDescription => {
                "What to roll, e.g. 2d6+3; without it, one die with six sides".to_owned()
            }
            Self::DiceSecretDescription => "Only you see the result".to_owned(),
            Self::Rolled { dice, roll } => format!("`{dice}` gives {roll}"),
            Self::DiceInvalid => {
                "PuppyBot rolls expressions like `2d6+3`: the number of dice, d, and their sides, e.g. `d20`, plus or minus numbers or more dice. `4d6kh3` counts the highest 3 of 4 dice, `kl` the lowest."
                    .to_owned()
            }
            Self::DiceTooLarge => {
                "PuppyBot rolls at most 100 dice at once, with up to 1000 sides, in at most 20 parts, and numbers up to 1000000."
                    .to_owned()
            }
            Self::DiceFailed => {
                "PuppyBot can't roll right now. Please try again later.".to_owned()
            }
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
                // No mentions, which would ping someone, though the answers allow none.
                for mention in ["@everyone", "@here", "<@"] {
                    assert!(!rendered.contains(mention), "{text:?}: {rendered}");
                }
            }
            assert_ne!(texts[0], texts[1], "{text:?} isn't translated");
        }
    }

    #[test]
    fn descriptions_fit_into_discord() {
        for text in Text::DESCRIPTIONS {
            for lang in Lang::ALL {
                let description = text.render(lang);
                assert!(description.chars().count() <= 100, "{description}");
            }
        }
    }

    #[test]
    fn the_help_and_the_prefix() {
        let help = Text::Help { prefix: "?" };
        assert_eq!(
            help.render(Lang::En),
            "PuppyBot's text commands start with `?` here, or with a mention of PuppyBot:\n\
             - `?info`: what PuppyBot is, and which version runs\n\
             - `?dice 2d6+3`: rolls dice, e.g. also `d20` or `4d6kh3`\n\
             - `?help`: these commands\n\
             Its slash commands, e.g. `/info`, are in Discord's list of commands."
        );
        assert!(
            help.render(Lang::De)
                .contains("\n- `?hilfe`: diese Befehle\n")
        );
        assert_eq!(
            Text::PrefixIs { chosen: None }.render(Lang::En),
            "On this server, PuppyBot's text commands start with `!`, e.g. `!info`, as nobody chose a prefix yet, or with a mention of PuppyBot."
        );
        assert_eq!(
            Text::PrefixSet { prefix: "pb!" }.render(Lang::De),
            "Ab jetzt beginnen die Textbefehle von PuppyBot auf diesem Server mit `pb!`, z. B. `pb!info`."
        );
    }

    #[test]
    fn the_answers_of_settings_language() {
        let answers = [
            (
                Text::LanguageIs { chosen: None },
                "On this server, PuppyBot writes to everyone in English, as nobody chose a language yet.",
                "Auf diesem Server schreibt PuppyBot allen auf Englisch, da noch niemand eine Sprache gewählt hat.",
            ),
            (
                Text::LanguageIs {
                    chosen: Some(Lang::De),
                },
                "On this server, PuppyBot writes to everyone in German.",
                "Auf diesem Server schreibt PuppyBot allen auf Deutsch.",
            ),
            (
                Text::LanguageSet { lang: Lang::En },
                "From now on, PuppyBot writes to everyone on this server in English. Answers like this one are in the language of the person who asks.",
                "Ab jetzt schreibt PuppyBot allen auf diesem Server auf Englisch. Antworten wie diese sind in der Sprache der Person, die fragt.",
            ),
        ];
        for (text, en, de) in answers {
            assert_eq!(text.render(Lang::En), en);
            assert_eq!(text.render(Lang::De), de);
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
