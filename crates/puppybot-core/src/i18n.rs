//! The languages of PuppyBot's texts: German and English.
//!
//! Every text that users see is a typed text: an enum with a variant for each message, holding
//! what the message shows, and one function per language with an exhaustive `match`. A text
//! without a translation doesn't compile, nothing is parsed at run time, and the tests reach every
//! text in every language:
//!
//! ```
//! use puppybot_core::i18n::Lang;
//!
//! /// What the bot says when a member joins.
//! enum Welcome<'a> {
//!     Greeting { name: &'a str },
//! }
//!
//! impl Welcome<'_> {
//!     fn render(&self, lang: Lang) -> String {
//!         match lang {
//!             Lang::De => self.de(),
//!             Lang::En => self.en(),
//!         }
//!     }
//!
//!     fn de(&self) -> String {
//!         match self {
//!             Self::Greeting { name } => format!("Willkommen, {name}!"),
//!         }
//!     }
//!
//!     fn en(&self) -> String {
//!         match self {
//!             Self::Greeting { name } => format!("Welcome, {name}!"),
//!         }
//!     }
//! }
//!
//! let greeting = Welcome::Greeting { name: "Rex" };
//! assert_eq!(greeting.render(Lang::from_locale("de")), "Willkommen, Rex!");
//! assert_eq!(greeting.render(Lang::from_locale("en-GB")), "Welcome, Rex!");
//! ```
//!
//! Dates and times aren't part of the texts: the bot sends them as Discord timestamps, e.g.
//! `<t:1767225600:F>`, which Discord shows in the language and time zone of each reader.

/// A language of PuppyBot's texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    /// German.
    De,
    /// English, also for every language that PuppyBot doesn't speak.
    En,
}

impl Lang {
    /// All languages, e.g. for tests that every text exists in each of them.
    pub const ALL: [Self; 2] = [Self::De, Self::En];

    /// The language for a locale, e.g. `de` or `en-US` as Discord sends them, or a tag like
    /// `de-AT` or `de_CH`: German for German locales, English for all others.
    #[must_use]
    pub fn from_locale(locale: &str) -> Self {
        match locale.split(['-', '_']).next() {
            Some(language) if language.eq_ignore_ascii_case("de") => Self::De,
            _ => Self::En,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Lang;

    #[test]
    fn german_locales() {
        for locale in ["de", "DE", "de-AT", "de_CH", "de-DE-1996"] {
            assert_eq!(Lang::from_locale(locale), Lang::De, "{locale}");
        }
    }

    #[test]
    fn every_other_locale_is_english() {
        // Discord's English locales, other ones it sends, and some it never sends.
        for locale in [
            "en-US", "en-GB", "fr", "es-419", "zh-TW", "", "-", "deu", "d", "dee-de",
        ] {
            assert_eq!(Lang::from_locale(locale), Lang::En, "{locale}");
        }
    }

    #[test]
    fn all_languages() {
        assert_eq!(Lang::ALL, [Lang::De, Lang::En]);
    }
}
