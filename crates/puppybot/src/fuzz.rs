//! Entry points for the fuzz targets in `fuzz/`, compiled with `--cfg fuzzing`, which cargo fuzz
//! sets, and for the tests, which run each of them on a few inputs: a smoke test that needs
//! neither nightly Rust nor cargo-fuzz. See `fuzz/README.md`.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;

use crate::cli;

/// The command line, as arguments separated by NUL bytes, which arguments can't hold.
///
/// Checks beyond not crashing: the command line of what [`cli::parse`] understood is understood
/// the same, and [`crate::run`] returns one of the three exit statuses.
///
/// # Panics
///
/// If one of these checks fails, which is what the fuzz target looks for.
pub fn cli_args(data: &[u8]) {
    let args: Vec<OsString> = if data.is_empty() {
        Vec::new()
    } else {
        data.split(|&byte| byte == 0)
            .map(|arg| OsString::from_vec(arg.to_vec()))
            .collect()
    };
    if let Ok(action) = cli::parse(args.clone()) {
        assert_eq!(
            cli::parse(action.args().iter().map(OsString::from)),
            Ok(action)
        );
    }
    let status = crate::run(args, &mut io::sink(), &mut io::sink());
    assert!(matches!(
        status,
        crate::EXIT_SUCCESS | crate::EXIT_FAILURE | crate::EXIT_USAGE
    ));
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_args_smoke_test() {
        let inputs: [&[u8]; 8] = [
            b"",
            b"--version",
            b"-h",
            b"--help\0--version",
            b"--bogus",
            b"\0",
            b"-V\xff",
            b"\xff\xfe\0\0-h",
        ];
        for input in inputs {
            super::cli_args(input);
        }
    }
}
