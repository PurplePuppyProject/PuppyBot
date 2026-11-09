//! An expression of dice, see `puppybot::fuzz::dice`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::dice(data));
