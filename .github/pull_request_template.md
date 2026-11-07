## What and why

<!-- What does this change, and why? Link an issue it closes, as "Closes #123". -->

## How to see it

<!-- Where to look: the command, event or setting on Discord (or Signal), the configuration, the
Docker image. For anything users see, a screenshot of what the bot answers, in German and English. -->

## Tried on

<!-- Where it ran: a test server on Discord, Signal, locally with `make run` or in the Docker image.
For crashes or wrong answers, attach the log, without tokens or other secrets. -->

## Checklist

- [ ] `make check` passes (rustfmt, Clippy, docs, the dependencies of `puppybot-core`, the tests with 100 % of lines and functions covered, cargo deny, zizmor), as CI runs it
- [ ] `make fuzz` passes where the change touches input from users, Discord, feeds or the configuration; a new parser has a fuzz target
- [ ] A fixed bug has a test that would catch it again; a new feature has tests that cover all of it
- [ ] Changes users can notice are in `CHANGELOG.md` under `## [Unreleased]`, in the section they belong to (Added, Changed, Fixed or Removed), written for users, in English
- [ ] Every text users see is a typed text in `puppybot-core`, in German and English; dates and times go out as Discord timestamps
- [ ] `puppybot-core` stays free of Discord, storage and I/O
- [ ] No secrets in the code, tests, examples or logs: they come only from files and are never logged
- [ ] No `unsafe`; a new dependency is needed, maintained and passes `cargo deny`
- [ ] The README and `docs/PLAN.md` still say how things work
