//! Tests of PuppyBot on Discord, with Discord on this machine, see `common`: what it registers,
//! which servers it leaves, how it answers, and how it handles what goes wrong.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Clippy counts only the #[test] functions as tests, not their helpers in this file"
)]

mod common;

use std::fs;
use std::time::{Duration, Instant};

use common::{
    APPLICATION, CHANNEL_MESSAGES, FakeDiscord, GUILD, MANAGE_GUILD, MENTION, OTHER_GUILD, Running,
    SETTINGS_ANSWER, Send, TOKEN, config, guild_create, interaction, message, ready, settings,
    setup,
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

    // It identifies with the token, and the intents GUILDS, GUILD_MESSAGES and MESSAGE_CONTENT.
    bot.wait_for("registered the commands");
    let identify = &discord.received()[0];
    assert!(
        identify.contains(&format!(r#""token":"Bot {TOKEN}""#)),
        "{identify}"
    );
    assert!(identify.contains(r#""intents":33281,"#), "{identify}");

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
    // /settings, which Discord shows only to people who may manage the server.
    assert!(
        commands.body.contains(r#""name":"settings""#),
        "{}",
        commands.body
    );
    assert!(
        commands
            .body
            .contains(r#""default_member_permissions":"32""#),
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
                message(1, "!info", GUILD, false),
            ]
        },
        Some(400),
    );
    let (_dir, mut bot) = start(&discord);
    // The last of the script.
    bot.wait_for("can't answer a text command");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    for warning in [
        "can't remove the global commands",
        "can't register the commands",
        "can't leave the server",
        "can't answer a command",
        "can't answer a text command",
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

#[test]
fn the_language_of_a_server_stays_after_a_restart() {
    let discord = FakeDiscord::start(
        |url| {
            vec![
                ready(url),
                settings("language", Some("de"), "en-US", MANAGE_GUILD),
            ]
        },
        None,
    );
    let (dir, mut bot) = start(&discord);
    // In the language of the person who asked.
    let answer = discord.wait_for_request(SETTINGS_ANSWER);
    assert!(
        answer
            .body
            .contains("From now on, PuppyBot writes to everyone on this server in German."),
        "{}",
        answer.body
    );
    bot.wait_for("changed the language of the server");
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(
        log.contains(&format!(
            "changed the language of the server server={GUILD} language=\"de\""
        )),
        "{log}"
    );

    // Again, with the same database.
    let discord = FakeDiscord::start(
        |url| vec![ready(url), settings("language", None, "de", MANAGE_GUILD)],
        None,
    );
    fs::write(dir.path().join("config.toml"), config(Some(&discord), "")).unwrap();
    let bot = Running::start(dir.path(), &[]);
    let answer = discord.wait_for_request(SETTINGS_ANSWER);
    assert!(
        answer
            .body
            .contains("Auf diesem Server schreibt PuppyBot allen auf Deutsch."),
        "{}",
        answer.body
    );
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
}

#[test]
fn settings_only_for_people_who_may_manage_the_server() {
    let discord = FakeDiscord::start(
        |url| vec![ready(url), settings("language", Some("en"), "de", 0)],
        None,
    );
    let (_dir, bot) = start(&discord);
    let answer = discord.wait_for_request(SETTINGS_ANSWER);
    assert!(
        answer.body.contains(
            "Die Einstellungen von PuppyBot dürfen nur Leute nutzen, die diesen Server verwalten dürfen."
        ),
        "{}",
        answer.body
    );
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(!log.contains("changed the language"), "{log}");
}

#[test]
fn text_commands_after_the_prefix_or_a_mention() {
    let discord = FakeDiscord::start(
        |url| {
            vec![
                ready(url),
                // Not for PuppyBot: a bot, another server, another command, no command.
                message(1, "!info", GUILD, true),
                message(2, "!info", OTHER_GUILD, false),
                message(3, "!play some song", GUILD, false),
                message(4, "! info", GUILD, false),
                message(5, "hello", GUILD, false),
                // For PuppyBot.
                message(6, "!info", GUILD, false),
                message(7, &format!("{MENTION} Hilfe"), GUILD, false),
            ]
        },
        None,
    );
    let (_dir, bot) = start(&discord);
    let deadline = Instant::now() + Duration::from_secs(30);
    let answers = loop {
        let answers: Vec<_> = discord
            .requests()
            .into_iter()
            .filter(|request| format!("{} {}", request.method, request.path) == CHANNEL_MESSAGES)
            .collect();
        if answers.len() >= 2 || Instant::now() > deadline {
            break answers;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert_eq!(answers.len(), 2, "{answers:#?}");

    // In English, the language of a server that chose none, as a reply that mentions nobody.
    let info = &answers[0].body;
    assert!(
        info.contains("a Discord bot of the Purple Puppy Project"),
        "{info}"
    );
    assert!(info.contains(r#""message_reference":{"#), "{info}");
    assert!(info.contains(r#""message_id":"6""#), "{info}");
    assert!(
        info.contains(r#""allowed_mentions":{"parse":[]}"#),
        "{info}"
    );
    let help = &answers[1].body;
    assert!(help.contains("`!help`: these commands"), "{help}");
    assert!(help.contains(r#""message_id":"7""#), "{help}");
}

#[test]
fn the_prefix_and_the_language_of_a_server() {
    let discord = FakeDiscord::start(
        |url| {
            vec![
                ready(url),
                settings("prefix", Some("pb?"), "en-US", MANAGE_GUILD),
                settings("language", Some("de"), "en-US", MANAGE_GUILD),
                // The old prefix no longer, the new one in the language of the server.
                message(1, "!info", GUILD, false),
                message(2, "pb?info", GUILD, false),
            ]
        },
        None,
    );
    let (_dir, bot) = start(&discord);
    let answer = discord.wait_for_request(CHANNEL_MESSAGES);
    assert!(
        answer
            .body
            .contains("ein Discord-Bot des Purple Puppy Project"),
        "{}",
        answer.body
    );
    assert!(
        answer.body.contains(r#""message_id":"2""#),
        "{}",
        answer.body
    );
    let (status, log) = bot.stop_with("TERM");
    assert_eq!(status, Some(0), "{log}");
    assert!(
        log.contains(&format!(
            "changed the prefix of the server server={GUILD} prefix=\"pb?\""
        )),
        "{log}"
    );
    let settings_answers: Vec<_> = discord
        .requests()
        .into_iter()
        .filter(|request| format!("{} {}", request.method, request.path) == SETTINGS_ANSWER)
        .collect();
    assert!(
        settings_answers[0]
            .body
            .contains("From now on, PuppyBot's text commands start with `pb?` on this server"),
        "{settings_answers:#?}"
    );
    let answers = discord
        .requests()
        .into_iter()
        .filter(|request| format!("{} {}", request.method, request.path) == CHANNEL_MESSAGES)
        .count();
    assert_eq!(answers, 1);
}
