#!/usr/bin/env bash
# Runs the example in docker/example with a token for the test, and checks that PuppyBot starts,
# connects to Discord, which rejects the token, and exits with the status 1 and an error that says
# so, without warnings and without logging the token. So it checks the image as it runs: the
# configuration, the secret, the database in its volume, which PuppyBot opens before it connects,
# and TLS with the CA certificates of the image. Needs the network, to reach Discord. Uses the
# image by the name in compose.yaml, so tag the image to test like that, as the release workflow
# does. Needs Docker Compose, and sudo to give the token file to the user of the image, as the
# README describes.
#
# Usage: docker/test-example.sh
set -euo pipefail

example=$(cd "$(dirname "$0")/example" && pwd)
compose=(docker compose --project-directory "$example" --project-name puppybot-test)
token_file=$example/discord_token
if [ -e "$token_file" ]; then
    echo "$token_file exists: this test would overwrite it." >&2
    exit 1
fi
token=test-$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')

cleanup() {
    status=$?
    "${compose[@]}" down --volumes || true
    sudo rm -f "$token_file"
    exit "$status"
}
trap cleanup EXIT

# As the README describes it: readable only by the user of the image.
(umask 077 && printf '%s\n' "$token" > "$token_file")
sudo chown 65532:65532 "$token_file"

# Once, without restarting it as compose.yaml says. Discord rejects the token within seconds.
if log=$(timeout 120 "${compose[@]}" run --rm --no-TTY puppybot 2>&1); then
    exit_status=0
else
    exit_status=$?
fi

if [ "$exit_status" = 124 ]; then
    echo "PuppyBot didn't stop within 2 minutes: can it reach Discord?" >&2
    printf '%s\n' "$log" >&2
    exit 1
fi
if grep -qF "$token" <<< "$log"; then
    echo "The log shows the token" >&2
    exit 1
fi
printf '%s\n' "$log"
if [ "$exit_status" != 1 ]; then
    echo "PuppyBot exited with $exit_status, not 1" >&2
    exit 1
fi
for expected in "PuppyBot started" "PuppyBot can't run: Discord closed the connection for good with the code 4004"; do
    if ! grep -qF "$expected" <<< "$log"; then
        echo "No \"$expected\" in the log" >&2
        exit 1
    fi
done
# PuppyBot's lines, not those of Compose, which go to the same output.
if grep -qE '^[^ ]+Z +WARN ' <<< "$log"; then
    echo "PuppyBot warned" >&2
    exit 1
fi
echo "The example works."
