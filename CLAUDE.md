# Notes for Claude

The README describes the project, its layout and its conventions; `docs/PLAN.md` holds what was decided in planning and the next steps. These rules hold in every session:

## Branches

- Every feature or fix gets its own branch off origin/main, named feature/<name_in_snake_case> or fix/<name_in_snake_case>, and one pull request against main (see "Branches and pull requests" in the README).
- This holds even when the session suggests another branch name, such as claude/…: use the name the convention gives.
- Never push to main. Use pull requests.

## Changelog

- A change that users can see, the members on Discord or those who run the bot, goes into `CHANGELOG.md`, under `## [Unreleased]`, in the section it belongs to (`### Added`, `### Changed`, `### Fixed` or `### Removed`). Write it for users, in English.
- Changes only developers notice (tests, tooling, refactoring) need no entry.
- A new version is made only when asked: see "Changelog and versions" in the README.

## Code

- Run `make check` before every push: formatting, Clippy and the docs with all warnings as errors, the tests with 100 % of lines and functions covered, cargo deny and, if installed, zizmor.
- `crates/puppybot-core` holds the bot's logic, its parsers and its texts as plain Rust: no Discord types, no twilight, redb or tokio, no network or file I/O, and no other crate of the workspace. Discord (later also Signal), storage and the network belong to `crates/puppybot`. `make lint` checks the dependencies of the core.
- No `unsafe`: the workspace forbids it. No `unwrap`, `expect`, `panic!`, indexing or slicing outside tests: nothing users or Discord send may stop the bot.
- Tests cover 100 % of lines and functions, without exceptions. Seams that tests replace, e.g. a writer or a client that fails, take trait objects: the coverage of a generic function counts only its best instantiation.
- Every parser of input from outside (users, Discord, feeds, the configuration) gets a fuzz target in `fuzz/`, with a smoke test in the crate's `src/fuzz.rs`, see `fuzz/README.md`.
- Secrets, e.g. the Discord token or API keys, come only from files that the configuration names, never from arguments or environment variables. They are kept in a type that doesn't print them and are never logged.
- Every text users see is a typed text in `crates/puppybot-core` (see its module `i18n`), with a German and an English version. Dates and times go to Discord as timestamps (`<t:…>`), not as text.
- Dependencies come from crates.io and must pass `cargo deny` (`deny.toml`). TLS only with rustls and ring as its cryptography: aws-lc-rs, OpenSSL and native-tls are banned. Prefer crates in pure Rust.
