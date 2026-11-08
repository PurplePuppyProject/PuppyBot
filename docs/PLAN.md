# Plan

What was decided in the planning of PuppyBot on 2026-11-07, with the versions of that day, and the next steps. Versions move on: check them again when a step starts.

## Scope

- **Platforms:** Discord first. Signal may follow as a second adapter of the same logic; how is decided then, see [Open questions](#open-questions).
- **Deployment:** one instance for a few known Discord servers, with the settings of each server in the database. Under 100 servers, Discord needs no verification, and the privileged intents (Server Members, Message Content) can be switched on in the Developer Portal without applying.
- **The first version:**
  - Community and onboarding: welcome messages, self-roles with buttons or menus, confirming the rules, events and reminders.
  - Utility and fun: dice, polls, info commands, quotes, small games.
  - Integrations: new entries of RSS and Atom feeds, YouTube channels (through their feeds), Twitch streams (Helix API with an app token) and GitHub (Atom feeds of releases and commits), posted to channels.
  - Not in the first version: moderation, voice and music. Since March 2026, Discord requires end-to-end encryption (DAVE) for voice.
- **Interaction:** slash commands, context menu commands (e.g. "Apps → Save quote"), buttons and select menus, and prefix commands in messages, e.g. `!dice 2d6`. Prefix commands need the privileged Message Content intent. Their parser is platform-neutral, as Signal, which has no slash commands, will need it too.
- **Languages:** German and English. Interactions answer in the language of the person (their Discord locale); welcome messages and prefix commands in the language set for the server; English for every other language.

## Stack

| Area | Choice | Version on 2026-11-07 | Why |
|---|---|---|---|
| Discord | twilight: gateway, http, model, util, and twilight-interactions for commands | 0.18.0 | Modular, with plain data types: handlers are functions from Discord's models to answers, testable without Discord. serenity 0.12.5 and poise 0.7.1 are framework-heavy and face the breaking serenity 0.13. twilight 0.18 needs Rust 1.89, the oldest Rust that PuppyBot supports since `feature/discord_connection`. |
| Async runtime | tokio | 1.54.1 | What twilight uses. |
| TLS | rustls with ring as its cryptography | rustls 0.23.46, ring 0.17.14 | Less code than aws-lc-rs, the default of rustls, and a static musl build proven in vibe-portredirect. Accepted: no post-quantum key exchange, which ring lacks. The default features of rustls, and of the crates that use it, must be turned off; `deny.toml` bans aws-lc-rs. The CA certificates are the system's (twilight's `rustls-native-roots`), which the image has and which its base image updates, not a list compiled into the binary. |
| Storage | redb | 4.5.0 | Pure Rust, ACID transactions, one file in a volume, a backend in memory for tests. Queries over keys are built by hand. redb 4.5 needs Rust 1.90, the oldest Rust that PuppyBot supports since `feature/storage`. |
| Secrets | Files that the configuration names, e.g. Docker or Compose secrets in `/run/secrets/` | | Not shown by `docker inspect`, `/proc/<pid>/environ` or crash reports. |
| Configuration | A TOML file for the instance, `/etc/puppybot/config.toml` or the one `--config` names, with no settings from environment variables but `RUST_LOG`; the database for the settings of each server, changed with slash commands | serde 1.0.229, toml 1.2.1 | One place for every setting; unknown keys are errors. |
| Logging | tracing, to stderr: a line of text, or a JSON object per line | tracing 0.1.44, tracing-subscriber 0.3.23 | Docker collects stderr; JSON for log collectors. |
| Texts | Typed texts in `puppybot-core`: an enum per area, one exhaustive `match` per language | | A missing translation doesn't compile, nothing is parsed at run time, and tests reach every text. |
| Dates and times | Discord timestamps (`<t:…>`) | | Discord shows them in the language and time zone of each reader. |

Chosen in their steps, with the versions current then: the feed parser (e.g. feed-rs 3.0.0 or quick-xml 0.42.0), and the HTTP client for feeds and Twitch (with rustls and ring).

Decided in `feature/discord_connection`:

- **Intents:** only `GUILDS`, which isn't privileged: slash commands need no intent. Each step asks for the intents it needs.
- **No compression of the gateway:** with a few servers and `GUILDS`, there is little to save, and twilight's zstd is C. If it is needed later: zlib, preferably with zlib-rs, which is pure Rust.
- **Commands:** built with twilight-util's `CommandBuilder` while they have no options; twilight-interactions 0.18.0 is decided on with the first command that has options, `/settings language`.
- **The servers:** an allowlist in the configuration, `guilds`. The commands are registered only there, as commands of each server, which Discord updates at once, unlike global commands; PuppyBot removes its global commands, and leaves any other server.
- **Answers:** ephemeral, so only the person who asked sees them, and without mentions, also if a text ever holds one.

Decided in `feature/storage`:

- **The data have a version,** 1 so far, in the table `meta`. A PuppyBot converts the data of earlier versions when it opens them, and refuses those of versions it doesn't know, e.g. after going back to an older release, instead of misreading them.
- **redb checks the file whenever it opens it** (`set_check_integrity_on_open`): a damaged file is an error, where a plain open may panic.
- **The calls run on the thread of the runtime,** as they are short and settings change rarely. Feeds, which will write more often, may move them to `spawn_blocking`.
- **Commands stay with twilight-util's `CommandBuilder`,** also with options: reading `/settings language` takes a few lines with slice patterns, so twilight-interactions and its derive macros aren't needed yet.
- **`/settings`** is for people with "Manage Server" or "Administrator": Discord shows it only to them (`default_member_permissions`), and PuppyBot checks it again, as the server's admins can allow the command to others.
- **The language of a server** is for what PuppyBot writes to everyone, e.g. welcome messages. Answers stay in the language of the person who asked.

## Structure

A Cargo workspace:

- `crates/puppybot-core`: the logic, the parsers and the texts, without Discord, storage or I/O. `make lint` checks its dependencies.
- `crates/puppybot`: the program: the configuration, the Discord adapter, storage and feeds.
- Later `crates/puppybot-signal`, if Signal comes.

## Quality and Security

- Tests cover 100 % of lines and functions, on stable Rust, without exceptions.
- A fuzz target for every parser of input from outside: prefix commands, dice expressions, feeds, the configuration, and what comes later.
- CI as the README describes it, with zizmor for the workflows, the list of crates in the binaries (cargo-auditable) and an SBOM at the image.
- Integrations fetch only http(s) URLs of public addresses: nothing on loopback, private, link-local or other internal addresses, also not after a redirect or a new DNS answer, and with limits for size, time and redirects. So server admins can't make the bot fetch from the network it runs in.
- License GPL-3.0-only. Images for amd64 and arm64: servers, PCs, and Raspberry Pis 3 or newer with a 64-bit system.

## Next Steps

One branch and pull request each, in this order:

1. `feature/configuration`, done: the TOML configuration, secrets from files, logging, how the program starts and stops (signals, exit statuses), and an example for Docker Compose.
2. `feature/discord_connection`, done: twilight's gateway with the intents needed, reconnecting, the servers of the configuration, the slash commands registered with German and English descriptions, and a first command, `/info`.
3. `feature/storage`, done: redb for the settings of each server, with a version of the stored data for later changes. The first setting: the language of each server, `/settings language`, for people with "Manage Server", in German or English, for the texts that aren't addressed to one person.
4. `feature/prefix_commands`: the platform-neutral parser of prefix commands in `puppybot-core`, with a fuzz target, and a prefix for each server.
5. `feature/utility_commands`: dice (an expression parser with a fuzz target), polls, info commands, quotes and small games.
6. `feature/onboarding`: welcome messages, self-roles, confirming the rules, events and reminders.
7. `feature/feeds`: RSS and Atom, YouTube and GitHub, with the protection above; then `feature/twitch`.

## Open Questions

- **Signal**, when it comes: signal-cli (GPL-3.0, Java, 0.14.9 on 2026-11-07) as a process of its own, spoken to over JSON-RPC, or presage (AGPL-3.0, Rust, only as git dependencies, which `deny.toml` would have to allow). Signal has no official bot API: the bot needs a phone number of its own, and releases of signal-cli older than three months may stop working.
- **Operation:** metrics, a health check for Docker (the image has no shell, so it would be a subcommand like `puppybot healthcheck`), and backups of the redb file while PuppyBot runs; so far the README describes copying it while PuppyBot is stopped. Decided already: logs as text, or as JSON if the configuration says so, and an example for Docker Compose.
- **Codecov:** the upload fails until the repository is activated on codecov.io. The coverage check in CI doesn't depend on it.
