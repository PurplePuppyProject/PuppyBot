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
    /// The description of /poll.
    PollDescription,
    /// The description of the option `question` of /poll.
    PollQuestionDescription,
    /// The description of the options `answer1` to `answer10` of /poll.
    PollAnswerDescription,
    /// The description of the option `hours` of /poll.
    PollHoursDescription,
    /// The description of the option `multiple` of /poll.
    PollMultipleDescription,
    /// The answer to /poll once the poll is in the channel.
    PollPosted,
    /// The answer to /poll when PuppyBot can't post the poll, e.g. without permission.
    PollFailed,
    /// The answer to a poll without a question or with too few answers.
    PollInvalid,
    /// The answer to a poll beyond the limits of [`crate::poll`].
    PollTooLarge,
    /// The description of /server.
    ServerDescription,
    /// The description of /user.
    UserDescription,
    /// The description of the option `who` of /user.
    UserWhoDescription,
    /// The answer to /server.
    ServerInfo {
        /// The name of the server.
        name: &'a str,
        /// When it was created, in seconds since 1970.
        created: i64,
        /// About how many members it has, if Discord says.
        members: Option<u64>,
    },
    /// The answer to /user.
    UserInfo {
        /// The name of the person.
        name: &'a str,
        /// When they came to Discord, in seconds since 1970.
        created: i64,
        /// When they joined the server, in seconds since 1970, if Discord says.
        joined: Option<i64>,
    },
    /// The answer to /server when PuppyBot can't get at the details.
    InfoFailed,
}

/// The most characters of a name in a text, e.g. of a person or a server: Discord's names are
/// shorter, but PuppyBot doesn't count on that.
pub const MAX_NAME: usize = 100;

/// The start of `text`, of at most `max` characters.
#[must_use]
pub fn clip(text: &str, max: usize) -> &str {
    text.char_indices()
        .nth(max)
        .and_then(|(end, _)| text.get(..end))
        .unwrap_or(text)
}

/// What someone else wrote, e.g. a name, as PuppyBot repeats it: the start of `text`, of at most
/// `max` characters, which Discord shows as it is, on one line. A backslash goes before each
/// character that Discord reads as Markdown, a link, an emoji or a mention, e.g. `*`, `[`, `:`
/// and `<`; a space takes the place of line breaks and other control characters; and the marks
/// that turn the direction of the writing are left out. So a name can't hide a link, a mention or
/// another text in PuppyBot's messages.
#[must_use]
pub fn literal(text: &str, max: usize) -> String {
    let mut literal = String::new();
    for character in clip(text, max).chars() {
        match character {
            '\\' | '*' | '_' | '~' | '`' | '|' | '<' | '>' | '[' | ']' | ':' | '@' => {
                literal.push('\\');
                literal.push(character);
            }
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {}
            character if character.is_control() => literal.push(' '),
            character => literal.push(character),
        }
    }
    literal
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

    /// The answer to /poll once PuppyBot tried to post it: whether it `posted` it.
    #[must_use]
    pub const fn poll_posted(posted: bool) -> Text<'static> {
        if posted {
            Text::PollPosted
        } else {
            Text::PollFailed
        }
    }

    /// The answer to a poll that PuppyBot doesn't post, as of `error`.
    #[must_use]
    pub const fn poll_error(error: crate::poll::Error) -> Text<'static> {
        match error {
            crate::poll::Error::Invalid => Text::PollInvalid,
            crate::poll::Error::TooLarge => Text::PollTooLarge,
        }
    }

    /// All texts, with an example for each value they hold, e.g. for tests that each exists in
    /// every language.
    pub const ALL: [Text<'static>; 46] = [
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
        Text::PollDescription,
        Text::PollQuestionDescription,
        Text::PollAnswerDescription,
        Text::PollHoursDescription,
        Text::PollMultipleDescription,
        Text::PollPosted,
        Text::PollFailed,
        Text::PollInvalid,
        Text::PollTooLarge,
        Text::ServerDescription,
        Text::UserDescription,
        Text::UserWhoDescription,
        Text::ServerInfo {
            name: "Purple Puppies",
            created: 1_700_000_000,
            members: Some(42),
        },
        Text::ServerInfo {
            name: "Purple Puppies",
            created: 1_700_000_000,
            members: None,
        },
        Text::UserInfo {
            name: "Puppy",
            created: 1_600_000_000,
            joined: Some(1_700_000_000),
        },
        Text::UserInfo {
            name: "Puppy",
            created: 1_600_000_000,
            joined: None,
        },
        Text::InfoFailed,
    ];

    /// The descriptions in the list of commands, which Discord limits to 100 characters.
    pub const DESCRIPTIONS: [Text<'static>; 17] = [
        Text::InfoDescription,
        Text::SettingsDescription,
        Text::LanguageDescription,
        Text::LanguageToDescription,
        Text::PrefixDescription,
        Text::PrefixToDescription,
        Text::DiceDescription,
        Text::DiceExpressionDescription,
        Text::DiceSecretDescription,
        Text::PollDescription,
        Text::PollQuestionDescription,
        Text::PollAnswerDescription,
        Text::PollHoursDescription,
        Text::PollMultipleDescription,
        Text::ServerDescription,
        Text::UserDescription,
        Text::UserWhoDescription,
    ];

    /// The text in `lang`.
    #[must_use]
    pub fn render(&self, lang: Lang) -> String {
        match lang {
            Lang::De => self.de(),
            Lang::En => self.en(),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per text, so that a text without its translation doesn't compile"
    )]
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
                 - `{prefix}umfrage Pizza? | Ja | Nein`: startet eine Umfrage für 24 Stunden\n\
                 - `{prefix}server`, `{prefix}nutzer`: seit wann es diesen Server gibt, und seit wann jemand hier ist\n\
                 - `{prefix}münze`, `{prefix}8kugel Frage?`, `{prefix}ssp`, `{prefix}rate 42`: Münze, magische 8-Kugel, Schere-Stein-Papier, Zahlenraten\n\
                 - `{prefix}zitat 3`: ein Zitat, ohne Nummer ein zufälliges; neue speichert das Menü einer Nachricht, Apps → Zitat speichern\n\
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
            Self::PollDescription => "Startet eine Umfrage, die Discord auszählt".to_owned(),
            Self::PollQuestionDescription => "Die Frage, bis zu 300 Zeichen".to_owned(),
            Self::PollAnswerDescription => "Eine Antwort, bis zu 55 Zeichen".to_owned(),
            Self::PollHoursDescription => {
                "Wie viele Stunden sie läuft: 1 bis 168, ohne es 24".to_owned()
            }
            Self::PollMultipleDescription => {
                "Ob man mehr als eine Antwort wählen darf".to_owned()
            }
            Self::PollPosted => "PuppyBot hat die Umfrage gepostet.".to_owned(),
            Self::PollFailed => {
                "PuppyBot kann die Umfrage hier nicht posten: Es braucht in diesem Kanal das Recht, Nachrichten und Umfragen zu senden."
                    .to_owned()
            }
            Self::PollInvalid => {
                "Eine Umfrage braucht eine Frage und 2 bis 10 Antworten, keine davon leer; als Textbefehl getrennt durch `|`, z. B. `Pizza oder Pasta? | Pizza | Pasta`."
                    .to_owned()
            }
            Self::PollTooLarge => {
                "Eine Umfrage hat eine Frage mit bis zu 300 Zeichen und höchstens 10 Antworten mit bis zu 55 Zeichen, und sie läuft 1 bis 168 Stunden."
                    .to_owned()
            }
            Self::ServerDescription => {
                "Zeigt, wann dieser Server entstand und wie viele Mitglieder er hat".to_owned()
            }
            Self::UserDescription => {
                "Zeigt, seit wann jemand auf Discord und auf diesem Server ist".to_owned()
            }
            Self::UserWhoDescription => "Wen es zeigt; ohne es dich".to_owned(),
            Self::ServerInfo {
                name,
                created,
                members,
            } => {
                let members = members.map_or_else(String::new, |members| {
                    format!(" und hat etwa {members} Mitglieder")
                });
                let name = literal(name, MAX_NAME);
                format!("**{name}** entstand am <t:{created}:D> (<t:{created}:R>){members}.")
            }
            Self::UserInfo {
                name,
                created,
                joined,
            } => {
                let joined = joined.map_or_else(String::new, |joined| {
                    format!(" und seit <t:{joined}:D> auf diesem Server")
                });
                let name = literal(name, MAX_NAME);
                format!("**{name}** ist seit <t:{created}:D> (<t:{created}:R>) auf Discord{joined}.")
            }
            Self::InfoFailed => {
                "PuppyBot kommt gerade nicht an die Angaben. Bitte später noch einmal versuchen."
                    .to_owned()
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per text, so that a text without its translation doesn't compile"
    )]
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
                 - `{prefix}poll Pizza? | Yes | No`: starts a poll for 24 hours\n\
                 - `{prefix}server`, `{prefix}user`: since when this server is there, and someone is here\n\
                 - `{prefix}coin`, `{prefix}8ball question?`, `{prefix}rps`, `{prefix}guess 42`: a coin, the magic 8-ball, rock paper scissors, guessing a number\n\
                 - `{prefix}quote 3`: a quote, without a number one at random; the menu of a message saves new ones, Apps → Save quote\n\
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
            Self::PollDescription => "Starts a poll, which Discord counts".to_owned(),
            Self::PollQuestionDescription => "The question, up to 300 characters".to_owned(),
            Self::PollAnswerDescription => "An answer, up to 55 characters".to_owned(),
            Self::PollHoursDescription => {
                "How many hours it runs: 1 to 168, 24 without it".to_owned()
            }
            Self::PollMultipleDescription => {
                "Whether one may choose more than one answer".to_owned()
            }
            Self::PollPosted => "PuppyBot posted the poll.".to_owned(),
            Self::PollFailed => {
                "PuppyBot can't post the poll here: it needs the permission to send messages and polls in this channel."
                    .to_owned()
            }
            Self::PollInvalid => {
                "A poll needs a question and 2 to 10 answers, none of them empty; as a text command separated by `|`, e.g. `Pizza or pasta? | Pizza | Pasta`."
                    .to_owned()
            }
            Self::PollTooLarge => {
                "A poll has a question of up to 300 characters and at most 10 answers of up to 55 characters, and it runs 1 to 168 hours."
                    .to_owned()
            }
            Self::ServerDescription => {
                "Shows when this server was created, and how many members it has".to_owned()
            }
            Self::UserDescription => {
                "Shows since when someone is on Discord and on this server".to_owned()
            }
            Self::UserWhoDescription => "Whom to show; without it, you".to_owned(),
            Self::ServerInfo {
                name,
                created,
                members,
            } => {
                let members = members.map_or_else(String::new, |members| {
                    format!(", and has about {members} members")
                });
                let name = literal(name, MAX_NAME);
                format!("**{name}** was created on <t:{created}:D> (<t:{created}:R>){members}.")
            }
            Self::UserInfo {
                name,
                created,
                joined,
            } => {
                let joined = joined.map_or_else(String::new, |joined| {
                    format!(", and on this server since <t:{joined}:D>")
                });
                let name = literal(name, MAX_NAME);
                format!("**{name}** is on Discord since <t:{created}:D> (<t:{created}:R>){joined}.")
            }
            Self::InfoFailed => {
                "PuppyBot can't get at the details right now. Please try again later.".to_owned()
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
    fn what_others_wrote() {
        assert_eq!(clip("Pfötchen", 4), "Pföt");
        assert_eq!(clip("Pfötchen", 8), "Pfötchen");
        assert_eq!(clip("Pfötchen", 100), "Pfötchen");
        assert_eq!(clip("", 3), "");
        assert_eq!(literal("Pfötchen 🐾", MAX_NAME), "Pfötchen 🐾");
        assert_eq!(literal("Pfötchen", 4), "Pföt");
        // Markdown, links, emoji and mentions as they are.
        assert_eq!(
            literal(r"**a** _b_ ~~c~~ `d` ||e|| > f \", MAX_NAME),
            r"\*\*a\*\* \_b\_ \~\~c\~\~ \`d\` \|\|e\|\| \> f \\"
        );
        assert_eq!(
            literal("[Nitro](https://example.com) <@1> @everyone :x:", MAX_NAME),
            r"\[Nitro\](https\://example.com) \<\@1\> \@everyone \:x\:"
        );
        // On one line, and in the direction of the rest of the text.
        assert_eq!(literal("a\nb\r\tc\u{7}", MAX_NAME), "a b  c ");
        assert_eq!(literal("a\u{202e}b\u{2067}c\u{2069}", MAX_NAME), "abc");
        // Clipped before the backslashes go in.
        assert_eq!(
            literal(&"*".repeat(MAX_NAME + 1), MAX_NAME),
            r"\*".repeat(MAX_NAME)
        );
        let long = "ä".repeat(MAX_NAME + 50);
        let user = Text::UserInfo {
            name: &long,
            created: 0,
            joined: None,
        };
        assert!(user.render(Lang::En).chars().count() < MAX_NAME + 100);
        let server = Text::ServerInfo {
            name: "**Puppies** <@1>",
            created: 0,
            members: None,
        };
        assert_eq!(
            server.render(Lang::De),
            r"**\*\*Puppies\*\* \<\@1\>** entstand am <t:0:D> (<t:0:R>)."
        );
    }

    #[test]
    fn dates_as_discord_timestamps() {
        let server = Text::ServerInfo {
            name: "Purple Puppies",
            created: 1_700_000_000,
            members: Some(42),
        };
        assert_eq!(
            server.render(Lang::En),
            "**Purple Puppies** was created on <t:1700000000:D> (<t:1700000000:R>), and has about 42 members."
        );
        let user = Text::UserInfo {
            name: "Puppy",
            created: 1_600_000_000,
            joined: Some(1_700_000_000),
        };
        assert_eq!(
            user.render(Lang::De),
            "**Puppy** ist seit <t:1600000000:D> (<t:1600000000:R>) auf Discord und seit <t:1700000000:D> auf diesem Server."
        );
    }

    #[test]
    fn answers_of_polls_and_dice() {
        assert_eq!(Text::poll_posted(true), Text::PollPosted);
        assert_eq!(Text::poll_posted(false), Text::PollFailed);
        assert_eq!(
            Text::poll_error(crate::poll::Error::Invalid),
            Text::PollInvalid
        );
        assert_eq!(
            Text::poll_error(crate::poll::Error::TooLarge),
            Text::PollTooLarge
        );
        assert_eq!(
            Text::dice_error(crate::dice::Error::Invalid),
            Text::DiceInvalid
        );
        assert_eq!(
            Text::dice_error(crate::dice::Error::TooLarge),
            Text::DiceTooLarge
        );
    }

    #[test]
    fn the_help_and_the_prefix() {
        let help = Text::Help { prefix: "?" };
        assert_eq!(
            help.render(Lang::En),
            "PuppyBot's text commands start with `?` here, or with a mention of PuppyBot:\n\
             - `?info`: what PuppyBot is, and which version runs\n\
             - `?dice 2d6+3`: rolls dice, e.g. also `d20` or `4d6kh3`\n\
             - `?poll Pizza? | Yes | No`: starts a poll for 24 hours\n\
             - `?server`, `?user`: since when this server is there, and someone is here\n\
             - `?coin`, `?8ball question?`, `?rps`, `?guess 42`: a coin, the magic 8-ball, rock paper scissors, guessing a number\n\
             - `?quote 3`: a quote, without a number one at random; the menu of a message saves new ones, Apps → Save quote\n\
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
