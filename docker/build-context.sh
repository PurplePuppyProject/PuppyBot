#!/usr/bin/env bash
# Prepares the build context for docker/Dockerfile: the binaries from the release archives,
# puppybot-linux-<arch>.tar.gz, by platform, e.g. linux/arm64/puppybot from
# puppybot-linux-arm64.tar.gz, and the empty directory data for the database.
#
# Usage: docker/build-context.sh <directory with the archives> <context directory>
#
# Then:
#
#   docker buildx build --file docker/Dockerfile --platform linux/amd64,linux/arm64 \
#     <context directory>
#
# The archives come from a release or from a run of the release workflow.
set -euo pipefail

archives=$1
context=$2
extracted=$(mktemp -d)
trap 'rm -rf "$extracted"' EXIT

for arch in amd64 arm64; do
    name=puppybot-linux-$arch
    tar --extract --gzip --file "$archives/$name.tar.gz" --directory "$extracted"
    mkdir -p "$context/linux/$arch"
    cp "$extracted/$name/puppybot" "$context/linux/$arch/"
done
mkdir -p "$context/data"
