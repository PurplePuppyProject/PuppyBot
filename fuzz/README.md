# Fuzzing

The fuzz targets feed random input to the code that reads what PuppyBot gets from outside: so far the command line of the program, the configuration file and the files with secrets.
Every parser of input from users, Discord or the configuration gets a target of its own.
They run with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) and libFuzzer.

| Target | Input | Checks beyond not crashing |
|---|---|---|
| `cli_args` | The command line, as arguments separated by NUL bytes | The command line of what the parser understood is understood the same, and the program prints the version or the help with the exit status 0, or an error with 2. It doesn't run the bot, which would read the files that the input names. |
| `config` | The configuration file, as its text | An accepted configuration names a token file, at an absolute path, as the directory of the configuration file is. |
| `token_file` | What a file with a secret holds | The secret is printable ASCII and the start of the input, and only line breaks follow it. |

## Running

The fuzz targets need a nightly toolchain and cargo-fuzz:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

Run each target for a minute, or for longer with `FUZZ_SECONDS`:

```sh
make fuzz
make fuzz FUZZ_SECONDS=600
```

Or run a single target until you stop it:

```sh
cd fuzz
cargo +nightly fuzz run cli_args -- -dict=puppybot.dict
```

[`puppybot.dict`](puppybot.dict) holds tokens of the input, e.g. the options of the command line, which help libFuzzer find valid input.
The inputs it found go to `corpus/<target>`, and are used again in the next run.

## When a Target Fails

libFuzzer saves the input that made the target fail in `artifacts/<target>/`, and prints the command that runs the target with it again:

```sh
cargo +nightly fuzz run cli_args artifacts/cli_args/crash-<hash>
```

`cargo +nightly fuzz tmin cli_args artifacts/cli_args/crash-<hash>` shortens the input. Once the bug is fixed, add the input to the smoke test of the target in the crate's `src/fuzz.rs`, so it stays fixed.

## How It Works

- The functions the targets call are in the `src/fuzz.rs` of the crate whose parser they feed, so far [`crates/puppybot/src/fuzz.rs`](../crates/puppybot/src/fuzz.rs). It is compiled with `--cfg fuzzing`, which cargo fuzz sets, and for the tests.
- `cargo test` runs each of these functions on a few inputs, as a smoke test that needs neither nightly nor cargo-fuzz.
- The fuzz crate has a workspace and a `Cargo.lock` of its own, so the `Cargo.lock` of PuppyBot stays free of fuzzing dependencies.
- CI ([`.github/workflows/fuzz.yml`](../.github/workflows/fuzz.yml)) runs each target for 30 seconds on pull requests and for 15 minutes once a week. It can also be started by hand, with the time per target. If a target fails, the input is uploaded as the artifact `fuzz-artifacts`.

## Adding a Target

1. Add a function to the crate's `src/fuzz.rs` that feeds its input to the parser and checks what it can, with a smoke test. A crate without one yet gets the module like `crates/puppybot`: `#[cfg(all(unix, any(test, fuzzing)))] pub mod fuzz;`, and becomes a dependency in `Cargo.toml`.
2. Add `fuzz_targets/<target>.rs`, which calls it, like the others.
3. Add a `[[bin]]` section for it to `Cargo.toml`.
