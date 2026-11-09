//! A poll as a text command writes it, see `puppybot::fuzz::poll`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::poll(data));
