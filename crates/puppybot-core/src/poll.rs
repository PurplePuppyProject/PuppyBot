//! Polls, which the platform shows and counts, e.g. Discord: a question, 2 to 10 answers, how
//! many hours they run, and whether people may choose more than one answer.
//!
//! ```
//! use puppybot_core::poll::Poll;
//!
//! let poll = Poll::parse("Pizza or pasta? | Pizza | Pasta").unwrap();
//! assert_eq!(poll.question(), "Pizza or pasta?");
//! assert_eq!(poll.answers(), ["Pizza", "Pasta"]);
//! assert_eq!(poll.hours(), 24);
//! ```

/// The most characters of a question.
pub const MAX_QUESTION: usize = 300;

/// The most characters of an answer.
pub const MAX_ANSWER: usize = 55;

/// The fewest answers.
pub const MIN_ANSWERS: usize = 2;

/// The most answers.
pub const MAX_ANSWERS: usize = 10;

/// The most hours that a poll runs: 7 days.
pub const MAX_HOURS: u16 = 168;

/// The hours that a poll runs if nobody says how many.
pub const DEFAULT_HOURS: u16 = 24;

/// What separates the question and the answers in [`Poll::parse`].
pub const SEPARATOR: char = '|';

/// A poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poll {
    question: String,
    answers: Vec<String>,
    hours: u16,
    multiple: bool,
}

/// Why something isn't a poll that PuppyBot posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// It has no question, or fewer than [`MIN_ANSWERS`] answers, or an empty one.
    Invalid,
    /// It exceeds a limit, e.g. [`MAX_ANSWER`] or [`MAX_HOURS`].
    TooLarge,
}

impl Poll {
    /// A poll of `question` and `answers`, without the spaces around them, which runs `hours`
    /// hours, and in which people may choose more than one answer if `multiple`.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for a question or an answer that is empty, or fewer than
    /// [`MIN_ANSWERS`] answers; [`Error::TooLarge`] for more than a limit, or for 0 hours.
    pub fn new(
        question: &str,
        answers: &[&str],
        hours: u16,
        multiple: bool,
    ) -> Result<Self, Error> {
        let question = question.trim();
        let answers: Vec<&str> = answers.iter().map(|answer| answer.trim()).collect();
        if question.is_empty() || answers.len() < MIN_ANSWERS || answers.contains(&"") {
            return Err(Error::Invalid);
        }
        let too_long = question.chars().count() > MAX_QUESTION
            || answers.len() > MAX_ANSWERS
            || answers
                .iter()
                .any(|answer| answer.chars().count() > MAX_ANSWER);
        if too_long || !(1..=MAX_HOURS).contains(&hours) {
            return Err(Error::TooLarge);
        }
        Ok(Self {
            question: question.to_owned(),
            answers: answers.into_iter().map(str::to_owned).collect(),
            hours,
            multiple,
        })
    }

    /// The poll in `text`: the question and the answers, separated by [`SEPARATOR`], e.g.
    /// `Pizza or pasta? | Pizza | Pasta`. It runs [`DEFAULT_HOURS`], with one answer each.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut parts = text.split(SEPARATOR);
        let question = parts.next().unwrap_or_default();
        let answers: Vec<&str> = parts.collect();
        Self::new(question, &answers, DEFAULT_HOURS, false)
    }

    /// The question.
    #[must_use]
    pub fn question(&self) -> &str {
        &self.question
    }

    /// The answers, in their order.
    #[must_use]
    pub fn answers(&self) -> &[String] {
        &self.answers
    }

    /// How many hours the poll runs.
    #[must_use]
    pub const fn hours(&self) -> u16 {
        self.hours
    }

    /// Whether people may choose more than one answer.
    #[must_use]
    pub const fn multiple(&self) -> bool {
        self.multiple
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls() {
        let poll = Poll::new(" Pizza? ", &["Ja", " Nein "], 48, true).unwrap();
        assert_eq!(
            (
                poll.question(),
                poll.answers(),
                poll.hours(),
                poll.multiple()
            ),
            (
                "Pizza?",
                &["Ja".to_owned(), "Nein".to_owned()][..],
                48,
                true
            )
        );
        let poll = Poll::parse("Wer kommt? |Ich| Du |Alle").unwrap();
        assert_eq!(poll.question(), "Wer kommt?");
        assert_eq!(poll.answers(), ["Ich", "Du", "Alle"]);
        assert_eq!((poll.hours(), poll.multiple()), (DEFAULT_HOURS, false));
        // At the limits.
        let question = "?".repeat(MAX_QUESTION);
        let answer = "ä".repeat(MAX_ANSWER);
        let answers = vec![answer.as_str(); MAX_ANSWERS];
        assert!(Poll::new(&question, &answers, MAX_HOURS, false).is_ok());
        assert!(Poll::new("?", &["a", "b"], 1, false).is_ok());
    }

    #[test]
    fn what_is_no_poll() {
        for (question, answers) in [
            ("", &["a", "b"][..]),
            ("  ", &["a", "b"]),
            ("?", &["a"]),
            ("?", &[]),
            ("?", &["a", " "]),
        ] {
            assert_eq!(
                Poll::new(question, answers, 24, false),
                Err(Error::Invalid),
                "{question:?} {answers:?}"
            );
        }
        for text in [
            "",
            "Pizza?",
            "Pizza? | Ja",
            "| Ja | Nein",
            "Pizza? | Ja |",
            "Pizza? || Nein",
        ] {
            assert_eq!(Poll::parse(text), Err(Error::Invalid), "{text:?}");
        }
    }

    #[test]
    fn limits() {
        let long_question = "?".repeat(MAX_QUESTION + 1);
        let long_answer = "a".repeat(MAX_ANSWER + 1);
        let many = vec!["a"; MAX_ANSWERS + 1];
        for (question, answers, hours) in [
            (long_question.as_str(), vec!["a", "b"], 24),
            ("?", vec!["a", long_answer.as_str()], 24),
            ("?", many, 24),
            ("?", vec!["a", "b"], 0),
            ("?", vec!["a", "b"], MAX_HOURS + 1),
        ] {
            assert_eq!(
                Poll::new(question, &answers, hours, false),
                Err(Error::TooLarge),
                "{answers:?} {hours}"
            );
        }
    }
}
