//! An answer of GitHub's API, see `puppybot::fuzz::github_api`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| puppybot::fuzz::github_api(data));
