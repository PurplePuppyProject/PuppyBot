# PuppyBot

A Discord bot of the Purple Puppy Project, written in Rust; a Signal bot may follow.

**Status: being built.** So far the program reads its [configuration](#configuration) and the token of the bot, logs, and runs until a signal [stops](#running-and-stopping) it; it doesn't connect to Discord yet. What the bot will do, with which libraries, and the next steps are in [docs/PLAN.md](docs/PLAN.md).

The project holds itself to:

- **100 % test coverage:** CI fails if the tests leave a single line or function uncovered.
- **Fuzzing:** every parser of input from outside has a fuzz target, run on each pull request and longer once a week.
- **Conservative security:** no `unsafe`, strict Clippy lints, checked dependencies, pinned GitHub Actions, a Docker image without shell or root, and attestations of where binaries and image were built.

## Docker

The bot is deployed as a Docker image, for amd64 and arm64:

- `ghcr.io/purplepuppyproject/puppybot`
  - `latest`: the newest [release](https://github.com/PurplePuppyProject/PuppyBot/releases) that isn't a pre-release. Until there is one, the latest commit on `main`.
  - `<version>`, e.g. `0.1.0`: a release, also a pre-release.
  - `edge`: the latest commit on `main`; `sha-<commit>`: an earlier commit.

The image is based on [distroless static](https://github.com/GoogleContainerTools/distroless): the static binary, CA certificates and time zone data, but no shell, no package manager and no C library. It runs as the user `65532` without privileges, and needs no writable file system and no capabilities:

```sh
docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges \
  ghcr.io/purplepuppyproject/puppybot:edge --version
```

### Docker Compose

[docker/example](docker/example) runs the bot with [Docker Compose](https://docs.docker.com/compose/): `compose.yaml`, and the [configuration](#configuration) `config.toml`, which names the token file. In that directory:

1. Put the token of the bot into the file `discord_token`, readable only by the user of the image. `read -s` keeps the token out of the shell's history and off the screen:
   ```sh
   (umask 077 && read -rsp 'Discord token: ' token && printf '%s\n' "$token" > discord_token)
   sudo chown 65532:65532 discord_token
   ```
2. Start the bot with `docker compose up -d`. `docker compose logs` shows its log, `docker compose stop` stops it.

The container runs read-only, without capabilities and without gaining privileges. Compose gives it the token as the secret `/run/secrets/discord_token`, a file, which unlike an environment variable doesn't show in `docker inspect`. CI runs the example with each change, see `docker/test-example.sh`.

### Checking the Image

To check that an image was built by this repository's workflow:

```sh
gh attestation verify oci://ghcr.io/purplepuppyproject/puppybot:edge --repo PurplePuppyProject/PuppyBot
```

The binary carries the list of the crates it was built from ([cargo-auditable](https://github.com/rust-secure-code/cargo-auditable)), and the image an SBOM that lists them, so scanners find vulnerable crates in it, e.g. `trivy image ghcr.io/purplepuppyproject/puppybot:edge`. To see the SBOM:

```sh
docker buildx imagetools inspect ghcr.io/purplepuppyproject/puppybot:edge --format '{{ json .SBOM }}'
```

## Configuration

PuppyBot reads its settings from a [TOML](https://toml.io) file: `/etc/puppybot/config.toml`, where the Docker image expects it, or the file that `--config` names, e.g. `puppybot --config bot.toml`.
[docker/example/config.toml](docker/example/config.toml) has every setting:

| Key | Default | What it sets |
|---|---|---|
| `[discord]` `token-file` | none, it must be given | The file with the token of the bot, e.g. a Docker secret |
| `[log]` `level` | `info` | The most detailed level that is logged: `error`, `warn`, `info`, `debug` or `trace` |
| `[log]` `format` | `text` | `text`, a line per message, or `json`, a JSON object per line, for log collectors |

- **Checked:** unknown keys, e.g. typos, and invalid values are errors: PuppyBot names the line and exits with the status 2.
- **Paths:** a relative path, e.g. `token-file = "token"`, is relative to the directory of the configuration file.
- **Secrets** come only from files that the configuration names, never from the configuration itself, the command line or environment variables, and are never logged. The token file holds the token, with line breaks at its end or without; a character that isn't printable ASCII, e.g. a space, or a file larger than 4 KiB is an error. PuppyBot warns if users other than its owner may read or change the file.
- **`RUST_LOG`**, e.g. `RUST_LOG=debug` or `RUST_LOG=info,puppybot=trace`, takes precedence over `[log]` `level`, to find a bug.

## Running and Stopping

PuppyBot runs until `SIGTERM`, which `docker stop` and service managers send, or `SIGINT`, from Ctrl+C, stops it. It logs to stderr, which Docker collects. Its exit status is:

- 0 when a signal stopped it, or after `--version` or `--help`;
- 1 if it can't run, e.g. as the token file is missing or holds no valid token;
- 2 for a command line, a configuration file or a `RUST_LOG` that PuppyBot can't use.

## Project Layout

| Path | What it holds |
|---|---|
| `crates/puppybot-core` | The bot's logic, its parsers and its texts in German and English, without Discord, storage or I/O |
| `crates/puppybot` | The program: its command line, configuration, secrets, logging and signals; later Discord, storage and feeds |
| `fuzz/` | The fuzz targets, a workspace of its own, see [fuzz/README.md](fuzz/README.md) |
| `docker/` | The Dockerfile, the script that prepares its build context, the check of its SBOMs, and the example of Docker Compose with its test |
| `utils/` | `set-version.sh`, which prepares a release |
| `docs/PLAN.md` | What was decided in planning, and the next steps |

## Development

### Getting Started

Install Rust with [rustup](https://rustup.rs). The bot builds with the current stable Rust, and with Rust 1.85 or newer (`rust-version` in `Cargo.toml`, checked by CI).
The checks need some tools:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov cargo-deny
pipx install zizmor   # optional: make check uses it if it is installed
```

The `Makefile` holds the commands that CI runs, e.g. `make test`, `make lint` and `make run ARGS=--help`, with a list of all its targets at the top.
Before pushing, run everything a pull request must pass:

```sh
make check
```

It checks the formatting (`cargo fmt`), runs Clippy and builds the docs with all warnings as errors, checks that `puppybot-core` depends on no crate with I/O, runs the tests with the coverage check, checks the dependencies, and audits the workflows if zizmor is installed.

### Tests and Coverage

`make coverage` runs the tests with [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) and fails if they cover less than 100 % of the lines or of the functions (`COVERAGE_MIN` in the `Makefile`).
It prints a summary per file, writes an HTML report with the covered lines to `target/llvm-cov/html` and an LCOV report to `target/llvm-cov/lcov.info`.

- The program in `crates/puppybot/src/main.rs` only hands over to its library, and `crates/puppybot/tests/cli.rs` starts it, so its coverage counts too.
- The coverage of a generic function is that of its best instantiation, not of all of them together. Functions that the tests need to call with other types, e.g. writers that fail, take trait objects (`&mut dyn Write`) instead.

### Fuzzing and Dependency Checks

The fuzz targets in `fuzz/` feed random input to the parsers.
They need a nightly toolchain and [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz); `make fuzz` runs each of them for a minute, see [fuzz/README.md](fuzz/README.md).
`make test` runs a short smoke test of the same functions on stable Rust.

[cargo-deny](https://github.com/EmbarkStudios/cargo-deny) checks the dependencies for known vulnerabilities, unmaintained crates, licenses that don't go with the GPL, and sources other than crates.io, as configured in `deny.toml`:

```sh
make deny
```

### CI

| Workflow | Runs on | Checks |
|---|---|---|
| `lint.yml` | pull requests, `main` | rustfmt, Clippy and rustdoc, with a pinned Rust version, so new lints don't break unrelated changes |
| `rust-build.yml` | pull requests, `main` | the debug build, and `cargo check` with the oldest supported Rust (MSRV) |
| `rust-test.yml` | pull requests, `main` | the tests |
| `coverage.yml` | pull requests, `main` | 100 % of lines and functions; the summary is on the run's page, the reports are an artifact and go to Codecov |
| `fuzz.yml` | pull requests (30 s per target), weekly (15 min), by hand | the fuzz targets |
| `deny.yml`, `audit.yml` | pull requests, `main`, weekly | the dependencies, with cargo-deny and cargo-audit |
| `codeql.yml` | pull requests, `main`, weekly | CodeQL's analysis of the Rust code and of the workflows |
| `zizmor.yml` | pull requests, `main`, weekly | the workflows, with zizmor: e.g. template injection, permissions, known vulnerable actions |
| `release.yml` | pull requests, `main`, version tags | the static binaries, tested on amd64 and arm64, with the list of their crates checked by `cargo audit bin`, and the Docker image with its SBOMs; publishes them for `main` and version tags, see [Releasing](#releasing) |

Dependabot proposes updates of the crates, the base image and the GitHub Actions once a week, each kind in one pull request, and only for versions at least a week old.
All GitHub Actions are pinned to the SHA of their commit, and no workflow gets more permissions than it needs.

Settings of the repository that the workflows rely on, made once by an admin:

- **Rulesets** (Settings → Rules → Rulesets): a ruleset for `main` that requires a pull request and the checks above.
- **Private vulnerability reporting** (Settings → Code security), so [SECURITY.md](SECURITY.md) works.
- **The package's visibility:** after the first image was published, make the package `puppybot` public in its settings, so the image can be pulled without logging in.
- **Codecov:** the [Codecov app](https://github.com/apps/codecov) for the organization, and the repository activated on codecov.io; until then, only the upload fails, not the check.
- **Release immutability** (Settings → General → Releases) may be on: releases are published only once they have all files.

## Branches and pull requests

- Every feature or fix gets its own branch off `origin/main`, named `feature/<name_in_snake_case>` or `fix/<name_in_snake_case>`, for example `feature/slash_commands` or `fix/reconnect_loop`, and one pull request against `main`. One feature or fix a branch keeps a pull request easy to review and to take back.
- Nobody pushes to `main`: changes reach it by pull request only.
- A pull request starts from `.github/pull_request_template.md`: what changes and why, how to see it, where it was tried, and a checklist of the project's rules (the checks with full coverage, fuzzing, tests for fixes and features, the changelog, texts in German and English, `puppybot-core` free of I/O, no secrets, no `unsafe`, the README).
- CI runs on every pull request (see above); it must pass before the pull request is merged.

## Changelog and versions

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); changes users can notice go under `## [Unreleased]`, in English. Changes only developers notice, e.g. of tests or tooling, need no entry.
A version is made only when asked, as described below. Versions follow [Semantic Versioning](https://semver.org/).

### Releasing

1. In a pull request, run `make version VERSION=0.1.0`, with the new version. It sets the version in `Cargo.toml`, updates `Cargo.lock` and `fuzz/Cargo.lock`, and turns the section `[Unreleased]` of [CHANGELOG.md](CHANGELOG.md) into one for the version, with today's date. Describe the version at the start of its section.
2. Once it is merged, tag the merge commit with `v` and the version, and push the tag:
   ```sh
   git tag --annotate v0.1.0 --message "PuppyBot 0.1.0" <commit>
   git push origin v0.1.0
   ```
3. The workflow `release.yml` builds and tests the binaries for the tag, checks that it matches the version in `Cargo.toml` and is on `main`, and publishes the release: the binaries with their checksums, the version's section of the changelog as notes, and the Docker image tagged with the version, and with `latest` if it is the newest release. A version with a suffix, e.g. `-rc.1`, becomes a pre-release, which `latest` doesn't point to.

If publishing fails, run the workflow again: it completes the draft of the release it left. It doesn't change a published release.

## License

[GPL-3.0-only](LICENSE)
