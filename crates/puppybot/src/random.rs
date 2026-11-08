//! Random numbers, for dice and games: from the randomness of the system, through ring, which
//! TLS uses too. They can't be guessed from earlier ones, unlike those of a fast generator.

use ring::rand::{SecureRandom as _, SystemRandom};

/// A number of 32 bits, none if the system has no randomness, which Linux has once it started.
#[must_use]
pub fn next_u32() -> Option<u32> {
    let mut bytes = [0; 4];
    SystemRandom::new().fill(&mut bytes).ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// A die with `sides` sides: a number from 1 to `sides`, each as likely, see
/// [`puppybot_core::dice::uniform`].
#[must_use]
pub fn die(sides: u32) -> Option<u32> {
    puppybot_core::dice::uniform(sides, &mut next_u32)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_side_and_no_other() {
        let shown: BTreeSet<u32> = (0..1000).filter_map(|_| die(6)).collect();
        assert_eq!(shown, BTreeSet::from([1, 2, 3, 4, 5, 6]));
        assert_eq!(die(1), Some(1));
        assert_eq!(die(0), None);
    }
}
