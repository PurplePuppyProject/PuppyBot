//! An interaction, as JSON from Discord's gateway, see `puppybot::fuzz::interaction`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::interaction(data));
