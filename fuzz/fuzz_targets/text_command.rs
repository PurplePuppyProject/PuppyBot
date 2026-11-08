//! A prefix and a message, see `puppybot::fuzz::text_command`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::text_command(data));
