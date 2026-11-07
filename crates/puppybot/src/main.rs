//! The program `puppybot`. What it does is in the library, see `src/lib.rs`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let status = puppybot::run(
        std::env::args_os().skip(1),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    ExitCode::from(status)
}
