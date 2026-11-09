//! GitHub's REST API, as PuppyBot asks it about the repositories that servers announce: whether
//! one is public, and its newest commits and releases. With the token, if there is one; with the
//! `ETag`s of earlier answers, so that GitHub answers "not modified" when nothing changed; and with
//! a pause when GitHub says that its limit is reached.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use http_body_util::{BodyExt as _, Empty, Limited};
use hyper::body::Bytes;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Request, StatusCode};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use puppybot_core::github::{Commit, Release, Repo, is_valid_branch};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

/// GitHub's API.
pub const API: &str = "https://api.github.com";

/// How long PuppyBot waits for an answer of GitHub.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// How many commits a check asks for: more than fit into a message, so that the others can be
/// counted.
const COMMITS: usize = 30;

/// How many releases a check asks for.
const RELEASES: usize = 10;

/// The most bytes of an answer.
const MAX_BODY: usize = 1024 * 1024;

/// The longest pause that GitHub can ask for.
const MAX_PAUSE: Duration = Duration::from_secs(3600);

/// The version of GitHub's API that PuppyBot speaks.
const API_VERSION: &str = "2022-11-28";

/// Why GitHub didn't answer a request as PuppyBot wanted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// GitHub has no such thing, or doesn't show it, e.g. a branch that isn't there.
    NotFound,
    /// GitHub's limit is reached, or it asked PuppyBot to wait: until then, PuppyBot asks nothing.
    Limited,
    /// The request failed: e.g. no network, another status, or an answer that GitHub doesn't
    /// send.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("GitHub has no such thing"),
            Self::Limited => f.write_str("GitHub's limit of requests is reached"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

/// What GitHub says of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// Its name, as GitHub writes it.
    pub repo: Repo,
    /// Whether it is public.
    pub public: bool,
    /// Its default branch.
    pub default_branch: String,
}

/// A commit in an answer of GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedCommit {
    /// Its hash, 7 to 64 hexadecimal digits.
    pub sha: String,
    /// The first line of its message.
    pub title: String,
    /// Who wrote it: their name on GitHub, or else in Git.
    pub author: String,
}

impl OwnedCommit {
    /// The commit, as `puppybot-core` takes it.
    #[must_use]
    pub fn commit(&self) -> Commit<'_> {
        Commit {
            sha: &self.sha,
            title: &self.title,
            author: &self.author,
        }
    }
}

/// A release in an answer of GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedRelease {
    /// Its ID.
    pub id: u64,
    /// Its tag.
    pub tag: String,
    /// Its name, which may be empty.
    pub name: String,
}

impl OwnedRelease {
    /// The release, as `puppybot-core` takes it.
    #[must_use]
    pub fn release(&self) -> Release<'_> {
        Release {
            id: self.id,
            tag: &self.tag,
            name: &self.name,
        }
    }
}

#[derive(Deserialize)]
struct RepoJson {
    full_name: String,
    private: bool,
    default_branch: String,
}

#[derive(Deserialize)]
struct CommitJson {
    sha: String,
    commit: GitCommitJson,
    author: Option<UserJson>,
}

#[derive(Deserialize)]
struct GitCommitJson {
    message: String,
    author: Option<GitAuthorJson>,
}

#[derive(Deserialize)]
struct GitAuthorJson {
    name: String,
}

#[derive(Deserialize)]
struct UserJson {
    login: String,
}

#[derive(Deserialize)]
struct ReleaseJson {
    id: u64,
    tag_name: String,
    name: Option<String>,
    draft: bool,
}

/// The repository in an answer of GitHub, if it is one with a name and a default branch that
/// PuppyBot takes.
#[must_use]
pub fn parse_repo(body: &[u8]) -> Option<RepoInfo> {
    let json: RepoJson = serde_json::from_slice(body).ok()?;
    let repo = Repo::parse(&json.full_name)?;
    is_valid_branch(&json.default_branch).then_some(RepoInfo {
        repo,
        public: !json.private,
        default_branch: json.default_branch,
    })
}

/// The commits in an answer of GitHub, the newest first, without those whose hash isn't one.
#[must_use]
pub fn parse_commits(body: &[u8]) -> Option<Vec<OwnedCommit>> {
    let json: Vec<CommitJson> = serde_json::from_slice(body).ok()?;
    let commits = json
        .into_iter()
        .filter(|commit| {
            (7..=64).contains(&commit.sha.len())
                && commit.sha.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .map(|commit| {
            let title = commit.commit.message.lines().next().unwrap_or_default();
            let author = commit
                .author
                .map(|user| user.login)
                .or_else(|| commit.commit.author.map(|author| author.name))
                .unwrap_or_default();
            OwnedCommit {
                sha: commit.sha,
                title: title.trim().to_owned(),
                author,
            }
        })
        .collect();
    Some(commits)
}

/// The releases in an answer of GitHub, without drafts.
#[must_use]
pub fn parse_releases(body: &[u8]) -> Option<Vec<OwnedRelease>> {
    let json: Vec<ReleaseJson> = serde_json::from_slice(body).ok()?;
    let releases = json
        .into_iter()
        .filter(|release| !release.draft)
        .map(|release| OwnedRelease {
            id: release.id,
            tag: release.tag_name,
            name: release.name.unwrap_or_default(),
        })
        .collect();
    Some(releases)
}

/// How long PuppyBot waits between its checks of GitHub, which take `requests`: `configured`, or
/// longer without a token, as GitHub allows 60 requests an hour then, of which PuppyBot uses
/// at most 50.
#[must_use]
pub fn interval(configured: Duration, requests: usize, token: bool) -> Duration {
    if token {
        return configured;
    }
    let needed =
        Duration::from_secs(72).saturating_mul(u32::try_from(requests).unwrap_or(u32::MAX));
    configured.max(needed)
}

/// GitHub's API, as PuppyBot asks it.
pub struct Github {
    client: Client<HttpsConnector<HttpConnector>, Empty<Bytes>>,
    api: String,
    token: Option<SecretString>,
    timeout: Duration,
    /// The `ETag`s of the last answers, by the paths of the requests.
    etags: RefCell<BTreeMap<String, String>>,
    /// Until when GitHub asked PuppyBot to wait.
    paused: Cell<Option<Instant>>,
}

impl Github {
    /// GitHub's API at `api`, [`API`] or one for tests, with `token` if there is one, waiting
    /// `timeout` for each answer. Its connections check GitHub's certificate with the system's
    /// CA certificates, which `discord::prepare_tls` checked, with rustls and ring.
    ///
    /// # Errors
    ///
    /// [`rustls::Error`] if ring can't speak the versions of TLS that rustls would.
    pub fn new(
        api: &str,
        token: Option<SecretString>,
        timeout: Duration,
    ) -> Result<Self, rustls::Error> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            // http:// only for the API of the tests, which the configuration checks.
            .https_or_http()
            .enable_http1()
            .build();
        Ok(Self {
            client: Client::builder(TokioExecutor::new()).build(connector),
            api: api.trim_end_matches('/').to_owned(),
            token,
            timeout,
            etags: RefCell::new(BTreeMap::new()),
            paused: Cell::new(None),
        })
    }

    /// Whether PuppyBot has a token for GitHub.
    #[must_use]
    pub const fn has_token(&self) -> bool {
        self.token.is_some()
    }

    /// What GitHub says of `repo`.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if GitHub has no such repository, or doesn't show it.
    pub async fn repo(&self, repo: &str) -> Result<RepoInfo, Error> {
        let body = self.get(&format!("/repos/{repo}"), false).await?;
        body.as_deref()
            .and_then(parse_repo)
            .ok_or_else(|| Error::Failed("GitHub sent no repository".to_owned()))
    }

    /// The newest commits of `branch` of `repo`, the newest first, or none if they are the same
    /// as at the last check, when `changed` asks only for changes.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if the repository or the branch isn't there.
    pub async fn commits(
        &self,
        repo: &str,
        branch: &str,
        changed: bool,
    ) -> Result<Option<Vec<OwnedCommit>>, Error> {
        let path = format!("/repos/{repo}/commits?sha={branch}&per_page={COMMITS}");
        self.list(&path, changed, parse_commits).await
    }

    /// The newest releases of `repo`, or none if they are the same as at the last check, when
    /// `changed` asks only for changes.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if the repository isn't there.
    pub async fn releases(
        &self,
        repo: &str,
        changed: bool,
    ) -> Result<Option<Vec<OwnedRelease>>, Error> {
        let path = format!("/repos/{repo}/releases?per_page={RELEASES}");
        self.list(&path, changed, parse_releases).await
    }

    /// The list at `path`, read with `parse`, see [`Self::commits`].
    async fn list<T>(
        &self,
        path: &str,
        changed: bool,
        parse: fn(&[u8]) -> Option<T>,
    ) -> Result<Option<T>, Error> {
        match self.get(path, changed).await? {
            Some(body) => parse(&body)
                .map(Some)
                .ok_or_else(|| Error::Failed("GitHub sent no list".to_owned())),
            None => Ok(None),
        }
    }

    /// GitHub's answer to `path`, e.g. `/repos/a/b`: none if it is the same as last time, which
    /// only a request that asks only for changes, `changed`, finds.
    async fn get(&self, path: &str, changed: bool) -> Result<Option<Bytes>, Error> {
        if self
            .paused
            .get()
            .is_some_and(|until| Instant::now() < until)
        {
            return Err(Error::Limited);
        }
        let mut request = Request::get(format!("{}{path}", self.api))
            .header(
                header::USER_AGENT,
                concat!("PuppyBot/", env!("CARGO_PKG_VERSION")),
            )
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("x-github-api-version", API_VERSION);
        if let Some(token) = &self.token {
            let mut value = HeaderValue::try_from(format!("Bearer {}", token.expose_secret()))
                .map_err(|_| Error::Failed("the GitHub token isn't a header".to_owned()))?;
            // Never printed, e.g. in an error.
            value.set_sensitive(true);
            request = request.header(header::AUTHORIZATION, value);
        }
        let etag = changed
            .then(|| self.etags.borrow().get(path).cloned())
            .flatten();
        if let Some(etag) = etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let request = request
            .body(Empty::new())
            .map_err(|error| Error::Failed(error.to_string()))?;
        let response = tokio::time::timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| Error::Failed("GitHub didn't answer in time".to_owned()))?
            .map_err(|error| Error::Failed(format!("can't reach GitHub: {error}")))?;
        let status = response.status();
        let paused = self.pause(response.headers());
        if status == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        if matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
        ) {
            return Err(Error::NotFound);
        }
        if paused
            && matches!(
                status,
                StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
            )
        {
            return Err(Error::Limited);
        }
        if !status.is_success() {
            return Err(Error::Failed(format!("GitHub answered {status}")));
        }
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|etag| etag.to_str().ok())
            .map(str::to_owned);
        let body = Limited::new(response.into_body(), MAX_BODY)
            .collect()
            .await
            .map_err(|error| Error::Failed(format!("can't read GitHub's answer: {error}")))?
            .to_bytes();
        if let Some(etag) = etag {
            self.etags.borrow_mut().insert(path.to_owned(), etag);
        }
        Ok(Some(body))
    }

    /// Pauses the requests as GitHub's `headers` say: for the seconds of `retry-after`, or until
    /// `x-ratelimit-reset` when no requests are left, but at most [`MAX_PAUSE`]. Returns whether
    /// it paused.
    fn pause(&self, headers: &HeaderMap) -> bool {
        let number = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let wait = number("retry-after").or_else(|| {
            (number("x-ratelimit-remaining") == Some(0))
                .then(|| number("x-ratelimit-reset").map_or(60, |reset| reset.saturating_sub(now)))
        });
        let Some(wait) = wait else {
            return false;
        };
        let wait = Duration::from_secs(wait).min(MAX_PAUSE);
        tracing::warn!(
            seconds = wait.as_secs(),
            "GitHub asks PuppyBot to wait before it asks again"
        );
        self.paused.set(Some(Instant::now() + wait));
        true
    }
}

#[cfg(test)]
mod tests {
    use tokio::net::TcpListener;

    use super::*;
    use crate::fake_github::{FakeGithub, commits_json, releases_json, repo_json};

    /// Runs `test` in a runtime like the program's, with a log at every level that goes nowhere:
    /// tracing builds only the messages that a log wants, and the tests reach them all.
    fn in_runtime(test: impl Future<Output = ()>) {
        crate::discord::prepare_tls().unwrap();
        let log = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        tracing::subscriber::with_default(log, || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(test);
        });
    }

    #[test]
    fn what_github_answers() {
        assert_eq!(
            parse_repo(repo_json("PurplePuppyProject/PuppyBot", false).as_bytes()),
            Some(RepoInfo {
                repo: Repo::parse("PurplePuppyProject/PuppyBot").unwrap(),
                public: true,
                default_branch: "main".to_owned(),
            })
        );
        assert_eq!(parse_repo(repo_json("no", false).as_bytes()), None);
        assert_eq!(
            parse_repo(br#"{"full_name": "a/b", "private": true, "default_branch": "-x"}"#),
            None
        );
        assert_eq!(parse_repo(b"[]"), None);

        let commits = format!(
            "[{}, {}, {}]",
            r#"{"sha": "a1b2c3d4", "commit": {"message": " Roll dice \r\nMore", "author": {"name": "Puppy"}}, "author": null}"#,
            r#"{"sha": "not a hash", "commit": {"message": "x", "author": null}, "author": null}"#,
            r#"{"sha": "0f9e8d7", "commit": {"message": "", "author": null}, "author": {"login": "rex"}}"#
        );
        assert_eq!(
            parse_commits(commits.as_bytes()),
            Some(vec![
                OwnedCommit {
                    sha: "a1b2c3d4".to_owned(),
                    title: "Roll dice".to_owned(),
                    author: "Puppy".to_owned(),
                },
                OwnedCommit {
                    sha: "0f9e8d7".to_owned(),
                    title: String::new(),
                    author: "rex".to_owned(),
                },
            ])
        );
        assert_eq!(parse_commits(b"{}"), None);

        let releases = r#"[{"id": 2, "tag_name": "v2", "name": null, "draft": false},
            {"id": 3, "tag_name": "v3", "name": "Soon", "draft": true},
            {"id": 1, "tag_name": "v1", "name": "First", "draft": false}]"#;
        let releases = parse_releases(releases.as_bytes()).unwrap();
        let tags: Vec<_> = releases
            .iter()
            .map(|release| {
                (
                    release.release().id,
                    release.release().tag,
                    release.release().name,
                )
            })
            .collect();
        assert_eq!(tags, [(2, "v2", ""), (1, "v1", "First")]);
        assert_eq!(parse_releases(b"null"), None);
    }

    #[test]
    fn checks_without_a_token_come_less_often() {
        let five = Duration::from_secs(300);
        assert_eq!(interval(five, 100, true), five);
        assert_eq!(interval(five, 2, false), five);
        assert_eq!(interval(five, 10, false), Duration::from_secs(720));
        assert_eq!(
            interval(five, usize::MAX, false).as_secs(),
            72 * u64::from(u32::MAX)
        );
    }

    #[test]
    fn requests_and_answers() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let token = SecretString::from("github_pat_x");
            let github = Github::new(&format!("{}/", fake.url), Some(token), TIMEOUT).unwrap();
            assert!(github.has_token());
            fake.answer(
                "/repos/PurplePuppyProject/PuppyBot",
                200,
                &[],
                &repo_json("PurplePuppyProject/PuppyBot", false),
            );
            let info = github.repo("PurplePuppyProject/PuppyBot").await.unwrap();
            assert!(info.public);
            let commits = "/repos/a/b/commits?sha=main&per_page=30";
            fake.answer(
                commits,
                200,
                &[("etag", "\"1\"")],
                &commits_json(&[("abcdef0", "Wuff")]),
            );
            let first = github.commits("a/b", "main", true).await.unwrap().unwrap();
            assert_eq!(first[0].commit().title, "Wuff");
            // The same again: not modified.
            assert_eq!(github.commits("a/b", "main", true).await, Ok(None));
            // Unless it asks for all.
            assert!(
                github
                    .commits("a/b", "main", false)
                    .await
                    .unwrap()
                    .is_some()
            );
            let releases = "/repos/a/b/releases?per_page=10";
            fake.answer(releases, 200, &[], &releases_json(&[(7, "v1")]));
            let found = github.releases("a/b", true).await.unwrap().unwrap();
            assert_eq!(found[0].release().tag, "v1");

            // The token, and the ETag of the last answer.
            let got = fake.got();
            assert_eq!(got[0].token.as_deref(), Some("Bearer github_pat_x"));
            assert_eq!(
                (
                    got[1].etag.as_deref(),
                    got[2].etag.as_deref(),
                    got[3].etag.as_deref()
                ),
                (None, Some("\"1\""), None)
            );
        });
    }

    #[test]
    fn answers_that_are_errors() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let github = Github::new(&fake.url, None, Duration::from_millis(300)).unwrap();
            assert!(!github.has_token());
            // Not there.
            assert_eq!(github.repo("a/b").await, Err(Error::NotFound));
            fake.answer("/repos/a/b/commits?sha=x&per_page=30", 422, &[], "{}");
            assert_eq!(github.commits("a/b", "x", true).await, Err(Error::NotFound));
            // Not what GitHub sends.
            fake.answer("/repos/a/b", 200, &[], "[]");
            assert_eq!(
                github.repo("a/b").await,
                Err(Error::Failed("GitHub sent no repository".to_owned()))
            );
            fake.answer("/repos/a/b/releases?per_page=10", 200, &[], "{}");
            assert_eq!(
                github.releases("a/b", false).await,
                Err(Error::Failed("GitHub sent no list".to_owned()))
            );
            // Another status.
            fake.answer("/repos/a/b", 500, &[], "");
            assert_eq!(
                github.repo("a/b").await,
                Err(Error::Failed(
                    "GitHub answered 500 Internal Server Error".to_owned()
                ))
            );
            // Too large.
            fake.answer("/repos/a/b", 200, &[], &" ".repeat(MAX_BODY + 1));
            let error = github.repo("a/b").await.unwrap_err().to_string();
            assert!(error.starts_with("can't read GitHub's answer: "), "{error}");
            // Forbidden, but not as the limit.
            fake.answer("/repos/a/b", 403, &[], "{}");
            assert_eq!(
                github.repo("a/b").await,
                Err(Error::Failed("GitHub answered 403 Forbidden".to_owned()))
            );
            // The limit: no requests until GitHub's time.
            fake.answer("/repos/a/b", 403, &[("x-ratelimit-remaining", "0")], "{}");
            assert_eq!(github.repo("a/b").await, Err(Error::Limited));
            let asked = fake.got().len();
            assert_eq!(github.repo("a/b").await, Err(Error::Limited));
            assert_eq!(fake.got().len(), asked);
            assert_eq!(
                Error::Limited.to_string(),
                "GitHub's limit of requests is reached"
            );
            assert_eq!(Error::NotFound.to_string(), "GitHub has no such thing");
        });
    }

    #[test]
    fn github_asks_to_wait() {
        in_runtime(async {
            let fake = FakeGithub::start().await;
            let github = Github::new(&fake.url, None, TIMEOUT).unwrap();
            fake.answer("/repos/a/b", 429, &[("retry-after", "0")], "{}");
            assert_eq!(github.repo("a/b").await, Err(Error::Limited));
            // A pause of 0 seconds is over at once, and an answer within the limit pauses
            // nothing.
            let within = [("x-ratelimit-remaining", "4999")];
            fake.answer("/repos/a/b", 200, &within, &repo_json("a/b", false));
            assert!(github.repo("a/b").await.is_ok());
            assert!(github.repo("a/b").await.is_ok());
            // The last request of the hour: none until GitHub's time.
            let reset = (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 30)
                .to_string();
            let last = [
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", reset.as_str()),
            ];
            fake.answer("/repos/a/b", 200, &last, &repo_json("a/b", false));
            assert!(github.repo("a/b").await.is_ok());
            assert_eq!(github.repo("a/b").await, Err(Error::Limited));

            // Without a time to wait for: a minute.
            let github = Github::new(&fake.url, None, TIMEOUT).unwrap();
            fake.answer("/repos/a/b", 403, &[("x-ratelimit-remaining", "0")], "{}");
            assert_eq!(github.repo("a/b").await, Err(Error::Limited));
            let paused = github.paused.get().unwrap();
            assert!(paused > Instant::now() + Duration::from_secs(50));
        });
    }

    #[test]
    fn no_answer() {
        in_runtime(async {
            // Nothing listens there.
            let github = Github::new("http://127.0.0.1:1", None, TIMEOUT).unwrap();
            let error = github.releases("a/b", true).await.unwrap_err().to_string();
            assert!(error.starts_with("can't reach GitHub: "), "{error}");
            // Nothing answers there in time.
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let github = Github::new(&url, None, Duration::from_millis(100)).unwrap();
            assert_eq!(
                github.releases("a/b", true).await,
                Err(Error::Failed("GitHub didn't answer in time".to_owned()))
            );
            drop(listener);
            // A token that can't be a header.
            let github = Github::new(&url, Some(SecretString::from("a\nb")), TIMEOUT).unwrap();
            assert_eq!(
                github.releases("a/b", true).await,
                Err(Error::Failed("the GitHub token isn't a header".to_owned()))
            );
            // A path that can't be an address, which PuppyBot's checked names never make.
            let github = Github::new(&url, None, TIMEOUT).unwrap();
            let error = github.releases("a b", true).await.unwrap_err();
            assert!(matches!(error, Error::Failed(_)), "{error:?}");
        });
    }
}
