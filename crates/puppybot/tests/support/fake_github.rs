//! GitHub on this machine, for the unit tests of the program, which include this file: an API
//! that answers what the tests set, as GitHub would. It lives with the support of the tests, as
//! the fake Discord in `common` does, as it is test code that tests nothing itself.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

/// What [`FakeGithub`] answers to a path.
#[derive(Debug, Clone)]
struct Route {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// A request that [`FakeGithub`] got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Got {
    /// The path, with the query.
    pub path: String,
    /// Its token, if it had one.
    pub token: Option<String>,
    /// Its `ETag`, if it asked only for changes.
    pub etag: Option<String>,
}

/// GitHub on this machine, for the tests: it answers each path with what [`Self::answer`]
/// set for it, or 404, and "not modified" to a request with the `ETag` of the answer.
pub struct FakeGithub {
    pub url: String,
    routes: Arc<Mutex<BTreeMap<String, Route>>>,
    got: Arc<Mutex<Vec<Got>>>,
}

impl FakeGithub {
    /// Starts it in the runtime of the test.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes = Arc::new(Mutex::new(BTreeMap::new()));
        let got = Arc::new(Mutex::new(Vec::new()));
        let (server_routes, server_got) = (Arc::clone(&routes), Arc::clone(&got));
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (routes, got) = (Arc::clone(&server_routes), Arc::clone(&server_got));
                let service = service_fn(move |request: hyper::Request<Incoming>| {
                    let reply = reply(&request, &routes, &got);
                    async move { Ok::<_, Infallible>(reply) }
                });
                tokio::spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Self { url, routes, got }
    }

    /// From now on, answers `path` with `status`, `headers` and `body`.
    pub fn answer(&self, path: &str, status: u16, headers: &[(&str, &str)], body: &str) {
        let route = Route {
            status,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body: body.to_owned(),
        };
        self.routes.lock().unwrap().insert(path.to_owned(), route);
    }

    /// The requests that it got so far.
    pub fn got(&self) -> Vec<Got> {
        self.got.lock().unwrap().clone()
    }
}

/// The answer of [`FakeGithub`] to `request`, which it records in `got`.
fn reply(
    request: &hyper::Request<Incoming>,
    routes: &Mutex<BTreeMap<String, Route>>,
    got: &Mutex<Vec<Got>>,
) -> hyper::Response<Full<Bytes>> {
    let header = |name| {
        request
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap().to_owned())
    };
    let path = request.uri().path_and_query().unwrap().to_string();
    let etag = header(header::IF_NONE_MATCH);
    got.lock().unwrap().push(Got {
        path: path.clone(),
        token: header(header::AUTHORIZATION),
        etag: etag.clone(),
    });
    let route = routes.lock().unwrap().get(&path).cloned();
    let route = route.unwrap_or(Route {
        status: 404,
        headers: Vec::new(),
        body: r#"{"message": "Not Found"}"#.to_owned(),
    });
    let mut response = hyper::Response::builder();
    let current = route
        .headers
        .iter()
        .find(|(name, _)| name == "etag")
        .map(|(_, value)| value.clone());
    let unchanged = etag.is_some() && etag == current;
    for (name, value) in &route.headers {
        response = response.header(name, value);
    }
    let (status, body) = if unchanged {
        (304, String::new())
    } else {
        (route.status, route.body)
    };
    response
        .status(status)
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

/// The answer of GitHub to a request for a repository.
pub fn repo_json(name: &str, private: bool) -> String {
    format!(r#"{{"full_name": "{name}", "private": {private}, "default_branch": "main"}}"#)
}

/// The answer of GitHub to a request for commits, with these hashes and titles, the
/// newest first.
pub fn commits_json(commits: &[(&str, &str)]) -> String {
    let commits: Vec<String> = commits
        .iter()
        .map(|(sha, title)| {
            format!(
                r#"{{"sha": "{sha}", "commit": {{"message": "{title}\n\nMore.", "author": {{"name": "Rex"}}}}, "author": {{"login": "rex"}}}}"#
            )
        })
        .collect();
    format!("[{}]", commits.join(","))
}

/// The answer of GitHub to a request for releases, with these IDs and tags.
pub fn releases_json(releases: &[(u64, &str)]) -> String {
    let releases: Vec<String> = releases
        .iter()
        .map(|(id, tag)| {
            format!(
                r#"{{"id": {id}, "tag_name": "{tag}", "name": "Release {tag}", "draft": false}}"#
            )
        })
        .collect();
    format!("[{}]", releases.join(","))
}
