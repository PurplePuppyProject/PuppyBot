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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    /// German.
    De,
    /// English, also for every language that PuppyBot doesn't speak, and of a server for which
    /// nobody chose a language.
    #[default]
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

    /// The code of the language, as PuppyBot stores it and as the choices of its commands hold
    /// it: `de` or `en`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::De => "de",
            Self::En => "en",
        }
    }

    /// The language whose [`Self::code`] is `code`, and none for any other text, e.g. `DE` or
    /// `de-AT`.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lang| lang.code() == code)
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
        assert_eq!(Lang::default(), Lang::En);
    }

    #[test]
    fn codes() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Lang::De.code(), "de");
        assert_eq!(Lang::En.code(), "en");
        for code in ["", "DE", "de-AT", "deu", "en-US", "fr", " de"] {
            assert_eq!(Lang::from_code(code), None, "{code:?}");
        }
    }
}
