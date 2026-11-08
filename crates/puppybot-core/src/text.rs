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
}

impl Text<'_> {
    /// All texts, with an example for each value they hold, e.g. for tests that each exists in
    /// every language.
    pub const ALL: [Text<'static>; 15] = [
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
    ];

    /// The descriptions in the list of commands, which Discord limits to 100 characters.
    pub const DESCRIPTIONS: [Text<'static>; 4] = [
        Text::InfoDescription,
        Text::SettingsDescription,
        Text::LanguageDescription,
        Text::LanguageToDescription,
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
        for text in Text::DESCRIPTIONS {
            for lang in Lang::ALL {
                let description = text.render(lang);
                assert!(description.chars().count() <= 100, "{description}");
            }
        }
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
