#!/usr/bin/env bash
# Runs the example in docker/example with a token for the test, and checks that PuppyBot starts,
# never logs the token, and stops on SIGTERM with the exit status 0. Uses the image by the name in
# compose.yaml, so tag the image to test like that, as the release workflow does. Needs Docker
# Compose, and sudo to give the token file to the user of the image, as the README describes.
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
    if [ "$status" -ne 0 ]; then
        "${compose[@]}" logs || true
    fi
    "${compose[@]}" down --volumes || true
    sudo rm -f "$token_file"
    exit "$status"
}
trap cleanup EXIT

# Waits until the log contains the text.
wait_for_log() {
    for _ in $(seq 60); do
        if "${compose[@]}" logs 2>&1 | grep -qF "$1"; then
            return 0
        fi
        sleep 1
    done
    echo "No \"$1\" in the log" >&2
    return 1
}

# As the README describes it: readable only by the user of the image.
(umask 077 && printf '%s\n' "$token" > "$token_file")
sudo chown 65532:65532 "$token_file"

"${compose[@]}" up --detach
wait_for_log "PuppyBot started"
# Sends SIGTERM and waits until the program exits.
"${compose[@]}" stop
container=$("${compose[@]}" ps --all --quiet puppybot)
exit_status=$(docker inspect --format '{{.State.ExitCode}}' "$container")
log=$("${compose[@]}" logs 2>&1)

if [ "$exit_status" != 0 ]; then
    echo "PuppyBot exited with $exit_status, not 0" >&2
    exit 1
fi
if ! grep -qF 'PuppyBot stopped signal="SIGTERM"' <<< "$log"; then
    echo "PuppyBot didn't log that SIGTERM stopped it" >&2
    exit 1
fi
if grep -qF "$token" <<< "$log"; then
    echo "The log shows the token" >&2
    exit 1
fi
if grep -q WARN <<< "$log"; then
    echo "PuppyBot warned" >&2
    exit 1
fi
echo "The example works."
