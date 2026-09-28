//! From any URL on a DreamWeave site to that site's `dreamweave.json`, by the protocol's own
//! discovery algorithm and nothing else.
//!
//! 1. GET the URL. A DreamWeave JSON document is used as it is: an index is the answer, a
//!    project manifest is walked up from until its index turns up.
//! 2. An HTML page is read for exactly one thing: `<link rel="alternate">` with a DreamWeave media
//!    type. An advertised index is fetched; an advertised manifest is walked up from.
//! 3. Otherwise, try `dreamweave.json` in the URL's directory, then in each parent, up to the
//!    origin's root. This is what finds a GitHub Pages project site below `you.github.io`.
//!
//! An author never needs to know where their `dreamweave.json` lives. Any page will do.

use std::fmt;

use scraper::{Html, Selector};
use url::Url;

use crate::{
    fetch::{FetchError, Fetcher, Response, Validators},
    policy,
    protocol::{INDEX_FILE_NAME, INDEX_MEDIA_TYPE, PROJECT_MEDIA_TYPE, SCHEMA_VERSION, parse},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// The URL was the index itself.
    Direct,
    /// An HTML page advertised the index with `<link rel="alternate">`.
    IndexLink,
    /// An HTML page advertised a project manifest; the index was found above it.
    ProjectLink,
    /// The URL was a project manifest; the index was found above it.
    Manifest,
    /// Nothing advertised anything; the index was found by walking up the path.
    PathWalk,
}

impl Method {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Direct => "the URL is the site index",
            Self::IndexLink => "the page links the site index",
            Self::ProjectLink => "the page links a project manifest; the index is above it",
            Self::Manifest => "the URL is a project manifest; the index is above it",
            Self::PathWalk => "found by walking up the path",
        }
    }
}

/// One request discovery made, and what came of it.
#[derive(Debug, Clone)]
pub struct Attempt {
    pub url: Url,
    pub outcome: String,
}

#[derive(Debug, Clone)]
pub struct Discovery {
    pub method: Method,
    /// The index response. `not_modified` when the index URL was already known and the server
    /// agreed nothing changed; the caller keeps its cached bytes.
    pub index: Response,
    pub trail: Vec<Attempt>,
}

impl Discovery {
    pub fn index_url(&self) -> &Url {
        &self.index.url
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The host could not be reached at all.
    Unreachable,
    /// This crawler's policy refused a URL on the way.
    Refused,
    /// The site speaks a newer protocol version.
    UnsupportedVersion,
    /// A page advertised an index that could not be read.
    BrokenAdvertisement,
    /// Nothing anywhere up the path.
    NotFound,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub kind: FailureKind,
    pub detail: String,
    pub trail: Vec<Attempt>,
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

/// The index URL a previous crawl found, and the validators it was served with.
pub struct Known<'a> {
    pub index_url: &'a Url,
    pub validators: &'a Validators,
}

struct Walker<'a> {
    fetcher: &'a Fetcher,
    known: Option<Known<'a>>,
    trail: Vec<Attempt>,
}

pub async fn discover(
    fetcher: &Fetcher,
    source: &Url,
    known: Option<Known<'_>>,
) -> Result<Discovery, Failure> {
    let mut walker = Walker {
        fetcher,
        known,
        trail: Vec::new(),
    };
    walker.run(source).await
}

impl Walker<'_> {
    fn note(&mut self, url: &Url, outcome: impl Into<String>) {
        self.trail.push(Attempt {
            url: url.clone(),
            outcome: outcome.into(),
        });
    }

    fn fail(&mut self, kind: FailureKind, detail: String) -> Failure {
        Failure {
            kind,
            detail,
            trail: std::mem::take(&mut self.trail),
        }
    }

    fn fetch_failure(&mut self, url: &Url, error: &FetchError) -> Failure {
        let kind = if error.is_policy_refusal() {
            FailureKind::Refused
        } else {
            FailureKind::Unreachable
        };
        self.fail(kind, format!("{url}: {error}"))
    }

    fn found(&mut self, method: Method, index: Response) -> Discovery {
        Discovery {
            method,
            index,
            trail: std::mem::take(&mut self.trail),
        }
    }

    fn validators_for(&self, url: &Url) -> Option<&Validators> {
        self.known
            .as_ref()
            .filter(|known| known.index_url == url && !known.validators.is_empty())
            .map(|known| known.validators)
    }

    async fn run(&mut self, source: &Url) -> Result<Discovery, Failure> {
        let validators = self.validators_for(source).cloned();
        let response = match self
            .fetcher
            .get(source, policy::MAXIMUM_MANIFEST_BYTES, validators.as_ref())
            .await
        {
            Ok(response) => response,
            Err(FetchError::Status(status)) => {
                self.note(source, format!("HTTP {status}"));
                return self.walk(source, Method::PathWalk).await;
            }
            Err(error) => {
                self.note(source, error.to_string());
                return Err(self.fetch_failure(source, &error));
            }
        };
        if response.not_modified {
            self.note(&response.url, "site index unchanged (HTTP 304)");
            return Ok(self.found(Method::Direct, response));
        }

        if let Some(envelope) = parse::envelope(&response.body) {
            if envelope.schema_version != SCHEMA_VERSION {
                self.note(
                    &response.url,
                    format!("schema_version {:?}", envelope.schema_version),
                );
                return Err(self.newer_protocol(&response.url, &envelope.schema_version));
            }
            match envelope.document.as_str() {
                "index" => {
                    self.note(&response.url, "a DreamWeave site index");
                    return Ok(self.found(Method::Direct, response));
                }
                "project" => {
                    self.note(&response.url, "a DreamWeave project manifest");
                    let from = response.url.clone();
                    return self.walk(&from, Method::Manifest).await;
                }
                other => {
                    self.note(&response.url, format!("a DreamWeave {other:?} document"));
                    let from = response.url.clone();
                    return self.walk(&from, Method::PathWalk).await;
                }
            }
        }

        if looks_like_html(&response) {
            let links = alternate_links(&response);
            for (href, problem) in &links.malformed {
                self.note(
                    &response.url,
                    format!("ignored <link> href {href:?}: {problem}"),
                );
            }
            if let Some(index_url) = links.index {
                self.note(
                    &response.url,
                    format!("links the site index at {index_url}"),
                );
                return self.advertised_index(&index_url).await;
            }
            if let Some(manifest_url) = links.project {
                self.note(
                    &response.url,
                    format!("links a project manifest at {manifest_url}"),
                );
                return self.walk(&manifest_url, Method::ProjectLink).await;
            }
            self.note(&response.url, "an HTML page with no DreamWeave <link>");
        } else {
            self.note(&response.url, "neither a DreamWeave document nor HTML");
        }
        let from = response.url.clone();
        self.walk(&from, Method::PathWalk).await
    }

    fn newer_protocol(&mut self, url: &Url, version: &str) -> Failure {
        self.fail(
            FailureKind::UnsupportedVersion,
            format!(
                "{url} uses DreamWeave schema_version {version:?}; this index reads \"{SCHEMA_VERSION}\""
            ),
        )
    }

    async fn advertised_index(&mut self, url: &Url) -> Result<Discovery, Failure> {
        if let Some(response) = self.probe(url).await? {
            return Ok(self.found(Method::IndexLink, response));
        }
        let outcome = self
            .trail
            .last()
            .map(|attempt| attempt.outcome.clone())
            .unwrap_or_default();
        Err(self.fail(
            FailureKind::BrokenAdvertisement,
            format!("the page links a site index at {url}, but it could not be read ({outcome})"),
        ))
    }

    async fn walk(&mut self, from: &Url, method: Method) -> Result<Discovery, Failure> {
        for candidate in walk_candidates(from) {
            if let Some(response) = self.probe(&candidate).await? {
                return Ok(self.found(method, response));
            }
        }
        let root = from
            .join("/")
            .map_or_else(|_| from.to_string(), |root| root.to_string());
        Err(self.fail(
            FailureKind::NotFound,
            format!(
                "no {INDEX_FILE_NAME} between {from} and the root of {root}, and no page advertised one"
            ),
        ))
    }

    /// One candidate index URL: `None` when it is missing or not an index, so keep looking. Unreachable host, refused
    /// URL, or a newer protocol: stop, because every further candidate shares the problem.
    async fn probe(&mut self, url: &Url) -> Result<Option<Response>, Failure> {
        let validators = self.validators_for(url).cloned();
        match self
            .fetcher
            .get(url, policy::MAXIMUM_INDEX_BYTES, validators.as_ref())
            .await
        {
            Ok(response) if response.not_modified => {
                self.note(url, "site index unchanged (HTTP 304)");
                Ok(Some(response))
            }
            Ok(response) => match parse::envelope(&response.body) {
                Some(envelope) if envelope.schema_version != SCHEMA_VERSION => {
                    self.note(url, format!("schema_version {:?}", envelope.schema_version));
                    Err(self.newer_protocol(url, &envelope.schema_version))
                }
                Some(envelope) if envelope.document == "index" => {
                    self.note(url, "a DreamWeave site index");
                    Ok(Some(response))
                }
                Some(envelope) => {
                    self.note(
                        url,
                        format!("a DreamWeave {:?} document", envelope.document),
                    );
                    Ok(None)
                }
                None => {
                    self.note(url, "not a DreamWeave document");
                    Ok(None)
                }
            },
            Err(FetchError::Status(status)) => {
                self.note(url, format!("HTTP {status}"));
                Ok(None)
            }
            Err(error) => {
                self.note(url, error.to_string());
                Err(self.fetch_failure(url, &error))
            }
        }
    }
}

/// `dreamweave.json` in the URL's directory, then in each parent up to the origin's root.
pub fn walk_candidates(from: &Url) -> Vec<Url> {
    let mut directory = from.clone();
    directory.set_query(None);
    directory.set_fragment(None);
    let mut segments: Vec<String> = directory
        .path_segments()
        .map(|segments| segments.map(str::to_owned).collect())
        .unwrap_or_default();
    // The last segment names a file unless the path ends in `/`, which leaves an empty one.
    segments.pop();
    let mut candidates = Vec::new();
    loop {
        let mut candidate = directory.clone();
        let mut path = String::from("/");
        for segment in &segments {
            path.push_str(segment);
            path.push('/');
        }
        path.push_str(INDEX_FILE_NAME);
        candidate.set_path(&path);
        candidates.push(candidate);
        if segments.pop().is_none() {
            break;
        }
    }
    candidates
}

fn looks_like_html(response: &Response) -> bool {
    if let Some(media_type) = &response.media_type {
        return matches!(media_type.as_str(), "text/html" | "application/xhtml+xml");
    }
    let start = response
        .body
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(0);
    response.body[start..].starts_with(b"<")
}

#[derive(Debug, Default)]
struct AlternateLinks {
    index: Option<Url>,
    project: Option<Url>,
    malformed: Vec<(String, String)>,
}

/// Reads `<link rel="alternate">` elements with a DreamWeave media type and nothing else: the
/// protocol forbids scraping anything more from a page.
fn alternate_links(response: &Response) -> AlternateLinks {
    let text = String::from_utf8_lossy(&response.body);
    let document = Html::parse_document(&text);
    let selector = Selector::parse("link[href]").expect("a static selector parses");
    let mut links = AlternateLinks::default();
    for element in document.select(&selector) {
        let element = element.value();
        let alternate = element.attr("rel").is_some_and(|rel| {
            rel.split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("alternate"))
        });
        let media_type = element.attr("type").map(str::trim).unwrap_or_default();
        let is_index = media_type.eq_ignore_ascii_case(INDEX_MEDIA_TYPE);
        let is_project = media_type.eq_ignore_ascii_case(PROJECT_MEDIA_TYPE);
        if !alternate || !(is_index || is_project) {
            continue;
        }
        let href = element.attr("href").unwrap_or_default().trim();
        let resolved = match response.url.join(href) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => url,
            Ok(url) => {
                links
                    .malformed
                    .push((href.to_owned(), format!("{} is not http(s)", url.scheme())));
                continue;
            }
            Err(error) => {
                links.malformed.push((href.to_owned(), error.to_string()));
                continue;
            }
        };
        if is_index && links.index.is_none() {
            links.index = Some(resolved);
        } else if is_project && links.project.is_none() {
            links.project = Some(resolved);
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates(from: &str) -> Vec<String> {
        walk_candidates(&Url::parse(from).unwrap())
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn walks_from_a_page_up_to_the_root() {
        assert_eq!(
            candidates("https://you.github.io/cool-mods/candlelight/index.html?tab=1#install"),
            [
                "https://you.github.io/cool-mods/candlelight/dreamweave.json",
                "https://you.github.io/cool-mods/dreamweave.json",
                "https://you.github.io/dreamweave.json",
            ]
        );
    }

    #[test]
    fn a_trailing_slash_is_a_directory() {
        assert_eq!(
            candidates("https://you.github.io/cool-mods/"),
            [
                "https://you.github.io/cool-mods/dreamweave.json",
                "https://you.github.io/dreamweave.json",
            ]
        );
        assert_eq!(
            candidates("https://example.org"),
            ["https://example.org/dreamweave.json"]
        );
    }

    #[test]
    fn a_manifest_url_walks_to_the_site_root() {
        assert_eq!(
            candidates("https://you.github.io/cool-mods/dreamweave/projects/4d0c9f6e.json"),
            [
                "https://you.github.io/cool-mods/dreamweave/projects/dreamweave.json",
                "https://you.github.io/cool-mods/dreamweave/dreamweave.json",
                "https://you.github.io/cool-mods/dreamweave.json",
                "https://you.github.io/dreamweave.json",
            ]
        );
    }
}
