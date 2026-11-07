# Changelog

All changes users of PuppyBot can notice are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- A configuration file in TOML, `/etc/puppybot/config.toml` or the file that `--config` names: it says where the token of the bot is, how detailed the log is, and whether it is text or JSON. A mistake in it names its line, see [Configuration](README.md#configuration).
- The token of the bot comes from a file, e.g. a Docker secret, and is never logged. PuppyBot warns if other users may read the file.
- PuppyBot runs until `SIGTERM` or `SIGINT` stops it, and then exits with the status 0; with 1 if it can't run, and with 2 for a wrong command line or configuration, see [Running and Stopping](README.md#running-and-stopping). It doesn't connect to Discord yet.
- An example for Docker Compose in [docker/example](docker/example): the token as a secret, and the container read-only and without privileges.
