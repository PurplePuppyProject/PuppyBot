//! Tests of running the bot, each in a process of its own, as Docker runs it: the configuration
//! and the token file, the log, the exit statuses, and stopping on a signal. Discord runs on this
//! machine, see `common`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Clippy counts only the #[test] functions as tests, not their helpers in this file"
)]

mod common;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use common::{FakeDiscord, Running, TOKEN, config, ready, run_to_end, setup};

/// Discord, which says READY once PuppyBot identified.
fn discord() -> FakeDiscord {
    FakeDiscord::start(|url| vec![ready(url)], None)
}

#[test]
fn stops_on_sigterm() {
    let discord = discord();
    let dir = setup(&config(Some(&discord), ""), Some((TOKEN, 0o600)));
    let mut bot = Running::start(dir.path(), &[]);
    bot.wait_for("registered the commands");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(log.contains(" INFO "), "{log}");
    assert!(log.contains("PuppyBot stopped"), "{log}");
    assert!(log.contains("signal=\"SIGTERM\""), "{log}");
    assert!(!log.contains(TOKEN), "{log}");
    // Only the owner may read the token file: no warning.
    assert!(!log.contains("WARN"), "{log}");
    // It closed the connection, normally.
    assert!(
        discord
            .received()
            .iter()
            .any(|message| message.starts_with("close 1000")),
        "{:#?}",
        discord.received()
    );
}

#[test]
fn stops_on_sigint_and_logs_json() {
    let discord = discord();
    let config = config(Some(&discord), "[log]\nformat = \"json\"\n");
    let dir = setup(&config, Some((TOKEN, 0o400)));
    let mut bot = Running::start(dir.path(), &[]);
    bot.wait_for("registered the commands");
    let (status, log) = bot.stop_with("INT");
    assert_eq!(status, Some(0), "{log}");
    for line in log.lines() {
        assert!(line.starts_with('{') && line.ends_with('}'), "{line}");
    }
    assert!(log.contains("\"level\":\"INFO\""), "{log}");
    assert!(log.contains("\"message\":\"PuppyBot stopped\""), "{log}");
    assert!(log.contains("\"signal\":\"SIGINT\""), "{log}");
    assert!(!log.contains(TOKEN), "{log}");
}

#[test]
fn rust_log_takes_precedence_over_the_level() {
    let discord = discord();
    let config = config(Some(&discord), "[log]\nlevel = \"error\"\n");
    let dir = setup(&config, Some((TOKEN, 0o600)));
    let mut bot = Running::start(dir.path(), &[("RUST_LOG", OsStr::new("info"))]);
    bot.wait_for("registered the commands");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
}

#[test]
fn warns_of_a_token_file_that_others_can_read() {
    let discord = discord();
    let dir = setup(&config(Some(&discord), ""), Some((TOKEN, 0o644)));
    let mut bot = Running::start(dir.path(), &[]);
    bot.wait_for("registered the commands");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(log.contains(" WARN "), "{log}");
    assert!(
        log.contains("can be read or changed by other users (mode 644)"),
        "{log}"
    );
}

#[test]
fn the_level_of_the_configuration_holds_back_details() {
    // The warning about the token file is held back at the level error, the error isn't.
    let config = config(None, "[log]\nlevel = \"error\"\n");
    let dir = setup(&config, Some(("two words", 0o644)));
    let (status, log) = run_to_end(dir.path(), &[]);
    assert_eq!(status, Some(1), "{log}");
    assert!(!log.contains("WARN"), "{log}");
    assert!(log.contains(" ERROR "), "{log}");
}

#[test]
fn a_token_file_that_is_missing() {
    let dir = setup(&config(None, ""), None);
    let (status, log) = run_to_end(dir.path(), &[]);
    assert_eq!(status, Some(1), "{log}");
    let path = dir.path().join("token");
    assert!(
        log.contains(&format!(
            "PuppyBot can't run: can't read the Discord token from {}: ",
            path.display()
        )),
        "{log}"
    );
}

#[test]
fn a_token_file_that_holds_no_valid_token() {
    let dir = setup(&config(None, ""), Some(("Bot two words\n", 0o600)));
    let (status, log) = run_to_end(dir.path(), &[]);
    assert_eq!(status, Some(1), "{log}");
    assert!(log.contains("isn't printable ASCII"), "{log}");
    assert!(!log.contains("two words"), "{log}");
}

#[test]
fn no_ca_certificates() {
    let dir = setup(&config(None, ""), Some((TOKEN, 0o600)));
    // Neither the file nor the directory that rustls-native-certs then looks in.
    let missing = dir.path().join("missing");
    let envs = [
        ("SSL_CERT_FILE", missing.as_os_str()),
        ("SSL_CERT_DIR", missing.as_os_str()),
    ];
    let (status, log) = run_to_end(dir.path(), &envs);
    assert_eq!(status, Some(1), "{log}");
    assert!(
        log.contains(
            "PuppyBot can't run: found no CA certificates to check Discord's certificates with: "
        ),
        "{log}"
    );
}

#[test]
fn a_configuration_that_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let (status, err) = run_to_end(dir.path(), &[]);
    assert_eq!(status, Some(2), "{err}");
    let path = dir.path().join("config.toml");
    assert!(
        err.starts_with(&format!(
            "puppybot: can't read the configuration {}: ",
            path.display()
        )),
        "{err}"
    );
}

#[test]
fn a_configuration_with_a_mistake_names_its_place() {
    let dir = setup("[discord]\ntokn-file = \"token\"\n", None);
    let (status, err) = run_to_end(dir.path(), &[]);
    assert_eq!(status, Some(2), "{err}");
    assert!(
        err.contains("is invalid: TOML parse error at line 2"),
        "{err}"
    );
    assert!(err.contains("unknown field `tokn-file`"), "{err}");
}

#[test]
fn rust_log_that_is_invalid() {
    let dir = setup(&config(None, ""), Some((TOKEN, 0o600)));
    let (status, err) = run_to_end(dir.path(), &[("RUST_LOG", OsStr::new("puppybot=loud"))]);
    assert_eq!(status, Some(2), "{err}");
    assert!(
        err.starts_with("puppybot: RUST_LOG isn't a valid filter: "),
        "{err}"
    );

    let (status, err) = run_to_end(dir.path(), &[("RUST_LOG", OsStr::from_bytes(b"inf\xff"))]);
    assert_eq!(status, Some(2), "{err}");
    assert_eq!(err, "puppybot: RUST_LOG isn't UTF-8");
}
