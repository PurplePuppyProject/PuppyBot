//! Tests of PuppyBot on Discord, with Discord on this machine, see `common`: what it registers,
//! which servers it leaves, how it answers, and how it handles what goes wrong.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Clippy counts only the #[test] functions as tests, not their helpers in this file"
)]

mod common;

use std::time::{Duration, Instant};

use common::{
    APPLICATION, FakeDiscord, GUILD, OTHER_GUILD, Running, Send, TOKEN, config, guild_create,
    interaction, ready, setup,
};

/// Starts PuppyBot with `discord`.
fn start(discord: &FakeDiscord) -> (tempfile::TempDir, Running) {
    let dir = setup(&config(Some(discord), ""), Some((TOKEN, 0o600)));
    let bot = Running::start(dir.path(), &[]);
    (dir, bot)
}

#[test]
fn registers_the_commands_leaves_other_servers_and_answers() {
    let discord = FakeDiscord::start(
        |url| {
            vec![
                ready(url),
                guild_create(GUILD),
                guild_create(OTHER_GUILD),
                // A button, which isn't a slash command: no answer.
                interaction(3, "", "de", GUILD),
                interaction(2, "info", "de", GUILD),
            ]
        },
        None,
    );
    let (_dir, mut bot) = start(&discord);

    // It identifies with the token and only the intent GUILDS.
    bot.wait_for("registered the commands");
    let identify = &discord.received()[0];
    assert!(
        identify.contains(&format!(r#""token":"Bot {TOKEN}""#)),
        "{identify}"
    );
    assert!(identify.contains(r#""intents":1,"#), "{identify}");

    // No global commands, and the commands on its server, in English and German.
    let global =
        discord.wait_for_request(&format!("PUT /api/v10/applications/{APPLICATION}/commands"));
    assert_eq!(global.body, "[]");
    let commands = discord.wait_for_request(&format!(
        "PUT /api/v10/applications/{APPLICATION}/guilds/{GUILD}/commands"
    ));
    assert!(
        commands.body.contains(r#""name":"info""#),
        "{}",
        commands.body
    );
    assert!(
        commands
            .body
            .contains("Zeigt, was PuppyBot ist und welche Version läuft"),
        "{}",
        commands.body
    );

    // It leaves the server that isn't its own, and only that one.
    discord.wait_for_request(&format!("DELETE /api/v10/users/@me/guilds/{OTHER_GUILD}"));
    // The answer to /info, in German, only for the person who asked.
    let answer = discord.wait_for_request(
        "POST /api/v10/interactions/1200000000000000005/interaction-token/callback",
    );
    assert!(answer.body.contains(r#""type":4"#), "{}", answer.body);
    assert!(answer.body.contains(r#""flags":64"#), "{}", answer.body);
    assert!(
        answer
            .body
            .contains("ein Discord-Bot des Purple Puppy Project"),
        "{}",
        answer.body
    );
    assert!(
        answer.body.contains(r#""allowed_mentions":{"parse":[]}"#),
        "{}",
        answer.body
    );

    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(log.contains("connected to Discord user=PuppyBot"), "{log}");
    assert!(
        log.contains(&format!(
            "leaving a server that isn't in [discord] guilds server={OTHER_GUILD}"
        )),
        "{log}"
    );
    let requests = discord.requests();
    assert_eq!(requests.len(), 4, "{requests:#?}");
    assert!(!log.contains(TOKEN), "{log}");
}

#[test]
fn discord_rejects_the_token() {
    let discord = FakeDiscord::start(|_| vec![Send::Close(4004, "Authentication failed.")], None);
    let (_dir, bot) = start(&discord);
    let (status, log) = bot.wait();
    assert_eq!(status, Some(1), "{log}");
    assert!(
        log.contains("PuppyBot can't run: Discord closed the connection for good with the code 4004: Authentication failed."),
        "{log}"
    );
}

#[test]
fn failing_requests_are_warnings() {
    let discord = FakeDiscord::start(
        |url| {
            vec![
                ready(url),
                guild_create(OTHER_GUILD),
                interaction(2, "info", "en-US", GUILD),
            ]
        },
        Some(400),
    );
    let (_dir, mut bot) = start(&discord);
    bot.wait_for("can't answer a command");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    for warning in [
        "can't remove the global commands",
        "can't register the commands",
        "can't leave the server",
        "can't answer a command",
    ] {
        assert!(log.contains(warning), "{warning}: {log}");
    }
}

#[test]
fn events_that_are_not_valid_are_warnings() {
    let discord = FakeDiscord::start(
        |url| {
            vec![
                ready(url),
                Send::Text(
                    r#"{"op": 0, "s": 2, "t": "INTERACTION_CREATE", "d": {"id": 1}}"#.to_owned(),
                ),
            ]
        },
        None,
    );
    let (_dir, mut bot) = start(&discord);
    bot.wait_for("can't read an event of Discord");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
}

#[test]
fn stops_although_discord_does_not_close_the_connection() {
    let discord = FakeDiscord::start(|url| vec![ready(url), Send::Silence], None);
    let (_dir, mut bot) = start(&discord);
    bot.wait_for("connected to Discord");
    let start = Instant::now();
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    // After waiting for Discord, for 5 seconds; Docker's own timeout is 10.
    assert!(
        start.elapsed() < Duration::from_secs(9),
        "{:?}",
        start.elapsed()
    );
}
