//! The command line of the program, see `puppybot::fuzz::cli_args`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::cli_args(data));
