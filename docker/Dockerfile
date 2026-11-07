# The image of PuppyBot for Linux on amd64 and arm64, with the static binaries that the release
# workflow builds and tests. The build context holds them by platform, see build-context.sh,
# which also shows how to build the image. The release workflow publishes it, see
# .github/workflows/release.yml.
#
# There is no RUN, so building the image for other platforms needs no emulation.

# distroless static: CA certificates, time zone data and a user without privileges, but no shell,
# no package manager and no C library, which the static binary doesn't need. The digest pins the
# image of the tag nonroot; Dependabot keeps it up to date.
FROM gcr.io/distroless/static-debian13:nonroot@sha256:25ac8f0c3b2334a9eb05452266bd4697864a8c8ef0ef71e5e6480d7f92868329
ARG TARGETPLATFORM
LABEL org.opencontainers.image.title="puppybot" \
      org.opencontainers.image.description="PuppyBot, a Discord bot of the Purple Puppy Project." \
      org.opencontainers.image.source="https://github.com/PurplePuppyProject/PuppyBot" \
      org.opencontainers.image.url="https://github.com/PurplePuppyProject/PuppyBot#docker" \
      org.opencontainers.image.licenses="GPL-3.0-only"
# Owned by root and not writable, so the bot can't change its own program.
COPY --chmod=0555 ${TARGETPLATFORM}/puppybot /usr/local/bin/puppybot
# The user nonroot of distroless, by number, so a runtime can check that it isn't root.
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/puppybot"]
