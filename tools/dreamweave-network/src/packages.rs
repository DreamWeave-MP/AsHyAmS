//! Jess package feeds, format version 1: the static document a site publishes as
//! `jess-packages.json` beside its `dreamweave.json`, naming the JPKs it distributes.
//!
//! This is not the DreamWeave protocol and does not change it. A `schema_version` "2" release has
//! to carry a game-data, binary or crate artifact, and a Jess package is none of those, so the
//! packages a site distributes are described in a document of their own, found by the same
//! means as the site index: the feed is always `jess-packages.json` in the directory
//! `dreamweave.json` lives in. If the protocol's next major version gives packages a place of
//! their own, every field here has a name there already.
//!
//! The same three rules apply as to a manifest. The publisher is the authority: this index
//! copies the feed and says where it came from. A package id is a claim, never proof of who may
//! publish it: the same id from two sites is shown as two claims. And nothing here is fetched or
//! run: the archive URLs are the publisher's, and only a Jess client downloads and verifies them.
//!
//! The document carries two digests per version and they are different facts. `content_digest`
//! is Jess's own: BLAKE3 over the package's canonical content, the digest `jess.lock` pins, which
//! is the same however the archive is packed. `artifact.digests.sha256` is the digest of the
//! archive file as served. A client checks both.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::LazyLock,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    policy,
    protocol::{ArtifactSource, Digests, ProjectId, Signature, is_project_id},
    version::{Scheme, Version},
};

pub const FORMAT: &str = "jess-packages";
pub const FORMAT_VERSION: u64 = 1;
/// The feed's file name, beside `dreamweave.json`.
pub const FILE_NAME: &str = "jess-packages.json";
/// The media type of a JPK archive, as the feed and a publisher's server name it.
pub const ARTIFACT_MEDIA_TYPE: &str = "application/vnd.jess.jpk";
pub const SCHEMA_PATH: &str = "static/schemas/jess-packages-1.schema.json";

static SCHEMA: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../static/schemas/jess-packages-1.schema.json"
    ))
    .expect("the feed schema is JSON");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the feed schema compiles")
});

/// How many schema or rule violations one report lists before it stops counting.
const REPORTED_PROBLEMS: usize = 12;

/// A Jess package id, as `jess.toml` declares it: reverse-DNS, lowercase. Chosen by the package's
/// author, which says nothing about who may publish it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageId(pub String);

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feed {
    pub format: String,
    pub format_version: u64,
    pub publisher: Publisher,
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publisher {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub id: PackageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The DreamWeave project this package belongs to, on the same site. Never resolved across
    /// sites: a project id is a claim there too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Newest first. Yanked versions stay listed.
    pub versions: Vec<PackageVersion>,
}

impl Package {
    /// The newest version the publisher still offers for new installs, if any.
    pub fn current(&self) -> Option<&PackageVersion> {
        self.versions
            .iter()
            .find(|version| version.status == VersionStatus::Available)
    }

    pub fn version(&self, text: &str) -> Option<&PackageVersion> {
        self.versions.iter().find(|version| version.version == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Available,
    Yanked,
}

impl VersionStatus {
    pub fn token(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Yanked => "yanked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Yanked {
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageVersion {
    pub version: String,
    pub jess_api: String,
    pub status: VersionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yanked: Option<Yanked>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Jess's content digest: BLAKE3 over canonical package content, what `jess.lock` pins.
    pub content_digest: String,
    /// The package's declared `[dependencies]`, by id, as Jess requirements.
    pub dependencies: BTreeMap<String, String>,
    pub artifact: Artifact,
}

/// The archive file. Its digest is of the bytes as served, which is not the content digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub filename: String,
    pub media_type: String,
    pub size: u64,
    pub digests: Digests,
    pub sources: Vec<ArtifactSource>,
    pub signatures: Vec<Signature>,
}

// Parsing -------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NotJson(String),
    /// JSON without `format` and `format_version`: not a package feed at all.
    NotFeed,
    /// A `format_version` this index cannot read.
    UnsupportedVersion(String),
    Schema(Vec<String>),
    Rules(Vec<String>),
    /// Refused by this crawler's resource policy. Not a problem with the feed.
    Policy(String),
}

impl Problem {
    pub fn is_policy_refusal(&self) -> bool {
        matches!(self, Self::Policy(_))
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |formatter: &mut fmt::Formatter<'_>, lead: &str, items: &[String]| {
            write!(formatter, "{lead}: {}", items.join("; "))
        };
        match self {
            Self::NotJson(reason) => write!(formatter, "not JSON: {reason}"),
            Self::NotFeed => formatter
                .write_str("JSON without format and format_version, so not a Jess package feed"),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "{FORMAT} format_version {version}: a feed version this index does not read (it reads {FORMAT_VERSION})"
            ),
            Self::Schema(errors) => list(formatter, "does not match its JSON Schema", errors),
            Self::Rules(errors) => list(formatter, "breaks the feed's rules", errors),
            Self::Policy(reason) => write!(formatter, "refused by crawler policy: {reason}"),
        }
    }
}

impl std::error::Error for Problem {}

/// Whether bytes claim to be a package feed, before anything in them is believed.
pub fn looks_like_feed(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
        value.get("format").and_then(Value::as_str) == Some(FORMAT)
            && value.get("format_version").is_some()
    })
}

fn url_problem(field: &str, text: &str) -> Option<String> {
    match Url::parse(text) {
        Ok(url) if matches!(url.scheme(), "http" | "https") && url.host().is_some() => None,
        Ok(_) => Some(format!(
            "{field} {text:?} is not an http(s) URL with a host"
        )),
        Err(error) => Some(format!("{field} {text:?} is not a URL: {error}")),
    }
}

fn check_strings(value: &Value, path: &str) -> Result<(), Problem> {
    let too_long = |path: &str| {
        Problem::Policy(format!(
            "a string at {} is longer than {} bytes",
            if path.is_empty() { "/" } else { path },
            policy::MAXIMUM_STRING_BYTES
        ))
    };
    match value {
        Value::String(text) if text.len() > policy::MAXIMUM_STRING_BYTES => Err(too_long(path)),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(position, item)| check_strings(item, &format!("{path}/{position}"))),
        Value::Object(fields) => fields.iter().try_for_each(|(key, item)| {
            if key.len() > policy::MAXIMUM_STRING_BYTES {
                return Err(too_long(path));
            }
            check_strings(item, &format!("{path}/{key}"))
        }),
        _ => Ok(()),
    }
}

fn check_counts(value: &Value) -> Result<(), Problem> {
    let count = |value: Option<&Value>| value.and_then(Value::as_array).map_or(0, Vec::len);
    let refuse = |what: &str, found: usize, limit: usize| {
        Err(Problem::Policy(format!(
            "{found} {what}; this crawler reads at most {limit}"
        )))
    };
    let packages = value.get("packages").and_then(Value::as_array);
    let listed = packages.map_or(0, Vec::len);
    if listed > policy::MAXIMUM_PACKAGES_PER_FEED {
        return refuse(
            "packages in one feed",
            listed,
            policy::MAXIMUM_PACKAGES_PER_FEED,
        );
    }
    for package in packages.into_iter().flatten() {
        let versions = package.get("versions").and_then(Value::as_array);
        let listed = versions.map_or(0, Vec::len);
        if listed > policy::MAXIMUM_RELEASES_PER_PROJECT {
            return refuse(
                "versions of one package",
                listed,
                policy::MAXIMUM_RELEASES_PER_PROJECT,
            );
        }
        for version in versions.into_iter().flatten() {
            let sources = count(version.pointer("/artifact/sources"));
            if sources > policy::MAXIMUM_SOURCES_PER_ARTIFACT {
                return refuse(
                    "sources for one artifact",
                    sources,
                    policy::MAXIMUM_SOURCES_PER_ARTIFACT,
                );
            }
        }
    }
    Ok(())
}

fn check_schema(value: &Value) -> Result<(), Problem> {
    let errors: Vec<String> = SCHEMA
        .iter_errors(value)
        .take(REPORTED_PROBLEMS)
        .map(|error| {
            let path = error.instance_path().to_string();
            let mut message = error.to_string();
            if message.len() > 240 {
                let mut end = 240;
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
                message.push('…');
            }
            format!(
                "at {}: {message}",
                if path.is_empty() { "/" } else { &path }
            )
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Problem::Schema(errors))
    }
}

/// Untrusted bytes in, a validated feed out: JSON, then the envelope, then this crawler's
/// resource policy, then the schema, then the rules a schema cannot state.
pub fn parse_feed(bytes: &[u8]) -> Result<Feed, Problem> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| Problem::NotJson(error.to_string()))?;
    let format = value.get("format").and_then(Value::as_str);
    let version = value.get("format_version");
    let (Some(format), Some(version)) = (format, version) else {
        return Err(Problem::NotFeed);
    };
    if format != FORMAT {
        return Err(Problem::NotFeed);
    }
    if version.as_u64() != Some(FORMAT_VERSION) {
        return Err(Problem::UnsupportedVersion(version.to_string()));
    }
    check_strings(&value, "")?;
    check_counts(&value)?;
    check_schema(&value)?;
    let feed: Feed =
        serde_json::from_value(value).map_err(|error| Problem::Schema(vec![error.to_string()]))?;
    let mut problems = feed_rules(&feed);
    if problems.is_empty() {
        Ok(feed)
    } else {
        problems.truncate(REPORTED_PROBLEMS);
        Err(Problem::Rules(problems))
    }
}

/// The feed's rules JSON Schema cannot state: ids listed once, versions distinct and newest
/// first, yank notices matching status, URLs that are URLs.
pub fn feed_rules(feed: &Feed) -> Vec<String> {
    let mut problems = Vec::new();
    problems.extend(url_problem("publisher.url", &feed.publisher.url));
    let mut ids = BTreeSet::new();
    for package in &feed.packages {
        let at = format!("package {}", package.id);
        if !ids.insert(&package.id) {
            problems.push(format!("{at} is listed twice"));
        }
        if let Some(project) = &package.project
            && !is_project_id(&project.0)
        {
            problems.push(format!(
                "{at}: project {project} is not a canonical lowercase UUID"
            ));
        }
        if let Some(repository) = &package.repository {
            problems.extend(url_problem(&format!("{at}: repository"), repository));
        }
        let mut versions: Vec<Version> = Vec::new();
        for entry in &package.versions {
            let where_ = format!("{at} {}", entry.version);
            match Version::parse(&entry.version, Scheme::Numeric) {
                Ok(version) => versions.push(version),
                Err(error) => problems.push(format!("{where_}: {error}")),
            }
            let has_notice = entry.yanked.is_some();
            if has_notice != (entry.status == VersionStatus::Yanked) {
                problems.push(format!(
                    "{where_}: `yanked` must be present exactly when status is yanked"
                ));
            }
            if let Some(replacement) = entry
                .yanked
                .as_ref()
                .and_then(|yanked| yanked.replacement.as_ref())
                && let Err(error) = Version::parse(replacement, Scheme::Numeric)
            {
                problems.push(format!("{where_}: replacement {error}"));
            }
            if entry.dependencies.contains_key(&package.id.0) {
                problems.push(format!("{where_}: depends on itself"));
            }
            let mut urls = BTreeSet::new();
            for source in &entry.artifact.sources {
                problems.extend(url_problem(&format!("{where_}: source"), &source.url));
                if !urls.insert(&source.url) {
                    problems.push(format!("{where_}: source {} is listed twice", source.url));
                }
            }
            for signature in &entry.artifact.signatures {
                problems.extend(url_problem(&format!("{where_}: signature"), &signature.url));
            }
        }
        for pair in versions.windows(2) {
            let (newer, older) = (&pair[0], &pair[1]);
            match newer.precedence(older) {
                Ordering::Greater => {}
                Ordering::Equal => problems.push(format!("{at}: version {older} is listed twice")),
                Ordering::Less => problems.push(format!(
                    "{at}: {older} is listed after {newer}, but versions are newest first"
                )),
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = include_str!("../tests/fixtures/jess/jess-packages.json");

    fn value() -> Value {
        serde_json::from_str(FEED).unwrap()
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn rule_problems(value: &Value) -> Vec<String> {
        match parse_feed(&bytes(value)) {
            Err(Problem::Rules(problems)) => problems,
            other => panic!("expected rule problems, got {other:?}"),
        }
    }

    #[test]
    fn the_fixture_feed_is_valid() {
        let feed = parse_feed(FEED.as_bytes()).unwrap();
        assert_eq!(feed.packages.len(), 2);
        let site = &feed.packages[0];
        assert_eq!(site.id.0, "org.dreamweave.site");
        assert_eq!(site.current().unwrap().version, "0.1.0");
        let words = &feed.packages[1];
        assert_eq!(words.versions[0].status, VersionStatus::Yanked);
        assert_eq!(
            words.current().unwrap().version,
            "1.2.0",
            "a yanked version is never current"
        );
        assert_eq!(words.versions[0].dependencies["org.example.style"], "^1.0");
    }

    #[test]
    fn the_envelope_is_read_first() {
        assert_eq!(parse_feed(b"{\"hello\": 1}").unwrap_err(), Problem::NotFeed);
        assert!(matches!(
            parse_feed(b"<html>").unwrap_err(),
            Problem::NotJson(_)
        ));
        let mut future = value();
        future["format_version"] = 2.into();
        future["anything"] = true.into();
        assert_eq!(
            parse_feed(&bytes(&future)).unwrap_err(),
            Problem::UnsupportedVersion("2".to_owned())
        );
        assert!(looks_like_feed(FEED.as_bytes()));
        assert!(!looks_like_feed(b"{\"schema_version\": \"2\"}"));
    }

    #[test]
    fn unknown_fields_and_other_artifact_kinds_are_schema_errors() {
        let mut feed = value();
        feed["packages"][0]["downloads"] = 100.into();
        assert!(matches!(
            parse_feed(&bytes(&feed)).unwrap_err(),
            Problem::Schema(_)
        ));
        let mut feed = value();
        feed["packages"][0]["versions"][0]["artifact"]["media_type"] = "application/zip".into();
        assert!(matches!(
            parse_feed(&bytes(&feed)).unwrap_err(),
            Problem::Schema(_)
        ));
        let mut feed = value();
        feed["packages"][0]["versions"][0]["content_digest"] = "778b9e0e".into();
        assert!(matches!(
            parse_feed(&bytes(&feed)).unwrap_err(),
            Problem::Schema(_)
        ));
        let mut feed = value();
        feed["packages"][0]["versions"][0]["version"] = "0.1".into();
        assert!(matches!(
            parse_feed(&bytes(&feed)).unwrap_err(),
            Problem::Schema(_)
        ));
        let mut feed = value();
        feed["packages"][1]["versions"][1]["dependencies"]["org.example.style"] = ">=1".into();
        assert!(matches!(
            parse_feed(&bytes(&feed)).unwrap_err(),
            Problem::Schema(_)
        ));
    }

    #[test]
    fn yank_notices_follow_status() {
        let mut feed = value();
        feed["packages"][1]["versions"][0]["status"] = "available".into();
        assert!(matches!(
            parse_feed(&bytes(&feed)).unwrap_err(),
            Problem::Schema(_)
        ));
    }

    #[test]
    fn versions_are_distinct_and_newest_first() {
        let mut feed = value();
        feed["packages"][1]["versions"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        let problems = rule_problems(&feed);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("newest first")),
            "{problems:?}"
        );
        let mut feed = value();
        feed["packages"][1]["versions"][1]["version"] = "1.3.0".into();
        let problems = rule_problems(&feed);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("listed twice")),
            "{problems:?}"
        );
    }

    #[test]
    fn ids_are_listed_once_and_do_not_depend_on_themselves() {
        let mut feed = value();
        let first = feed["packages"][0].clone();
        feed["packages"].as_array_mut().unwrap().push(first);
        let problems = rule_problems(&feed);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("listed twice"))
        );
        let mut feed = value();
        feed["packages"][0]["versions"][0]["dependencies"]["org.dreamweave.site"] = "0.1".into();
        let problems = rule_problems(&feed);
        assert!(problems.iter().any(|problem| problem.contains("itself")));
    }

    #[test]
    fn too_many_packages_is_a_policy_refusal() {
        let mut feed = value();
        let package = feed["packages"][0].clone();
        feed["packages"] = Value::Array(vec![package; policy::MAXIMUM_PACKAGES_PER_FEED + 1]);
        assert!(parse_feed(&bytes(&feed)).unwrap_err().is_policy_refusal());
    }
}
