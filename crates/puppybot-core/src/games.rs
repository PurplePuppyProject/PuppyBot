//! Small games: a coin, the magic 8-ball, rock, paper, scissors against PuppyBot, and guessing a
//! number. The randomness comes from the platform, as for [`crate::dice`]: `random(n)` gives a
//! number from 1 to `n`, or none if there is no randomness.
//!
//! Their texts are an enum of their own, [`GameText`], in German and English.

use crate::i18n::Lang;
use crate::text::{MAX_NAME, literal};

/// The most characters of a question to the 8-ball in its answer.
pub const MAX_QUESTION: usize = 300;

/// A side of a coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coin {
    /// Heads.
    Heads,
    /// Tails.
    Tails,
}

impl Coin {
    /// Flips a coin.
    pub fn flip(random: &mut dyn FnMut(u32) -> Option<u32>) -> Option<Self> {
        random(2).map(|side| if side == 1 { Self::Heads } else { Self::Tails })
    }
}

/// How many answers the magic 8-ball has.
pub const EIGHT_BALL: usize = 20;

/// The number of an answer of the magic 8-ball, below [`EIGHT_BALL`].
pub fn eight_ball(random: &mut dyn FnMut(u32) -> Option<u32>) -> Option<usize> {
    let answers = u32::try_from(EIGHT_BALL).unwrap_or(1);
    let answer = random(answers)?.clamp(1, answers);
    usize::try_from(answer - 1).ok()
}

/// A hand of rock, paper, scissors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    /// Rock, which beats scissors.
    Rock,
    /// Paper, which beats rock.
    Paper,
    /// Scissors, which beat paper.
    Scissors,
}

/// How a game of rock, paper, scissors ended, for the person who played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// They won.
    Won,
    /// PuppyBot won.
    Lost,
    /// Both chose the same hand.
    Draw,
}

impl Hand {
    /// All hands.
    pub const ALL: [Self; 3] = [Self::Rock, Self::Paper, Self::Scissors];

    /// The hand's name in IDs, e.g. of buttons: `rock`, `paper` or `scissors`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Rock => "rock",
            Self::Paper => "paper",
            Self::Scissors => "scissors",
        }
    }

    /// The hand with the name `id`, see [`Self::id`].
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|hand| hand.id() == id)
    }

    /// A hand that PuppyBot chooses.
    pub fn choose(random: &mut dyn FnMut(u32) -> Option<u32>) -> Option<Self> {
        Some(match random(3)? {
            1 => Self::Rock,
            2 => Self::Paper,
            _ => Self::Scissors,
        })
    }

    /// How a game of this hand against `other` ends, for this hand.
    #[must_use]
    pub const fn against(self, other: Self) -> Outcome {
        match (self, other) {
            (Self::Rock, Self::Scissors)
            | (Self::Paper, Self::Rock)
            | (Self::Scissors, Self::Paper) => Outcome::Won,
            (Self::Rock, Self::Rock)
            | (Self::Paper, Self::Paper)
            | (Self::Scissors, Self::Scissors) => Outcome::Draw,
            _ => Outcome::Lost,
        }
    }
}

/// A game of guessing a number from 1 to its largest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guessing {
    secret: u32,
    max: u32,
    tries: u32,
}

/// What PuppyBot says to a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// The number is higher.
    Higher,
    /// The number is lower.
    Lower,
    /// It is the number, after so many guesses.
    Right {
        /// The guesses, this one too.
        tries: u32,
    },
    /// The guess is outside the numbers of the game.
    Outside,
}

impl Guessing {
    /// The largest number of a game if nobody says.
    pub const DEFAULT_MAX: u32 = 100;

    /// The largest number of a game at most.
    pub const MAX: u32 = 1_000_000;

    /// A game of a number from 1 to `max`, held to 2 to [`Self::MAX`].
    pub fn new(max: u32, random: &mut dyn FnMut(u32) -> Option<u32>) -> Option<Self> {
        let max = max.clamp(2, Self::MAX);
        let secret = random(max)?.clamp(1, max);
        Some(Self {
            secret,
            max,
            tries: 0,
        })
    }

    /// The largest number of the game.
    #[must_use]
    pub const fn max(&self) -> u32 {
        self.max
    }

    /// A guess of `number`. Guesses outside the numbers don't count.
    pub fn guess(&mut self, number: u32) -> Hint {
        if !(1..=self.max).contains(&number) {
            return Hint::Outside;
        }
        self.tries += 1;
        match number.cmp(&self.secret) {
            std::cmp::Ordering::Less => Hint::Higher,
            std::cmp::Ordering::Greater => Hint::Lower,
            std::cmp::Ordering::Equal => Hint::Right { tries: self.tries },
        }
    }
}

/// A text of the games that users see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameText<'a> {
    /// The description of /coin.
    CoinDescription,
    /// What the coin shows.
    Coin(Coin),
    /// The description of /8ball.
    EightBallDescription,
    /// The description of the option `question` of /8ball.
    EightBallQuestionDescription,
    /// The answer of the magic 8-ball to `question`.
    EightBall {
        /// The question, if someone asked one.
        question: &'a str,
        /// The number of the answer, below [`EIGHT_BALL`].
        answer: usize,
    },
    /// The description of /rps.
    RpsDescription,
    /// The message with the buttons of the hands.
    RpsChoose,
    /// The name of a hand, e.g. on its button.
    Hand(Hand),
    /// A game against PuppyBot.
    RpsPlayed {
        /// The name of the person who played.
        player: &'a str,
        /// Their hand.
        hand: Hand,
        /// PuppyBot's hand.
        bot: Hand,
    },
    /// The description of /guess.
    GuessDescription,
    /// The description of the option `number` of /guess.
    GuessNumberDescription,
    /// The description of the option `max` of /guess.
    GuessMaxDescription,
    /// A new game of guessing.
    GuessStarted {
        /// The largest number.
        max: u32,
    },
    /// A guess without a number, while a game runs.
    GuessRunning {
        /// The largest number.
        max: u32,
    },
    /// The answer to a guess.
    Guessed {
        /// The name of the person who guessed.
        player: &'a str,
        /// The guess.
        number: u32,
        /// What PuppyBot says to it.
        hint: Hint,
        /// The largest number of the game.
        max: u32,
    },
    /// The answer when PuppyBot can't play, as the system has no randomness.
    GameFailed,
}

/// The answers of the magic 8-ball in German.
const EIGHT_BALL_DE: [&str; EIGHT_BALL] = [
    "Es ist sicher.",
    "Ganz bestimmt.",
    "Ohne jeden Zweifel.",
    "Ja, auf jeden Fall.",
    "Darauf kannst du dich verlassen.",
    "So wie ich es sehe, ja.",
    "Höchstwahrscheinlich.",
    "Die Aussichten sind gut.",
    "Ja.",
    "Die Zeichen deuten auf Ja.",
    "Die Antwort ist unklar, frag noch einmal.",
    "Frag später noch einmal.",
    "Das sage ich dir jetzt lieber nicht.",
    "Das kann ich jetzt nicht vorhersagen.",
    "Konzentrier dich und frag noch einmal.",
    "Verlass dich nicht darauf.",
    "Meine Antwort ist Nein.",
    "Meine Quellen sagen Nein.",
    "Die Aussichten sind nicht so gut.",
    "Sehr zweifelhaft.",
];

/// The answers of the magic 8-ball in English.
const EIGHT_BALL_EN: [&str; EIGHT_BALL] = [
    "It is certain.",
    "It is decidedly so.",
    "Without a doubt.",
    "Yes, definitely.",
    "You may rely on it.",
    "As I see it, yes.",
    "Most likely.",
    "Outlook good.",
    "Yes.",
    "Signs point to yes.",
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Outlook not so good.",
    "Very doubtful.",
];

impl GameText<'_> {
    /// All texts, with an example for each value they hold.
    pub const ALL: [GameText<'static>; 26] = [
        GameText::CoinDescription,
        GameText::Coin(Coin::Heads),
        GameText::Coin(Coin::Tails),
        GameText::EightBallDescription,
        GameText::EightBallQuestionDescription,
        GameText::EightBall {
            question: "Wird es sonnig?",
            answer: 0,
        },
        GameText::EightBall {
            question: "",
            answer: EIGHT_BALL - 1,
        },
        GameText::RpsDescription,
        GameText::RpsChoose,
        GameText::Hand(Hand::Rock),
        GameText::Hand(Hand::Paper),
        GameText::Hand(Hand::Scissors),
        GameText::RpsPlayed {
            player: "Puppy",
            hand: Hand::Rock,
            bot: Hand::Scissors,
        },
        GameText::RpsPlayed {
            player: "Puppy",
            hand: Hand::Rock,
            bot: Hand::Paper,
        },
        GameText::RpsPlayed {
            player: "Puppy",
            hand: Hand::Paper,
            bot: Hand::Paper,
        },
        GameText::GuessDescription,
        GameText::GuessNumberDescription,
        GameText::GuessMaxDescription,
        GameText::GuessStarted { max: 100 },
        GameText::GuessRunning { max: 100 },
        GameText::Guessed {
            player: "Puppy",
            number: 30,
            hint: Hint::Higher,
            max: 100,
        },
        GameText::Guessed {
            player: "Puppy",
            number: 70,
            hint: Hint::Lower,
            max: 100,
        },
        GameText::Guessed {
            player: "Puppy",
            number: 42,
            hint: Hint::Right { tries: 3 },
            max: 100,
        },
        GameText::Guessed {
            player: "Puppy",
            number: 7,
            hint: Hint::Right { tries: 1 },
            max: 100,
        },
        GameText::Guessed {
            player: "Puppy",
            number: 101,
            hint: Hint::Outside,
            max: 100,
        },
        GameText::GameFailed,
    ];

    /// The descriptions in the list of commands, which Discord limits to 100 characters.
    pub const DESCRIPTIONS: [GameText<'static>; 7] = [
        GameText::CoinDescription,
        GameText::EightBallDescription,
        GameText::EightBallQuestionDescription,
        GameText::RpsDescription,
        GameText::GuessDescription,
        GameText::GuessNumberDescription,
        GameText::GuessMaxDescription,
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
        match *self {
            Self::CoinDescription => "Wirft eine Münze: Kopf oder Zahl".to_owned(),
            Self::Coin(Coin::Heads) => "Die Münze zeigt **Kopf**.".to_owned(),
            Self::Coin(Coin::Tails) => "Die Münze zeigt **Zahl**.".to_owned(),
            Self::EightBallDescription => "Fragt die magische 8-Kugel".to_owned(),
            Self::EightBallQuestionDescription => "Was du die 8-Kugel fragst".to_owned(),
            Self::EightBall { question, answer } => {
                eight_ball_text(question, EIGHT_BALL_DE.get(answer).unwrap_or(&"…"))
            }
            Self::RpsDescription => "Schere, Stein, Papier gegen PuppyBot".to_owned(),
            Self::RpsChoose => "Schere, Stein oder Papier?".to_owned(),
            Self::Hand(Hand::Rock) => "Stein".to_owned(),
            Self::Hand(Hand::Paper) => "Papier".to_owned(),
            Self::Hand(Hand::Scissors) => "Schere".to_owned(),
            Self::RpsPlayed { player, hand, bot } => {
                let player = literal(player, MAX_NAME);
                let end = match hand.against(bot) {
                    Outcome::Won => format!("**{player} gewinnt!**"),
                    Outcome::Lost => "**PuppyBot gewinnt!**".to_owned(),
                    Outcome::Draw => "**Unentschieden!**".to_owned(),
                };
                format!(
                    "**{player}** wählt {}, PuppyBot {}: {end}",
                    Self::Hand(hand).de(),
                    Self::Hand(bot).de()
                )
            }
            Self::GuessDescription => {
                "Zahlenraten: ohne Zahl ein neues Spiel, mit Zahl ein Tipp".to_owned()
            }
            Self::GuessNumberDescription => "Dein Tipp".to_owned(),
            Self::GuessMaxDescription => {
                "Die größte Zahl eines neuen Spiels, ohne es 100".to_owned()
            }
            Self::GuessStarted { max } => {
                format!("PuppyBot denkt sich eine Zahl von 1 bis {max}. Ratet mit einer Zahl dazu!")
            }
            Self::GuessRunning { max } => {
                format!("Hier läuft ein Spiel: gesucht ist eine Zahl von 1 bis {max}.")
            }
            Self::Guessed {
                player,
                number,
                hint,
                max,
            } => match hint {
                Hint::Higher => format!("{number}? Die Zahl ist höher."),
                Hint::Lower => format!("{number}? Die Zahl ist niedriger."),
                Hint::Right { tries } => {
                    let player = literal(player, MAX_NAME);
                    if tries == 1 {
                        format!("**{player}** hat es: **{number}**, gleich beim ersten Tipp!")
                    } else {
                        format!("**{player}** hat es: **{number}**, nach {tries} Tipps!")
                    }
                }
                Hint::Outside => format!("Die Zahl ist zwischen 1 und {max}."),
            },
            Self::GameFailed => {
                "PuppyBot kann gerade nicht spielen. Bitte später noch einmal versuchen.".to_owned()
            }
        }
    }

    fn en(&self) -> String {
        match *self {
            Self::CoinDescription => "Flips a coin: heads or tails".to_owned(),
            Self::Coin(Coin::Heads) => "The coin shows **heads**.".to_owned(),
            Self::Coin(Coin::Tails) => "The coin shows **tails**.".to_owned(),
            Self::EightBallDescription => "Asks the magic 8-ball".to_owned(),
            Self::EightBallQuestionDescription => "What you ask the 8-ball".to_owned(),
            Self::EightBall { question, answer } => {
                eight_ball_text(question, EIGHT_BALL_EN.get(answer).unwrap_or(&"…"))
            }
            Self::RpsDescription => "Rock, paper, scissors against PuppyBot".to_owned(),
            Self::RpsChoose => "Rock, paper or scissors?".to_owned(),
            Self::Hand(Hand::Rock) => "Rock".to_owned(),
            Self::Hand(Hand::Paper) => "Paper".to_owned(),
            Self::Hand(Hand::Scissors) => "Scissors".to_owned(),
            Self::RpsPlayed { player, hand, bot } => {
                let player = literal(player, MAX_NAME);
                let end = match hand.against(bot) {
                    Outcome::Won => format!("**{player} wins!**"),
                    Outcome::Lost => "**PuppyBot wins!**".to_owned(),
                    Outcome::Draw => "**A draw!**".to_owned(),
                };
                format!(
                    "**{player}** chose {}, PuppyBot {}: {end}",
                    Self::Hand(hand).en().to_lowercase(),
                    Self::Hand(bot).en().to_lowercase()
                )
            }
            Self::GuessDescription => {
                "Guessing a number: without a number a new game, with one a guess".to_owned()
            }
            Self::GuessNumberDescription => "Your guess".to_owned(),
            Self::GuessMaxDescription => {
                "The largest number of a new game, 100 without it".to_owned()
            }
            Self::GuessStarted { max } => {
                format!("PuppyBot thinks of a number from 1 to {max}. Guess it with a number!")
            }
            Self::GuessRunning { max } => {
                format!("A game runs here: the number is from 1 to {max}.")
            }
            Self::Guessed {
                player,
                number,
                hint,
                max,
            } => match hint {
                Hint::Higher => format!("{number}? The number is higher."),
                Hint::Lower => format!("{number}? The number is lower."),
                Hint::Right { tries } => {
                    let player = literal(player, MAX_NAME);
                    if tries == 1 {
                        format!("**{player}** got it: **{number}**, on the first guess!")
                    } else {
                        format!("**{player}** got it: **{number}**, after {tries} guesses!")
                    }
                }
                Hint::Outside => format!("The number is between 1 and {max}."),
            },
            Self::GameFailed => "PuppyBot can't play right now. Please try again later.".to_owned(),
        }
    }
}

/// The answer of the 8-ball, after the question in italics, if there is one, as it is.
fn eight_ball_text(question: &str, answer: &str) -> String {
    let question = literal(question.trim(), MAX_QUESTION);
    let question = question.trim();
    if question.is_empty() {
        format!("🎱 **{answer}**")
    } else {
        format!("🎱 *{question}* **{answer}**")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Randomness that gives `n` each time.
    fn always(n: u32) -> impl FnMut(u32) -> Option<u32> {
        move |_| Some(n)
    }

    #[test]
    fn coins() {
        assert_eq!(Coin::flip(&mut always(1)), Some(Coin::Heads));
        assert_eq!(Coin::flip(&mut always(2)), Some(Coin::Tails));
        assert_eq!(Coin::flip(&mut |_| None), None);
    }

    #[test]
    fn the_eight_ball() {
        assert_eq!(eight_ball(&mut always(1)), Some(0));
        assert_eq!(eight_ball(&mut always(20)), Some(19));
        // Out of range, held to it.
        assert_eq!(eight_ball(&mut always(99)), Some(19));
        assert_eq!(eight_ball(&mut |_| None), None);
    }

    #[test]
    fn rock_paper_scissors() {
        for hand in Hand::ALL {
            assert_eq!(Hand::from_id(hand.id()), Some(hand));
            assert_eq!(hand.against(hand), Outcome::Draw);
        }
        assert_eq!(Hand::from_id("lizard"), None);
        assert_eq!(Hand::Rock.against(Hand::Scissors), Outcome::Won);
        assert_eq!(Hand::Scissors.against(Hand::Paper), Outcome::Won);
        assert_eq!(Hand::Paper.against(Hand::Rock), Outcome::Won);
        assert_eq!(Hand::Rock.against(Hand::Paper), Outcome::Lost);
        let chosen: Vec<_> = (1..=3).map(|n| Hand::choose(&mut always(n))).collect();
        assert_eq!(
            chosen,
            [Some(Hand::Rock), Some(Hand::Paper), Some(Hand::Scissors)]
        );
        assert_eq!(Hand::choose(&mut |_| None), None);
    }

    #[test]
    fn guessing() {
        let mut game = Guessing::new(100, &mut always(42)).unwrap();
        assert_eq!(game.max(), 100);
        assert_eq!(game.guess(0), Hint::Outside);
        assert_eq!(game.guess(101), Hint::Outside);
        assert_eq!(game.guess(30), Hint::Higher);
        assert_eq!(game.guess(70), Hint::Lower);
        // Guesses outside the numbers didn't count.
        assert_eq!(game.guess(42), Hint::Right { tries: 3 });
        // Held to the limits.
        assert_eq!(Guessing::new(0, &mut always(1)).unwrap().max(), 2);
        assert_eq!(
            Guessing::new(u32::MAX, &mut always(1)).unwrap().max(),
            Guessing::MAX
        );
        assert_eq!(Guessing::new(10, &mut always(99)).unwrap().secret, 10);
        assert_eq!(Guessing::new(10, &mut |_| None), None);
    }

    #[test]
    fn every_text_in_every_language() {
        for text in GameText::ALL {
            let texts = Lang::ALL.map(|lang| text.render(lang));
            for rendered in &texts {
                assert!(!rendered.is_empty(), "{text:?}");
                for mention in ["@everyone", "@here", "<@"] {
                    assert!(!rendered.contains(mention), "{text:?}: {rendered}");
                }
            }
            assert_ne!(texts[0], texts[1], "{text:?} isn't translated");
        }
        for text in GameText::DESCRIPTIONS {
            for lang in Lang::ALL {
                assert!(text.render(lang).chars().count() <= 100, "{text:?}");
            }
        }
        // Every answer of the 8-ball, and none beyond.
        for answer in 0..EIGHT_BALL {
            let text = GameText::EightBall {
                question: "",
                answer,
            };
            assert_ne!(text.render(Lang::De), text.render(Lang::En));
        }
        let beyond = GameText::EightBall {
            question: "?",
            answer: EIGHT_BALL,
        };
        assert_eq!(beyond.render(Lang::En), "🎱 *?* **…**");
    }

    #[test]
    fn the_answers() {
        let played = GameText::RpsPlayed {
            player: "Puppy",
            hand: Hand::Rock,
            bot: Hand::Scissors,
        };
        assert_eq!(
            played.render(Lang::En),
            "**Puppy** chose rock, PuppyBot scissors: **Puppy wins!**"
        );
        assert_eq!(
            played.render(Lang::De),
            "**Puppy** wählt Stein, PuppyBot Schere: **Puppy gewinnt!**"
        );
        let ball = GameText::EightBall {
            question: " Wird es sonnig? ",
            answer: 8,
        };
        assert_eq!(ball.render(Lang::De), "🎱 *Wird es sonnig?* **Ja.**");
        let first = GameText::Guessed {
            player: "Puppy",
            number: 7,
            hint: Hint::Right { tries: 1 },
            max: 10,
        };
        assert_eq!(
            first.render(Lang::En),
            "**Puppy** got it: **7**, on the first guess!"
        );
        assert_eq!(
            first.render(Lang::De),
            "**Puppy** hat es: **7**, gleich beim ersten Tipp!"
        );
    }

    #[test]
    fn names_and_questions_as_they_are() {
        let played = GameText::RpsPlayed {
            player: "*Puppy* <@1>",
            hand: Hand::Paper,
            bot: Hand::Rock,
        };
        assert_eq!(
            played.render(Lang::En),
            r"**\*Puppy\* \<\@1\>** chose paper, PuppyBot rock: **\*Puppy\* \<\@1\> wins!**"
        );
        let guessed = GameText::Guessed {
            player: "__Puppy__",
            number: 3,
            hint: Hint::Right { tries: 2 },
            max: 10,
        };
        assert_eq!(
            guessed.render(Lang::De),
            r"**\_\_Puppy\_\_** hat es: **3**, nach 2 Tipps!"
        );
        let ball = GameText::EightBall {
            question: "[Sonne](https://example.com)?\u{7}",
            answer: 8,
        };
        assert_eq!(
            ball.render(Lang::De),
            r"🎱 *\[Sonne\](https\://example.com)?* **Ja.**"
        );
        // Only what Discord wouldn't show: no question.
        let ball = GameText::EightBall {
            question: "\u{7}\u{202e}",
            answer: 8,
        };
        assert_eq!(ball.render(Lang::En), "🎱 **Yes.**");
        let long = "*".repeat(MAX_QUESTION + 10);
        let ball = GameText::EightBall {
            question: &long,
            answer: 8,
        };
        assert_eq!(
            ball.render(Lang::En),
            format!("🎱 *{}* **Yes.**", r"\*".repeat(MAX_QUESTION))
        );
    }
}
