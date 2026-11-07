//! The configuration file, see `puppybot::fuzz::config`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::config(data));
