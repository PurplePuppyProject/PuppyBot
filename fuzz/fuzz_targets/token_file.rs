//! What a file with a secret holds, see `puppybot::fuzz::token_file`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::token_file(data));
