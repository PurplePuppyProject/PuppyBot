# Security Policy

## Supported Versions

PuppyBot is not stable yet: there are no releases so far, only the latest code on `main` and its Docker image `ghcr.io/purplepuppyproject/puppybot:edge`.
Security fixes are made on `main` and come with the next release; there are no maintained release branches.
There are no guarantees, see the [license](LICENSE).

## Reporting a Vulnerability

Please **do not open a public issue** for security problems.
Report them privately via GitHub instead: on the repository page, go to the **Security** tab and click **Report a vulnerability**.
Include the version or commit, your setup and the steps to reproduce.

Never include a bot token or other secret in a report. If a token of PuppyBot may have leaked, reset it in the [Discord Developer Portal](https://discord.com/developers/applications) first: the old one stops working at once.

Bugs without security impact can be reported as normal issues.
