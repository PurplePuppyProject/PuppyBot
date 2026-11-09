//! GitHub's repositories, commits and releases, as PuppyBot announces them: the names that it
//! takes, which commits and releases of a check are new, and the texts, in German and English.
//!
//! ```
//! use puppybot_core::github::Repo;
//!
//! let repo = Repo::parse("https://github.com/PurplePuppyProject/PuppyBot").unwrap();
//! assert_eq!(repo.to_string(), "PurplePuppyProject/PuppyBot");
//! ```

use std::fmt;

use crate::i18n::Lang;
use crate::text::{MAX_NAME, literal};

/// The most repositories that a server can have announced.
pub const MAX_REPOS: usize = 10;

/// The most commits in one message; the others are counted.
pub const MAX_COMMITS: usize = 10;

/// The most characters of a branch's name that PuppyBot takes.
pub const MAX_BRANCH: usize = 100;

/// The most characters of a commit's title in a message.
const MAX_TITLE: usize = 72;

/// The most characters of the name of a commit's author in a message.
const MAX_AUTHOR: usize = 40;

/// The most characters of a message on Discord.
const MAX_MESSAGE: usize = 2000;

/// `repo` as PuppyBot shows it: as it is, also with `_` in it, which Discord could take for
/// italics.
fn shown(repo: &str) -> String {
    literal(repo, 140)
}

/// A repository on GitHub: its owner and its name, e.g. `PurplePuppyProject/PuppyBot`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Repo {
    owner: String,
    name: String,
}

impl Repo {
    /// The repository in `text`, `owner/name`, also as the address of its page on GitHub. The
    /// owner has 1 to 39 letters, digits and hyphens, but no hyphen at its start or end, and
    /// the name 1 to 100 letters, digits, `.`, `-` and `_`, but isn't `.` or `..`, as GitHub
    /// allows them; a `.git` at the end is left out.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = ["https://github.com/", "http://github.com/", "github.com/"]
            .iter()
            .find_map(|start| text.strip_prefix(start))
            .unwrap_or(text);
        let text = text.strip_suffix('/').unwrap_or(text);
        let text = text.strip_suffix(".git").unwrap_or(text);
        let (owner, name) = text.split_once('/')?;
        let owner_ok = (1..=39).contains(&owner.len())
            && owner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !owner.starts_with('-')
            && !owner.ends_with('-');
        let name_ok = (1..=100).contains(&name.len())
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
            && name != "."
            && name != "..";
        (owner_ok && name_ok).then(|| Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    /// The owner, e.g. `PurplePuppyProject`.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The name, e.g. `PuppyBot`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// Whether PuppyBot takes `name` as the name of a branch: up to [`MAX_BRANCH`] letters and
/// digits of ASCII, `.`, `-`, `_` and `/`, but no `-` or `/` at its start, no `/` or `.` at its
/// end, and no `..` or `//`. Git allows more, but such names fit into an address as they are.
#[must_use]
pub fn is_valid_branch(name: &str) -> bool {
    (1..=MAX_BRANCH).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_/".contains(&byte))
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("//")
}

/// A commit, as a check of GitHub finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commit<'a> {
    /// Its hash, in hexadecimal digits.
    pub sha: &'a str,
    /// The first line of its message.
    pub title: &'a str,
    /// Who wrote it: their name on GitHub, or else in Git.
    pub author: &'a str,
}

/// What a check of a branch found, compared with the commit that PuppyBot knew last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommits<'a> {
    /// The new commits, the oldest first: the newest [`MAX_COMMITS`].
    pub commits: Vec<Commit<'a>>,
    /// How many more, older ones there are.
    pub more: usize,
    /// Whether the commit known last is gone, e.g. after a force push, or so far back that the
    /// check didn't reach it: then the commits are just the newest.
    pub rewritten: bool,
}

/// The commits of `newest`, the newest first as GitHub lists them, that came after `last`.
#[must_use]
pub fn new_commits<'a>(newest: &[Commit<'a>], last: &str) -> NewCommits<'a> {
    let known = newest.iter().position(|commit| commit.sha == last);
    let new = newest
        .get(..known.unwrap_or(newest.len()))
        .unwrap_or(newest);
    let mut commits: Vec<_> = new.iter().take(MAX_COMMITS).copied().collect();
    commits.reverse();
    NewCommits {
        commits,
        more: new.len().saturating_sub(MAX_COMMITS),
        rewritten: known.is_none(),
    }
}

/// A release, as a check of GitHub finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release<'a> {
    /// Its ID on GitHub, which grows with each release.
    pub id: u64,
    /// Its tag, e.g. `v1.2.0`.
    pub tag: &'a str,
    /// Its name, which may be empty.
    pub name: &'a str,
}

/// The releases of `newest` that came after the one with the ID `last`, the oldest first.
#[must_use]
pub fn new_releases<'a>(newest: &[Release<'a>], last: u64) -> Vec<Release<'a>> {
    let mut releases: Vec<_> = newest
        .iter()
        .filter(|release| release.id > last)
        .copied()
        .collect();
    releases.sort_by_key(|release| release.id);
    releases
}

/// A repository that a server has announced, as /github list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watch<'a> {
    /// The repository, `owner/name`.
    pub repo: &'a str,
    /// The branch.
    pub branch: &'a str,
    /// The channel, as a mention of it.
    pub channel: &'a str,
    /// Whether its releases are announced, too.
    pub releases: bool,
}

/// A text of the announcements of GitHub that users see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubText<'a> {
    /// The description of /github.
    GithubDescription,
    /// The description of /github add.
    AddDescription,
    /// The description of /github remove.
    RemoveDescription,
    /// The description of /github list.
    ListDescription,
    /// The description of the option `repo`.
    RepoDescription,
    /// The description of the option `channel`.
    ChannelDescription,
    /// The description of the option `branch`.
    BranchDescription,
    /// The description of the option `releases`.
    ReleasesDescription,
    /// The answer to /github add.
    Added {
        /// The repository, `owner/name`.
        repo: &'a str,
        /// The branch.
        branch: &'a str,
        /// The channel, as a mention of it.
        channel: &'a str,
        /// Whether its releases are announced, too.
        releases: bool,
    },
    /// The answer to /github remove.
    Removed {
        /// The repository, `owner/name`.
        repo: &'a str,
    },
    /// The answer to /github remove of a repository that the server doesn't announce.
    NotWatched {
        /// The repository, `owner/name`.
        repo: &'a str,
    },
    /// The answer to /github list.
    Watched(&'a [Watch<'a>]),
    /// The answer to a repository that isn't `owner/name`.
    InvalidRepo,
    /// The answer to a branch that PuppyBot doesn't take, see [`is_valid_branch`].
    InvalidBranch,
    /// The answer to a repository that GitHub doesn't have, or that isn't public.
    NoSuchRepo {
        /// The repository, `owner/name`.
        repo: &'a str,
    },
    /// The answer to a branch that the repository doesn't have.
    NoSuchBranch {
        /// The repository, `owner/name`.
        repo: &'a str,
        /// The branch.
        branch: &'a str,
    },
    /// The answer to /github add on a server that announces [`MAX_REPOS`] repositories.
    Full,
    /// The answer when PuppyBot can't reach GitHub.
    GithubFailed,
    /// New commits of a branch.
    Pushed {
        /// The repository, `owner/name`, as [`Repo::parse`] takes it.
        repo: &'a str,
        /// The branch, as [`is_valid_branch`] takes it.
        branch: &'a str,
        /// The commits, the oldest first.
        commits: &'a [Commit<'a>],
        /// How many more there are.
        more: usize,
        /// Whether the commit known last is gone.
        rewritten: bool,
    },
    /// A new release.
    Released {
        /// The repository, `owner/name`, as [`Repo::parse`] takes it.
        repo: &'a str,
        /// The release.
        release: Release<'a>,
    },
}

/// The commit of the examples.
const COMMIT: Commit<'static> = Commit {
    sha: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678",
    title: "Roll dice with /dice",
    author: "Puppy",
};

/// The repository of the examples, as /github list shows it.
const WATCH: Watch<'static> = Watch {
    repo: "PurplePuppyProject/PuppyBot",
    branch: "main",
    channel: "<#4>",
    releases: true,
};

impl GithubText<'_> {
    /// All texts, with an example for each value they hold.
    pub const ALL: [GithubText<'static>; 26] = [
        GithubText::GithubDescription,
        GithubText::AddDescription,
        GithubText::RemoveDescription,
        GithubText::ListDescription,
        GithubText::RepoDescription,
        GithubText::ChannelDescription,
        GithubText::BranchDescription,
        GithubText::ReleasesDescription,
        GithubText::Added {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "main",
            channel: "<#4>",
            releases: true,
        },
        GithubText::Added {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "main",
            channel: "<#4>",
            releases: false,
        },
        GithubText::Removed {
            repo: "PurplePuppyProject/PuppyBot",
        },
        GithubText::NotWatched {
            repo: "PurplePuppyProject/PuppyBot",
        },
        GithubText::Watched(&[]),
        GithubText::Watched(&[WATCH]),
        GithubText::Watched(&[Watch {
            releases: false,
            ..WATCH
        }]),
        GithubText::InvalidRepo,
        GithubText::InvalidBranch,
        GithubText::NoSuchRepo {
            repo: "PurplePuppyProject/PuppyBot",
        },
        GithubText::NoSuchBranch {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "next",
        },
        GithubText::Full,
        GithubText::GithubFailed,
        GithubText::Pushed {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "main",
            commits: &[COMMIT],
            more: 0,
            rewritten: false,
        },
        GithubText::Pushed {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "main",
            commits: &[COMMIT, COMMIT],
            more: 0,
            rewritten: false,
        },
        GithubText::Pushed {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "main",
            commits: &[COMMIT],
            more: 3,
            rewritten: true,
        },
        GithubText::Released {
            repo: "PurplePuppyProject/PuppyBot",
            release: Release {
                id: 1,
                tag: "v1.2.0",
                name: "Big update",
            },
        },
        GithubText::Released {
            repo: "PurplePuppyProject/PuppyBot",
            release: Release {
                id: 1,
                tag: "v1.2.0",
                name: "",
            },
        },
    ];

    /// The descriptions in the list of commands, which Discord limits to 100 characters.
    pub const DESCRIPTIONS: [GithubText<'static>; 8] = [
        GithubText::GithubDescription,
        GithubText::AddDescription,
        GithubText::RemoveDescription,
        GithubText::ListDescription,
        GithubText::RepoDescription,
        GithubText::ChannelDescription,
        GithubText::BranchDescription,
        GithubText::ReleasesDescription,
    ];

    /// The text in `lang`.
    #[must_use]
    pub fn render(&self, lang: Lang) -> String {
        match lang {
            Lang::De => self.de(),
            Lang::En => self.en(),
        }
    }

    fn de(&self) -> String {
        match *self {
            Self::GithubDescription => {
                "Kündigt neue Commits und Releases von GitHub-Repositorys an".to_owned()
            }
            Self::AddDescription => "Kündigt ein Repository in einem Kanal an".to_owned(),
            Self::RemoveDescription => "Kündigt ein Repository nicht mehr an".to_owned(),
            Self::ListDescription => "Zeigt, welche Repositorys dieser Server ankündigt".to_owned(),
            Self::RepoDescription => "Das Repository, z. B. owner/name".to_owned(),
            Self::ChannelDescription => "Der Kanal der Ankündigungen".to_owned(),
            Self::BranchDescription => "Der Branch; ohne ihn der Standard-Branch".to_owned(),
            Self::ReleasesDescription => "Ob auch seine Releases angekündigt werden".to_owned(),
            Self::Added {
                repo,
                branch,
                channel,
                releases,
            } => {
                let releases = if releases { " und Releases" } else { "" };
                format!(
                    "PuppyBot kündigt ab jetzt neue Commits auf `{branch}`{releases} von **{}** in {channel} an.",
                    shown(repo)
                )
            }
            Self::Removed { repo } => {
                format!("PuppyBot kündigt **{}** nicht mehr an.", shown(repo))
            }
            Self::NotWatched { repo } => {
                format!("Dieser Server lässt **{}** nicht ankündigen.", shown(repo))
            }
            Self::InvalidRepo => {
                "Ein Repository heißt `owner/name`, z. B. `PurplePuppyProject/PuppyBot`.".to_owned()
            }
            Self::InvalidBranch => format!(
                "Einen solchen Branch nimmt PuppyBot nicht: bis zu {MAX_BRANCH} Buchstaben, Ziffern und `.-_/`."
            ),
            Self::NoSuchRepo { repo } => {
                format!(
                    "PuppyBot findet auf GitHub kein öffentliches Repository **{}**.",
                    shown(repo)
                )
            }
            Self::NoSuchBranch { repo, branch } => {
                format!("**{}** hat keinen Branch `{branch}`.", shown(repo))
            }
            Self::Full => format!(
                "Dieser Server lässt schon {MAX_REPOS} Repositorys ankündigen, mehr nicht. Bitte erst eins entfernen."
            ),
            Self::GithubFailed => {
                "PuppyBot erreicht GitHub gerade nicht. Bitte später noch einmal versuchen."
                    .to_owned()
            }
            Self::Watched(watches) => watched(watches, Lang::De),
            Self::Pushed {
                repo,
                branch,
                commits,
                more,
                rewritten,
            } => pushed(repo, branch, commits, more, rewritten, Lang::De),
            Self::Released { repo, release } => released(repo, release, Lang::De),
        }
    }

    fn en(&self) -> String {
        match *self {
            Self::GithubDescription => {
                "Announces new commits and releases of GitHub repositories".to_owned()
            }
            Self::AddDescription => "Announces a repository in a channel".to_owned(),
            Self::RemoveDescription => "Stops announcing a repository".to_owned(),
            Self::ListDescription => "Shows which repositories this server announces".to_owned(),
            Self::RepoDescription => "The repository, e.g. owner/name".to_owned(),
            Self::ChannelDescription => "The channel of the announcements".to_owned(),
            Self::BranchDescription => "The branch; without it, the default branch".to_owned(),
            Self::ReleasesDescription => "Whether its releases are announced, too".to_owned(),
            Self::Added {
                repo,
                branch,
                channel,
                releases,
            } => {
                let releases = if releases { " and releases" } else { "" };
                format!(
                    "From now on, PuppyBot announces new commits on `{branch}`{releases} of **{}** in {channel}.",
                    shown(repo)
                )
            }
            Self::Removed { repo } => format!("PuppyBot no longer announces **{}**.", shown(repo)),
            Self::NotWatched { repo } => {
                format!("This server doesn't have **{}** announced.", shown(repo))
            }
            Self::InvalidRepo => {
                "A repository is named `owner/name`, e.g. `PurplePuppyProject/PuppyBot`.".to_owned()
            }
            Self::InvalidBranch => format!(
                "PuppyBot doesn't take such a branch: up to {MAX_BRANCH} letters, digits and `.-_/`."
            ),
            Self::NoSuchRepo { repo } => {
                format!(
                    "PuppyBot finds no public repository **{}** on GitHub.",
                    shown(repo)
                )
            }
            Self::NoSuchBranch { repo, branch } => {
                format!("**{}** has no branch `{branch}`.", shown(repo))
            }
            Self::Full => format!(
                "This server has {MAX_REPOS} repositories announced, the most there can be. Please remove one first."
            ),
            Self::GithubFailed => {
                "PuppyBot can't reach GitHub right now. Please try again later.".to_owned()
            }
            Self::Watched(watches) => watched(watches, Lang::En),
            Self::Pushed {
                repo,
                branch,
                commits,
                more,
                rewritten,
            } => pushed(repo, branch, commits, more, rewritten, Lang::En),
            Self::Released { repo, release } => released(repo, release, Lang::En),
        }
    }
}

/// The answer to /github list, in `lang`.
fn watched(watches: &[Watch<'_>], lang: Lang) -> String {
    if watches.is_empty() {
        return match lang {
            Lang::De => "Dieser Server lässt noch keine Repositorys ankündigen. Das geht mit `/github hinzufügen`.",
            Lang::En => "This server has no repositories announced yet. `/github add` adds one.",
        }
        .to_owned();
    }
    let lines = watches.iter().map(|watch| {
        let Watch {
            repo,
            branch,
            channel,
            releases,
        } = *watch;
        let repo = shown(repo);
        match (lang, releases) {
            (Lang::De, true) => format!("- **{repo}**, `{branch}` und Releases, in {channel}"),
            (Lang::En, true) => format!("- **{repo}**, `{branch}` and releases, in {channel}"),
            // The same in both languages.
            (_, false) => format!("- **{repo}**, `{branch}`, in {channel}"),
        }
    });
    let mut message = match lang {
        Lang::De => "Dieser Server lässt ankündigen:",
        Lang::En => "This server has announced:",
    }
    .to_owned();
    // As many as fit into a message, with room for the end.
    for line in lines {
        if message.chars().count() + line.chars().count() > MAX_MESSAGE - 10 {
            message.push_str("\n- …");
            break;
        }
        message.push('\n');
        message.push_str(&line);
    }
    message
}

/// The message of new commits on `branch` of `repo`, in `lang`: as many of `commits` as fit
/// into a message of Discord, and how many more there are.
fn pushed(
    repo: &str,
    branch: &str,
    commits: &[Commit<'_>],
    more: usize,
    rewritten: bool,
    lang: Lang,
) -> String {
    let count = commits.len().saturating_add(more);
    let name = shown(repo);
    let head = match (lang, rewritten, count) {
        (Lang::De, true, _) => format!(
            "**`{branch}` von {name} hat sich geändert**, z. B. durch einen Force-Push. Die neuesten Commits:"
        ),
        (Lang::De, false, 1) => format!("**1 neuer Commit** in {name} (`{branch}`):"),
        (Lang::De, false, _) => format!("**{count} neue Commits** in {name} (`{branch}`):"),
        (Lang::En, true, _) => {
            format!("**`{branch}` of {name} changed**, e.g. by a force push. Its newest commits:")
        }
        (Lang::En, false, 1) => format!("**1 new commit** in {name} (`{branch}`):"),
        (Lang::En, false, _) => format!("**{count} new commits** in {name} (`{branch}`):"),
    };
    let history = format!("https://github.com/{repo}/commits/{branch}");
    let footer = |left: usize| match lang {
        Lang::De => format!("\n… und {left} mehr: [Verlauf](<{history}>)"),
        Lang::En => format!("\n… and {left} more: [history](<{history}>)"),
    };
    // Room for the footer with the most that it can count.
    let room = MAX_MESSAGE.saturating_sub(footer(count).chars().count());
    let mut message = head;
    let mut shown = 0;
    for commit in commits {
        let short = commit.sha.get(..7).unwrap_or(commit.sha);
        let line = format!(
            "\n- [`{short}`](<https://github.com/{repo}/commit/{}>) {}: {}",
            commit.sha,
            literal(commit.title, MAX_TITLE),
            literal(commit.author, MAX_AUTHOR)
        );
        if message.chars().count() + line.chars().count() > room {
            break;
        }
        message.push_str(&line);
        shown += 1;
    }
    let left = count - shown;
    if left > 0 || rewritten {
        message.push_str(&footer(left));
    }
    message
}

/// The message of a new `release` of `repo`, in `lang`.
fn released(repo: &str, release: Release<'_>, lang: Lang) -> String {
    let tag = literal(release.tag, MAX_NAME);
    // A tag too long for the link leads to the list of the releases.
    let page = if release.tag.len() <= MAX_NAME {
        format!("tag/{}", url_path(release.tag))
    } else {
        String::new()
    };
    let link = format!("[{tag}](<https://github.com/{repo}/releases/{page}>)");
    let name = literal(release.name, MAX_NAME);
    let name = name.trim();
    let name = if name.is_empty() {
        String::new()
    } else {
        format!(" {name}")
    };
    let repo = shown(repo);
    match lang {
        Lang::De => format!("**Neues Release** von {repo}: {link}{name}"),
        Lang::En => format!("**New release** of {repo}: {link}{name}"),
    }
}

/// `text` as a part of a path in an address: every byte but letters, digits and `.-_~`
/// percent-encoded.
fn url_path(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b".-_~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repositories() {
        for text in [
            "PurplePuppyProject/PuppyBot",
            " PurplePuppyProject/PuppyBot ",
            "https://github.com/PurplePuppyProject/PuppyBot",
            "http://github.com/PurplePuppyProject/PuppyBot/",
            "github.com/PurplePuppyProject/PuppyBot.git",
        ] {
            let repo = Repo::parse(text).unwrap();
            assert_eq!(
                (repo.owner(), repo.name()),
                ("PurplePuppyProject", "PuppyBot"),
                "{text:?}"
            );
            assert_eq!(repo.to_string(), "PurplePuppyProject/PuppyBot");
        }
        assert!(Repo::parse("a/b").is_some());
        assert!(Repo::parse(&format!("{}/{}", "a".repeat(39), "b".repeat(100))).is_some());
        assert!(Repo::parse("rust-lang/rust.vim_x-y").is_some());
        for text in [
            "",
            "PuppyBot",
            "/PuppyBot",
            "Purple/",
            "a/b/c",
            "-a/b",
            "a-/b",
            "a_b/c",
            "a/.",
            "a/..",
            "a/b c",
            "ä/b",
            "a/<@1>",
            &format!("{}/b", "a".repeat(40)),
            &format!("a/{}", "b".repeat(101)),
        ] {
            assert_eq!(Repo::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn branches() {
        for name in ["main", "release/1.2", "feature/dice_2", "v1.0-rc.1", "a"] {
            assert!(is_valid_branch(name), "{name:?}");
        }
        assert!(is_valid_branch(&"a".repeat(MAX_BRANCH)));
        for name in [
            "",
            "-main",
            "/main",
            "main/",
            "main.",
            "a..b",
            "a//b",
            "a b",
            "a?b",
            "ä",
            "a\u{0}",
            &"a".repeat(MAX_BRANCH + 1),
        ] {
            assert!(!is_valid_branch(name), "{name:?}");
        }
    }

    /// The commit with the hash `sha`.
    fn commit(sha: &str) -> Commit<'_> {
        Commit {
            sha,
            title: "Wuff",
            author: "Puppy",
        }
    }

    #[test]
    fn which_commits_are_new() {
        let shas = ["e", "d", "c", "b", "a"];
        let newest: Vec<_> = shas.iter().map(|sha| commit(sha)).collect();
        let found = new_commits(&newest, "c");
        let hashes: Vec<_> = found.commits.iter().map(|commit| commit.sha).collect();
        assert_eq!(
            (hashes, found.more, found.rewritten),
            (vec!["d", "e"], 0, false)
        );
        let found = new_commits(&newest, "e");
        assert_eq!((found.commits.len(), found.rewritten), (0, false));
        // Gone: the newest.
        let found = new_commits(&newest, "z");
        assert_eq!((found.commits.len(), found.rewritten), (5, true));
        assert_eq!(found.commits[0].sha, "a");
        // More than fit: the newest, and the count of the others.
        let shas: Vec<String> = (0..15).map(|n| format!("{n}")).collect();
        let newest: Vec<_> = shas.iter().map(|sha| commit(sha)).collect();
        let found = new_commits(&newest, "14");
        assert_eq!((found.commits.len(), found.more), (MAX_COMMITS, 4));
        assert_eq!((found.commits[0].sha, found.commits[9].sha), ("9", "0"));
    }

    #[test]
    fn which_releases_are_new() {
        let release = |id| Release {
            id,
            tag: "v1",
            name: "",
        };
        let newest = [release(30), release(20), release(10)];
        let ids: Vec<_> = new_releases(&newest, 10)
            .iter()
            .map(|release| release.id)
            .collect();
        assert_eq!(ids, [20, 30]);
        assert_eq!(new_releases(&newest, 30), Vec::new());
    }

    #[test]
    fn every_text_in_every_language() {
        for text in GithubText::ALL {
            let texts = Lang::ALL.map(|lang| text.render(lang));
            for rendered in &texts {
                assert!(!rendered.is_empty(), "{text:?}");
                for mention in ["@everyone", "@here", "<@"] {
                    assert!(!rendered.contains(mention), "{text:?}: {rendered}");
                }
            }
            assert_ne!(texts[0], texts[1], "{text:?} isn't translated");
        }
        for text in GithubText::DESCRIPTIONS {
            for lang in Lang::ALL {
                assert!(text.render(lang).chars().count() <= 100, "{text:?}");
            }
        }
    }

    #[test]
    fn announcements() {
        let pushed = GithubText::Pushed {
            repo: "PurplePuppyProject/PuppyBot",
            branch: "main",
            commits: &[
                COMMIT,
                Commit {
                    sha: "0f9e8d7",
                    title: "**Show** <@1> quotes",
                    author: "_Rex_",
                },
            ],
            more: 0,
            rewritten: false,
        };
        assert_eq!(
            pushed.render(Lang::En),
            concat!(
                "**2 new commits** in PurplePuppyProject/PuppyBot (`main`):\n",
                "- [`a1b2c3d`](<https://github.com/PurplePuppyProject/PuppyBot/commit/a1b2c3d4e5f60718293a4b5c6d7e8f9012345678>) Roll dice with /dice: Puppy\n",
                r"- [`0f9e8d7`](<https://github.com/PurplePuppyProject/PuppyBot/commit/0f9e8d7>) \*\*Show\*\* \<\@1\> quotes: \_Rex\_"
            )
        );
        let rewritten = GithubText::Pushed {
            repo: "a/b",
            branch: "main",
            commits: &[COMMIT],
            more: 2,
            rewritten: true,
        };
        assert_eq!(
            rewritten.render(Lang::De),
            concat!(
                "**`main` von a/b hat sich geändert**, z. B. durch einen Force-Push. Die neuesten Commits:\n",
                "- [`a1b2c3d`](<https://github.com/a/b/commit/a1b2c3d4e5f60718293a4b5c6d7e8f9012345678>) Roll dice with /dice: Puppy\n",
                "… und 2 mehr: [Verlauf](<https://github.com/a/b/commits/main>)"
            )
        );
        let release = GithubText::Released {
            repo: "a/b",
            release: Release {
                id: 1,
                tag: "v1.2.0+[x]",
                name: " **Big** update ",
            },
        };
        assert_eq!(
            release.render(Lang::En),
            r"**New release** of a/b: [v1.2.0+\[x\]](<https://github.com/a/b/releases/tag/v1.2.0%2B%5Bx%5D>) \*\*Big\*\* update"
        );
    }

    #[test]
    fn the_most_fits_into_a_message() {
        let repo = format!("{}/{}", "a".repeat(39), "_".repeat(100));
        let branch = "c".repeat(MAX_BRANCH);
        let watch = Watch {
            repo: &repo,
            branch: &branch,
            channel: "<#18446744073709551615>",
            releases: true,
        };
        let watches = [watch; MAX_REPOS];
        for lang in Lang::ALL {
            let list = GithubText::Watched(&watches).render(lang);
            assert!(list.chars().count() <= MAX_MESSAGE, "{list}");
            assert!(list.ends_with("\n- …"), "{list}");
        }
        let repo = format!("{}/{}", "a".repeat(39), "b".repeat(100));
        let branch = "c".repeat(MAX_BRANCH);
        let sha = "f".repeat(64);
        let title = "*".repeat(500);
        let author = "_".repeat(500);
        let commits = [Commit {
            sha: &sha,
            title: &title,
            author: &author,
        }; MAX_COMMITS];
        for rewritten in [false, true] {
            let pushed = GithubText::Pushed {
                repo: &repo,
                branch: &branch,
                commits: &commits,
                more: usize::MAX - MAX_COMMITS,
                rewritten,
            };
            for lang in Lang::ALL {
                let message = pushed.render(lang);
                assert!(message.chars().count() <= MAX_MESSAGE, "{message}");
                // Some of the commits, and the count of the others.
                assert!(message.contains("`fffffff`"), "{message}");
            }
        }
        let release = GithubText::Released {
            repo: &repo,
            release: Release {
                id: 1,
                tag: &title,
                name: &title,
            },
        };
        assert!(release.render(Lang::En).chars().count() <= MAX_MESSAGE);
    }
}
