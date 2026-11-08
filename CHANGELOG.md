# Changelog

All changes users of PuppyBot can notice are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- PuppyBot connects to Discord, and connects again by itself when the connection breaks. It works only on the servers whose IDs `guilds` in the configuration lists: it registers its commands there, and leaves any other server, with a warning in its log. How to set the bot up on Discord is in the [README](README.md#setting-up-the-bot-on-discord).
- The slash command `/info`, which shows what PuppyBot is, which version runs, and where its code is.
- PuppyBot's answers are shown only to the person who asked, and mention nobody. They are in German for people whose Discord is in German, and in English for everyone else.
- A configuration file in TOML, `/etc/puppybot/config.toml` or the file that `--config` names: it says where the token of the bot is, on which servers it works, how detailed the log is, and whether it is text or JSON. A mistake in it names its line, see [Configuration](README.md#configuration).
- The token of the bot comes from a file, e.g. a Docker secret, and is never logged. PuppyBot warns if other users may read the file.
- PuppyBot runs until `SIGTERM` or `SIGINT` stops it, and then closes the connection to Discord and exits with the status 0; with 1 if it can't run, e.g. as Discord rejected the token, and with 2 for a wrong command line or configuration, see [Running and Stopping](README.md#running-and-stopping).
- An example for Docker Compose in [docker/example](docker/example): the token as a secret, and the container read-only and without privileges.
