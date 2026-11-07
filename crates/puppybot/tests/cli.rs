//! Tests of the program as it is started, e.g. by Docker: what it prints and its exit status.

use std::io;
use std::process::Command;

/// Starts the program with the arguments, waits for it to exit, and returns its exit status with
/// what it printed and its errors.
fn puppybot(args: &[&str]) -> io::Result<(Option<i32>, String, String)> {
    let output = Command::new(env!("CARGO_BIN_EXE_puppybot"))
        .args(args)
        .output()?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

#[test]
fn version() {
    let (status, out, err) = puppybot(&["--version"]).unwrap();
    assert_eq!(status, Some(0));
    assert_eq!(out, format!("puppybot {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(err, "");
}

#[test]
fn help() {
    let (status, out, err) = puppybot(&["--help"]).unwrap();
    assert_eq!(status, Some(0));
    assert!(out.starts_with("Usage: puppybot "), "{out}");
    assert_eq!(err, "");
}

#[test]
fn a_wrong_command_line_fails() {
    let (status, out, err) = puppybot(&["--bogus"]).unwrap();
    assert_eq!(status, Some(2));
    assert_eq!(out, "");
    assert!(
        err.starts_with("puppybot: unknown argument \"--bogus\"\n"),
        "{err}"
    );
}

#[test]
fn without_arguments_it_fails_until_the_bot_exists() {
    let (status, out, err) = puppybot(&[]).unwrap();
    assert_eq!(status, Some(1));
    assert_eq!(out, "");
    assert!(err.contains("doesn't exist yet"), "{err}");
}
