//! Dice: expressions like `2d6+3`, `d20`, `2W6` or `4d6kh3`, and their rolls.
//!
//! The random numbers come from the platform, which has the randomness of its system, so the
//! crate stays free of I/O and the tests can choose the numbers:
//!
//! ```
//! use puppybot_core::dice::Dice;
//!
//! let dice = Dice::parse("2d6+3").unwrap();
//! // Every die shows its most.
//! let roll = dice.roll(&mut |sides| Some(sides)).unwrap();
//! assert_eq!(roll.total(), 15);
//! assert_eq!(dice.to_string(), "2d6+3");
//! ```

use std::collections::BTreeSet;
use std::fmt;

/// The most dice that an expression may roll, all its terms together.
pub const MAX_DICE: u32 = 100;

/// The most sides that a die may have.
pub const MAX_SIDES: u32 = 1000;

/// The most terms, dice and numbers, that an expression may have.
pub const MAX_TERMS: usize = 20;

/// The largest number in an expression.
pub const MAX_NUMBER: u32 = 1_000_000;

/// The longest expression, in bytes.
pub const MAX_LENGTH: u8 = 100;

/// An expression of dice: terms that are added or subtracted, e.g. `2d6+3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dice {
    /// The terms with their signs; the first is always added.
    terms: Vec<(Sign, Term)>,
}

/// Whether a term is added or subtracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// Added.
    Plus,
    /// Subtracted.
    Minus,
}

/// A term of an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term {
    /// Dice, e.g. `4d6kh3`.
    Dice {
        /// How many dice.
        count: u32,
        /// How many sides each has.
        sides: u32,
        /// Which of the dice count, all if none.
        keep: Option<Keep>,
    },
    /// A number, e.g. `3`.
    Number(u32),
}

/// Which of a term's dice count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// The highest, so many, e.g. `kh3`.
    Highest(u32),
    /// The lowest, so many, e.g. `kl1`.
    Lowest(u32),
}

/// Why a text isn't an expression of dice that PuppyBot rolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// It isn't written as one.
    Invalid,
    /// It is one, but exceeds a limit, e.g. [`MAX_DICE`].
    TooLarge,
}

/// A roll of an expression: each term with what it showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roll {
    terms: Vec<(Sign, Rolled)>,
}

/// What a term showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rolled {
    /// The dice: each with what it showed, and whether it counts.
    Dice(Vec<(u32, bool)>),
    /// A number.
    Number(u32),
}

impl Dice {
    /// One die with six sides: what PuppyBot rolls if nobody says what.
    #[must_use]
    pub fn d6() -> Self {
        Self {
            terms: vec![(
                Sign::Plus,
                Term::Dice {
                    count: 1,
                    sides: 6,
                    keep: None,
                },
            )],
        }
    }

    /// The expression in `text`, see [`Self::parse`], or [`Self::d6`] if `text` is only spaces,
    /// e.g. as someone gave none.
    ///
    /// # Errors
    ///
    /// As [`Self::parse`].
    pub fn parse_or_d6(text: &str) -> Result<Self, Error> {
        if text.trim().is_empty() {
            Ok(Self::d6())
        } else {
            Self::parse(text)
        }
    }

    /// The expression in `text`, in any case, with spaces around the signs or not: terms joined
    /// by `+` or `-`, each a number, e.g. `3`, or dice, e.g. `2d6`, `d20` or `2W6`, of which
    /// `kh3` or `k3` keeps the highest 3, and `kl1` the lowest 1.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] if `text` isn't written so, [`Error::TooLarge`] if it exceeds a limit,
    /// e.g. [`MAX_DICE`].
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() > usize::from(MAX_LENGTH) {
            return Err(Error::TooLarge);
        }
        let text = text.to_ascii_lowercase();
        let mut chars = text.chars().peekable();
        let mut terms = Vec::new();
        let mut sign = Sign::Plus;
        loop {
            skip_spaces(&mut chars);
            terms.push((sign, term(&mut chars)?));
            skip_spaces(&mut chars);
            sign = match chars.next() {
                None => break,
                Some('+') => Sign::Plus,
                Some('-') => Sign::Minus,
                Some(_) => return Err(Error::Invalid),
            };
        }
        let dice: u64 = terms
            .iter()
            .map(|(_, term)| match term {
                Term::Dice { count, .. } => u64::from(*count),
                Term::Number(_) => 0,
            })
            .sum();
        if terms.len() > MAX_TERMS || dice > u64::from(MAX_DICE) {
            return Err(Error::TooLarge);
        }
        Ok(Self { terms })
    }

    /// Rolls the dice, with `random(sides)`, which shows a die with so many sides: a number
    /// from 1 to `sides`, or none if there is no randomness.
    ///
    /// Returns none if `random` returned none.
    pub fn roll(&self, random: &mut dyn FnMut(u32) -> Option<u32>) -> Option<Roll> {
        let mut terms = Vec::with_capacity(self.terms.len());
        for &(sign, term) in &self.terms {
            let rolled = match term {
                Term::Number(number) => Rolled::Number(number),
                Term::Dice { count, sides, keep } => {
                    let shown = (0..count)
                        .map(|_| random(sides).map(|side| side.clamp(1, sides)))
                        .collect::<Option<Vec<u32>>>()?;
                    let kept = kept(&shown, keep);
                    Rolled::Dice(shown.into_iter().zip(kept).collect())
                }
            };
            terms.push((sign, rolled));
        }
        Some(Roll { terms })
    }
}

impl fmt::Display for Dice {
    /// The expression without spaces, in lower case, e.g. `2d6+3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (sign, term)) in self.terms.iter().enumerate() {
            if index > 0 {
                f.write_str(sign.symbol())?;
            }
            match term {
                Term::Number(number) => write!(f, "{number}")?,
                Term::Dice { count, sides, keep } => {
                    if *count != 1 {
                        write!(f, "{count}")?;
                    }
                    write!(f, "d{sides}")?;
                    match keep {
                        Some(Keep::Highest(keep)) => write!(f, "kh{keep}")?,
                        Some(Keep::Lowest(keep)) => write!(f, "kl{keep}")?,
                        None => {}
                    }
                }
            }
        }
        Ok(())
    }
}

impl Sign {
    /// `+` or `-`.
    const fn symbol(self) -> &'static str {
        match self {
            Self::Plus => "+",
            Self::Minus => "-",
        }
    }
}

impl Roll {
    /// The sum of the terms, of the dice those that count.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.terms
            .iter()
            .map(|(sign, rolled)| {
                let value = match rolled {
                    Rolled::Number(number) => i64::from(*number),
                    Rolled::Dice(dice) => dice
                        .iter()
                        .filter(|(_, counts)| *counts)
                        .map(|(shown, _)| i64::from(*shown))
                        .sum(),
                };
                match sign {
                    Sign::Plus => value,
                    Sign::Minus => -value,
                }
            })
            .sum()
    }
}

impl fmt::Display for Roll {
    /// The terms and the total, in Discord's Markdown: dice in brackets, those that don't count
    /// struck through, e.g. `[4, ~~1~~] + 3 = **7**`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (sign, rolled)) in self.terms.iter().enumerate() {
            if index > 0 {
                write!(f, " {} ", sign.symbol())?;
            }
            match rolled {
                Rolled::Number(number) => write!(f, "{number}")?,
                Rolled::Dice(dice) => {
                    f.write_str("[")?;
                    for (position, (shown, counts)) in dice.iter().enumerate() {
                        if position > 0 {
                            f.write_str(", ")?;
                        }
                        if *counts {
                            write!(f, "{shown}")?;
                        } else {
                            write!(f, "~~{shown}~~")?;
                        }
                    }
                    f.write_str("]")?;
                }
            }
        }
        write!(f, " = **{}**", self.total())
    }
}

/// A number from 1 to `n`, each as likely, from `next`, which returns random numbers of 32 bits,
/// or none if there is no randomness. Numbers that would make some results likelier than others
/// are dropped, and `next` is asked again.
///
/// Returns none if `n` is 0, or `next` returned none.
pub fn uniform(n: u32, next: &mut dyn FnMut() -> Option<u32>) -> Option<u32> {
    let n = u64::from(n);
    if n == 0 {
        return None;
    }
    // The largest multiple of n up to 2^32: below it, each remainder is as likely.
    let zone = (1 << 32) / n * n;
    loop {
        let random = u64::from(next()?);
        if random < zone {
            return u32::try_from(random % n + 1).ok();
        }
    }
}

/// Skips the spaces at the start of `chars`.
fn skip_spaces(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.next_if(|c| c.is_whitespace()).is_some() {}
}

/// The number at the start of `chars`, none if there is none.
fn number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<Option<u32>, Error> {
    let mut digits = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        digits.push(digit);
    }
    if digits.is_empty() {
        return Ok(None);
    }
    // Leading zeros don't make a number larger. More than 7 digits are more than MAX_NUMBER,
    // and up to 7 fit into a u32; none are 0.
    let significant = digits.trim_start_matches('0');
    if significant.len() > 7 {
        return Err(Error::TooLarge);
    }
    let number = significant.parse::<u32>().unwrap_or(0);
    if number > MAX_NUMBER {
        return Err(Error::TooLarge);
    }
    Ok(Some(number))
}

/// The term at the start of `chars`: dice, e.g. `2d6kh1`, or a number.
fn term(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<Term, Error> {
    let count = number(chars)?;
    if chars.next_if(|c| matches!(c, 'd' | 'w')).is_none() {
        return count.map(Term::Number).ok_or(Error::Invalid);
    }
    let count = count.unwrap_or(1);
    let sides = number(chars)?.ok_or(Error::Invalid)?;
    if count == 0 || sides == 0 {
        return Err(Error::Invalid);
    }
    if count > MAX_DICE || sides > MAX_SIDES {
        return Err(Error::TooLarge);
    }
    let keep = if chars.next_if_eq(&'k').is_some() {
        // kh or k for the highest, kl for the lowest.
        let lowest = chars.next_if(|c| matches!(c, 'h' | 'l')) == Some('l');
        let keep = number(chars)?.ok_or(Error::Invalid)?;
        if keep == 0 || keep > count {
            return Err(Error::Invalid);
        }
        Some(if lowest {
            Keep::Lowest(keep)
        } else {
            Keep::Highest(keep)
        })
    } else {
        None
    };
    Ok(Term::Dice { count, sides, keep })
}

/// Which of the dice that showed `shown` count with `keep`: the highest or lowest so many, of
/// equal ones the first.
fn kept(shown: &[u32], keep: Option<Keep>) -> Vec<bool> {
    let Some(keep) = keep else {
        return vec![true; shown.len()];
    };
    let mut order: Vec<(usize, u32)> = shown.iter().copied().enumerate().collect();
    let count = match keep {
        Keep::Highest(count) => {
            order.sort_by_key(|&(_, shown)| std::cmp::Reverse(shown));
            count
        }
        Keep::Lowest(count) => {
            order.sort_by_key(|&(_, shown)| shown);
            count
        }
    };
    let chosen: BTreeSet<usize> = order
        .iter()
        .take(usize::try_from(count).unwrap_or(usize::MAX))
        .map(|&(index, _)| index)
        .collect();
    (0..shown.len())
        .map(|index| chosen.contains(&index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dice that show, one after the other, `shown`, and then 1.
    fn showing(shown: &[u32]) -> impl FnMut(u32) -> Option<u32> {
        let mut shown = shown.iter().copied();
        move |_| Some(shown.next().unwrap_or(1))
    }

    #[test]
    fn expressions() {
        for (text, written) in [
            ("2d6+3", "2d6+3"),
            ("d20", "d20"),
            ("1d20", "d20"),
            ("2W6", "2d6"),
            ("4d6kh3", "4d6kh3"),
            ("4D6K3", "4d6kh3"),
            ("2d20kl1", "2d20kl1"),
            ("3d8 - 2 + 1d4", "3d8-2+d4"),
            ("  5  ", "5"),
            ("0", "0"),
            ("007", "7"),
            ("100d1000", "100d1000"),
            ("1000000", "1000000"),
            (
                "d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6",
                "d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6+d6",
            ),
        ] {
            assert_eq!(
                Dice::parse(text).map(|dice| dice.to_string()),
                Ok(written.to_owned()),
                "{text:?}"
            );
        }
        assert_eq!(Dice::d6().to_string(), "d6");
        assert_eq!(Dice::parse_or_d6(" "), Ok(Dice::d6()));
        assert_eq!(Dice::parse_or_d6("2d4"), Dice::parse("2d4"));
        assert_eq!(Dice::parse_or_d6("2d"), Err(Error::Invalid));
    }

    #[test]
    fn what_is_no_expression() {
        for text in [
            "", " ", "d", "2d", "d0", "0d6", "2 d6", "2d 6", "2d6+", "+2d6", "2d6 3", "2d6kh",
            "2d6kh0", "2d6kh3", "2d6kx1", "2x6", "2d6*2", "zwei", "1.5", "d6!", "--1",
        ] {
            assert_eq!(Dice::parse(text), Err(Error::Invalid), "{text:?}");
        }
    }

    #[test]
    fn limits() {
        for text in [
            "101d6",
            "d1001",
            "1000001",
            "99999999999",
            "60d6+41d6",
            "1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1+1",
            &"1".repeat(101),
        ] {
            assert_eq!(Dice::parse(text), Err(Error::TooLarge), "{text:?}");
        }
    }

    #[test]
    fn rolls() {
        let dice = Dice::parse("2d6+3-1d4").unwrap();
        let roll = dice.roll(&mut showing(&[4, 2, 3])).unwrap();
        assert_eq!(roll.total(), 6);
        assert_eq!(roll.to_string(), "[4, 2] + 3 - [3] = **6**");

        // The highest 3 of 4, of equal ones the first.
        let dice = Dice::parse("4d6kh3").unwrap();
        let roll = dice.roll(&mut showing(&[1, 5, 5, 5])).unwrap();
        assert_eq!(roll.to_string(), "[~~1~~, 5, 5, 5] = **15**");
        let roll = dice.roll(&mut showing(&[6, 2, 6, 6])).unwrap();
        assert_eq!(roll.to_string(), "[6, ~~2~~, 6, 6] = **18**");
        let dice = Dice::parse("3d20kl1").unwrap();
        let roll = dice.roll(&mut showing(&[7, 3, 3])).unwrap();
        assert_eq!(roll.to_string(), "[~~7~~, 3, ~~3~~] = **3**");

        // Numbers out of range are held to it.
        let roll = Dice::d6().roll(&mut |_| Some(9)).unwrap();
        assert_eq!(roll.total(), 6);
        let roll = Dice::d6().roll(&mut |_| Some(0)).unwrap();
        assert_eq!(roll.total(), 1);
        // Without randomness, no roll.
        assert_eq!(Dice::d6().roll(&mut |_| None), None);
        assert_eq!(
            Dice::parse("3")
                .unwrap()
                .roll(&mut |_| None)
                .unwrap()
                .total(),
            3
        );
    }

    #[test]
    fn each_side_as_likely() {
        // 2^32 isn't a multiple of 6: the numbers from the last 2^32 % 6 = 4 are dropped.
        let mut numbers = vec![u32::MAX, u32::MAX - 3, 5].into_iter();
        assert_eq!(uniform(6, &mut || numbers.next()), Some(6));
        let mut numbers = vec![u32::MAX - 4].into_iter();
        assert_eq!(uniform(6, &mut || numbers.next()), Some(6));
        assert_eq!(uniform(6, &mut || Some(0)), Some(1));
        assert_eq!(uniform(1, &mut || Some(u32::MAX)), Some(1));
        assert_eq!(
            uniform(u32::MAX, &mut || Some(u32::MAX - 1)),
            Some(u32::MAX)
        );
        assert_eq!(uniform(0, &mut || Some(1)), None);
        assert_eq!(uniform(6, &mut || None), None);
    }
}
