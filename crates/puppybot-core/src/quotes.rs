//! Quotes: messages that someone saved, so that PuppyBot shows them again, at random or by their
//! number. Each server numbers its quotes from 1, and gives no number twice.
//!
//! Their texts are an enum of their own, [`QuoteText`], in German and English.
//!
//! ```
//! use puppybot_core::quotes::{self, Error};
//!
//! assert_eq!(quotes::quote("  Wuff!  "), Ok("Wuff!"));
//! assert_eq!(quotes::quote(" "), Err(Error::Empty));
//! ```

use crate::i18n::Lang;
use crate::text::{MAX_NAME, clip, literal};

/// The most characters of a quote. A longer message isn't one, so that every quote fits into a
/// message of PuppyBot, also with a backslash before each of its characters.
pub const MAX_QUOTE: usize = 500;

/// The most quotes that PuppyBot keeps for a server.
pub const MAX_QUOTES: u32 = 10_000;

/// Why a message isn't a quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// It has no text, e.g. only a picture.
    Empty,
    /// It has more than [`MAX_QUOTE`] characters.
    TooLong,
}

/// The quote of a message with `text`: the text without the spaces around it.
///
/// # Errors
///
/// [`Error::Empty`] for a message without text, and [`Error::TooLong`] for one of more than
/// [`MAX_QUOTE`] characters.
pub fn quote(text: &str) -> Result<&str, Error> {
    let text = text.trim();
    if text.is_empty() {
        Err(Error::Empty)
    } else if text.chars().count() > MAX_QUOTE {
        Err(Error::TooLong)
    } else {
        Ok(text)
    }
}

/// A text of the quotes that users see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteText<'a> {
    /// The description of /quote.
    QuoteDescription,
    /// The description of /quote show.
    ShowDescription,
    /// The description of /quote delete.
    DeleteDescription,
    /// The description of the option `number` of both.
    NumberDescription,
    /// The answer to saving a message.
    Saved {
        /// The number of its quote.
        number: u32,
    },
    /// The answer to saving a message that is a quote already.
    AlreadySaved {
        /// The number of its quote.
        number: u32,
    },
    /// The answer to saving a message without text, see [`Error::Empty`].
    Empty,
    /// The answer to saving a message of more than [`MAX_QUOTE`] characters.
    TooLong,
    /// The answer to saving a message on a server with [`MAX_QUOTES`] quotes.
    Full,
    /// A quote.
    Shown {
        /// Its number.
        number: u32,
        /// The name of the person who wrote it.
        author: &'a str,
        /// When they wrote it, in seconds since 1970.
        said: i64,
        /// What they wrote.
        text: &'a str,
        /// The address of the message.
        link: &'a str,
    },
    /// The answer to showing a quote on a server without any.
    NoQuotes,
    /// The answer to showing or deleting a quote that isn't there.
    NoSuchQuote {
        /// The number that was asked for.
        number: u32,
    },
    /// The answer to deleting a quote.
    Deleted {
        /// Its number.
        number: u32,
    },
    /// The answer to deleting the quote of someone else, without the permission to manage
    /// messages.
    NotYours {
        /// Its number.
        number: u32,
    },
}

impl QuoteText<'_> {
    /// All texts, with an example for each value they hold.
    pub const ALL: [QuoteText<'static>; 14] = [
        QuoteText::QuoteDescription,
        QuoteText::ShowDescription,
        QuoteText::DeleteDescription,
        QuoteText::NumberDescription,
        QuoteText::Saved { number: 3 },
        QuoteText::AlreadySaved { number: 3 },
        QuoteText::Empty,
        QuoteText::TooLong,
        QuoteText::Full,
        QuoteText::Shown {
            number: 3,
            author: "Puppy",
            said: 1_700_000_000,
            text: "Wuff!",
            link: "https://discord.com/channels/1/2/3",
        },
        QuoteText::NoQuotes,
        QuoteText::NoSuchQuote { number: 7 },
        QuoteText::Deleted { number: 3 },
        QuoteText::NotYours { number: 3 },
    ];

    /// The descriptions in the list of commands, which Discord limits to 100 characters.
    pub const DESCRIPTIONS: [QuoteText<'static>; 4] = [
        QuoteText::QuoteDescription,
        QuoteText::ShowDescription,
        QuoteText::DeleteDescription,
        QuoteText::NumberDescription,
    ];

    /// The answer to a message that isn't a quote, as of `error`.
    #[must_use]
    pub const fn error(error: Error) -> QuoteText<'static> {
        match error {
            Error::Empty => QuoteText::Empty,
            Error::TooLong => QuoteText::TooLong,
        }
    }

    /// The text in `lang`.
    #[must_use]
    pub fn render(&self, lang: Lang) -> String {
        match lang {
            Lang::De => self.de(),
            Lang::En => self.en(),
        }
    }

    fn de(&self) -> String {
        match *self {
            Self::QuoteDescription => "Zitate dieses Servers: zeigen und löschen".to_owned(),
            Self::ShowDescription => "Zeigt ein Zitat, ohne Nummer ein zufälliges".to_owned(),
            Self::DeleteDescription => {
                "Löscht ein Zitat von dir, oder jedes, wenn du Nachrichten verwalten darfst"
                    .to_owned()
            }
            Self::NumberDescription => "Die Nummer des Zitats".to_owned(),
            Self::Saved { number } => {
                format!("PuppyBot hat die Nachricht als Zitat #{number} gespeichert.")
            }
            Self::AlreadySaved { number } => format!("Diese Nachricht ist schon Zitat #{number}."),
            Self::Empty => {
                "PuppyBot speichert nur den Text von Nachrichten, und diese hat keinen.".to_owned()
            }
            Self::TooLong => format!(
                "Diese Nachricht ist zu lang für ein Zitat: PuppyBot speichert bis zu {MAX_QUOTE} Zeichen."
            ),
            Self::Full => format!(
                "Dieser Server hat {MAX_QUOTES} Zitate, mehr speichert PuppyBot nicht. Bitte löscht zuerst welche."
            ),
            Self::Shown {
                number,
                author,
                said,
                text,
                link,
            } => format!(
                "[Zitat #{number}](<{link}>) von **{}**, <t:{said}:D>:\n{}",
                literal(author, MAX_NAME),
                block(text)
            ),
            Self::NoQuotes => {
                "Dieser Server hat noch keine Zitate. Speichert eins im Menü einer Nachricht: Apps → Zitat speichern."
                    .to_owned()
            }
            Self::NoSuchQuote { number } => {
                format!("Auf diesem Server gibt es kein Zitat #{number}.")
            }
            Self::Deleted { number } => format!("PuppyBot hat Zitat #{number} gelöscht."),
            Self::NotYours { number } => format!(
                "Zitat #{number} dürfen nur die zitierte Person und Leute löschen, die Nachrichten verwalten dürfen."
            ),
        }
    }

    fn en(&self) -> String {
        match *self {
            Self::QuoteDescription => "Quotes of this server: show and delete".to_owned(),
            Self::ShowDescription => "Shows a quote, without a number one at random".to_owned(),
            Self::DeleteDescription => {
                "Deletes a quote of yours, or any if you may manage messages".to_owned()
            }
            Self::NumberDescription => "The number of the quote".to_owned(),
            Self::Saved { number } => format!("PuppyBot saved the message as quote #{number}."),
            Self::AlreadySaved { number } => format!("This message is quote #{number} already."),
            Self::Empty => {
                "PuppyBot saves only the text of messages, and this one has none.".to_owned()
            }
            Self::TooLong => format!(
                "This message is too long for a quote: PuppyBot saves up to {MAX_QUOTE} characters."
            ),
            Self::Full => format!(
                "This server has {MAX_QUOTES} quotes, the most that PuppyBot keeps. Please delete some first."
            ),
            Self::Shown {
                number,
                author,
                said,
                text,
                link,
            } => format!(
                "[Quote #{number}](<{link}>) by **{}**, <t:{said}:D>:\n{}",
                literal(author, MAX_NAME),
                block(text)
            ),
            Self::NoQuotes => {
                "This server has no quotes yet. Save one in the menu of a message: Apps → Save quote."
                    .to_owned()
            }
            Self::NoSuchQuote { number } => format!("This server has no quote #{number}."),
            Self::Deleted { number } => format!("PuppyBot deleted quote #{number}."),
            Self::NotYours { number } => format!(
                "Only the person quoted, and people who may manage messages, may delete quote #{number}."
            ),
        }
    }
}

/// The first [`MAX_QUOTE`] characters of `text` as a block quote of Discord, each line after
/// `> `, as [`literal`] repeats it.
fn block(text: &str) -> String {
    let lines: Vec<String> = clip(text, MAX_QUOTE)
        .lines()
        .map(|line| format!("> {}", literal(line, MAX_QUOTE)))
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_a_quote() {
        assert_eq!(quote(" Wuff!\nWau! "), Ok("Wuff!\nWau!"));
        assert_eq!(quote(""), Err(Error::Empty));
        assert_eq!(quote(" \n\t "), Err(Error::Empty));
        let longest = "ä".repeat(MAX_QUOTE);
        assert_eq!(quote(&longest), Ok(longest.as_str()));
        assert_eq!(quote(&"ä".repeat(MAX_QUOTE + 1)), Err(Error::TooLong));
        assert_eq!(QuoteText::error(Error::Empty), QuoteText::Empty);
        assert_eq!(QuoteText::error(Error::TooLong), QuoteText::TooLong);
    }

    #[test]
    fn every_text_in_every_language() {
        for text in QuoteText::ALL {
            let texts = Lang::ALL.map(|lang| text.render(lang));
            for rendered in &texts {
                assert!(!rendered.is_empty(), "{text:?}");
                for mention in ["@everyone", "@here", "<@"] {
                    assert!(!rendered.contains(mention), "{text:?}: {rendered}");
                }
            }
            assert_ne!(texts[0], texts[1], "{text:?} isn't translated");
        }
        for text in QuoteText::DESCRIPTIONS {
            for lang in Lang::ALL {
                assert!(text.render(lang).chars().count() <= 100, "{text:?}");
            }
        }
    }

    #[test]
    fn quotes_as_they_were_written() {
        let shown = QuoteText::Shown {
            number: 3,
            author: "*Puppy*",
            said: 1_700_000_000,
            text: "**Wuff!**\n\nWau <@1>, [hier](https://example.com)",
            link: "https://discord.com/channels/1/2/3",
        };
        assert_eq!(
            shown.render(Lang::En),
            concat!(
                r"[Quote #3](<https://discord.com/channels/1/2/3>) by **\*Puppy\***, <t:1700000000:D>:",
                "\n",
                r"> \*\*Wuff!\*\*",
                // An empty line, as an empty line of the quote.
                "\n> \n",
                r"> Wau \<\@1\>, \[hier\](https\://example.com)"
            )
        );
        assert!(shown.render(Lang::De).starts_with(
            r"[Zitat #3](<https://discord.com/channels/1/2/3>) von **\*Puppy\***, <t:1700000000:D>:"
        ));
    }

    #[test]
    fn the_longest_quotes_fit_into_a_message() {
        let author = "*".repeat(MAX_NAME + 10);
        let link = format!("https://discord.com/channels/{0}/{0}/{0}", u64::MAX);
        let longest = [
            "\n".repeat(MAX_QUOTE + 10),
            "*".repeat(MAX_QUOTE + 10),
            "*\n".repeat(MAX_QUOTE),
        ];
        for text in &longest {
            let shown = QuoteText::Shown {
                number: u32::MAX,
                author: &author,
                said: i64::MIN,
                text,
                link: &link,
            };
            for lang in Lang::ALL {
                assert!(shown.render(lang).chars().count() <= 2000, "{text:?}");
            }
        }
    }
}
