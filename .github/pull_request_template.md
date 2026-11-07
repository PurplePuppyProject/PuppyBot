## What and why

<!-- What does this change, and why? Link an issue it closes, as "Closes #123". -->

## How to see it

<!-- Where to look: the command, event or setting on Discord (or Signal), the configuration, the
Docker image. For anything users see, a screenshot of what the bot answers. -->

## Tried on

<!-- Where it ran: a test server on Discord, Signal, locally with `make run` or in the Docker image.
For crashes or wrong answers, attach the log, without tokens or other secrets. -->

## Checklist

- [ ] `make check` passes (rustfmt, Clippy, docs, the tests with 100 % line coverage, cargo deny), as CI runs it
- [ ] `make fuzz` passes where the change touches input from users, Discord or the configuration; a new parser has a fuzz target
- [ ] A fixed bug has a test that would catch it again; a new feature has tests that cover all of it
- [ ] Changes users can notice are in `CHANGELOG.md` under `## [Unreleased]`, in the section they belong to (Added, Changed, Fixed or Removed), written for users, in English
- [ ] No secrets in the code, tests, examples or logs: tokens come from the configuration and are never logged
- [ ] No `unsafe`; a new dependency is needed, maintained and passes `cargo deny`
- [ ] The README still says how things work
