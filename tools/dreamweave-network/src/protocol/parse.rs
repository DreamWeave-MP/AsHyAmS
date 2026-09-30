//! Untrusted bytes in, a validated protocol document out.
//!
//! The order matters. JSON first, then the envelope (`schema_version`, `document`) because the
//! protocol says a reader checks both before anything else, then this crawler's resource policy,
//! then the published JSON Schema, then the protocol rules a schema cannot express: version
//! grammar under the project's scheme, release order, channel heads, references between
//! components. A document that fails any step never reaches the network model.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::LazyLock,
};

use serde_json::Value;
use url::Url;

use super::{
    IndexEntry, Manifest, ProjectId, RelationshipKind, ReleaseStatus, SCHEMA_VERSION, SiteIndex,
};
use crate::{
    policy,
    version::{Constraint, Scheme, Version},
};

static INDEX_SCHEMA: LazyLock<jsonschema::Validator> =
    LazyLock::new(|| compile(include_str!("../../schemas/dreamweave-index-2.schema.json")));
static MANIFEST_SCHEMA: LazyLock<jsonschema::Validator> =
    LazyLock::new(|| compile(include_str!("../../schemas/modManifest-2.schema.json")));

fn compile(text: &str) -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(text).expect("a vendored schema is JSON");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("a vendored schema compiles")
}

/// How many schema or rule violations one report lists before it stops counting.
const REPORTED_PROBLEMS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NotJson(String),
    /// JSON without `schema_version` and `document`: not a DreamWeave document at all.
    NotDreamWeave,
    /// A `schema_version` this index cannot read. The protocol says to stop, not to guess.
    UnsupportedVersion(String),
    WrongDocument {
        expected: &'static str,
        found: String,
    },
    Schema(Vec<String>),
    Protocol(Vec<String>),
    /// Refused by this crawler's resource policy. Not a protocol error.
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
            Self::NotDreamWeave => formatter.write_str(
                "JSON without schema_version and document, so not a DreamWeave document",
            ),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "schema_version {version:?}: a protocol version this index does not read (it reads \"{SCHEMA_VERSION}\")"
            ),
            Self::WrongDocument { expected, found } => write!(
                formatter,
                "a {found:?} document where a {expected:?} document belongs"
            ),
            Self::Schema(errors) => list(formatter, "does not match its JSON Schema", errors),
            Self::Protocol(errors) => list(formatter, "breaks the protocol", errors),
            Self::Policy(reason) => write!(formatter, "refused by crawler policy: {reason}"),
        }
    }
}

impl std::error::Error for Problem {}

/// What a JSON body says it is, before anything else is believed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub schema_version: String,
    pub document: String,
}

/// Reads the envelope. `None` for anything that is not a JSON object carrying both fields.
pub fn envelope(bytes: &[u8]) -> Option<Envelope> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    envelope_of(&value)
}

fn envelope_of(value: &Value) -> Option<Envelope> {
    Some(Envelope {
        schema_version: value.get("schema_version")?.as_str()?.to_owned(),
        document: value.get("document")?.as_str()?.to_owned(),
    })
}

fn read(bytes: &[u8], expected: &'static str) -> Result<Value, Problem> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| Problem::NotJson(error.to_string()))?;
    let found = envelope_of(&value).ok_or(Problem::NotDreamWeave)?;
    if found.schema_version != SCHEMA_VERSION {
        return Err(Problem::UnsupportedVersion(found.schema_version));
    }
    if found.document != expected {
        return Err(Problem::WrongDocument {
            expected,
            found: found.document,
        });
    }
    check_strings(&value, "")?;
    Ok(value)
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

fn check_schema(validator: &jsonschema::Validator, value: &Value) -> Result<(), Problem> {
    let errors: Vec<String> = validator
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

fn typed<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, Problem> {
    serde_json::from_value(value).map_err(|error| Problem::Schema(vec![error.to_string()]))
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

// Site index ----------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IndexReport {
    pub index: SiteIndex,
    /// Ids listed more than once. Which entry speaks for the project is ambiguous, so neither
    /// does: the crawler skips these and says why.
    pub duplicate_ids: BTreeSet<ProjectId>,
}

impl IndexReport {
    pub fn entries(&self) -> impl Iterator<Item = &IndexEntry> {
        self.index
            .projects
            .iter()
            .filter(|entry| !self.duplicate_ids.contains(&entry.id))
    }
}

pub fn parse_index(bytes: &[u8]) -> Result<IndexReport, Problem> {
    let value = read(bytes, "index")?;
    let listed = value
        .get("projects")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if listed > policy::MAXIMUM_PROJECTS_PER_SITE {
        return Err(Problem::Policy(format!(
            "the site lists {listed} projects; this crawler reads at most {}",
            policy::MAXIMUM_PROJECTS_PER_SITE
        )));
    }
    check_schema(&INDEX_SCHEMA, &value)?;
    let index: SiteIndex = typed(value)?;

    let mut problems = Vec::new();
    problems.extend(url_problem("site.url", &index.site.url));
    let mut seen = BTreeSet::new();
    let mut duplicate_ids = BTreeSet::new();
    for entry in &index.projects {
        problems.extend(url_problem(&format!("{}: page", entry.id), &entry.page));
        problems.extend(url_problem(
            &format!("{}: manifest", entry.id),
            &entry.manifest,
        ));
        if !seen.insert(entry.id.clone()) {
            duplicate_ids.insert(entry.id.clone());
        }
    }
    if !problems.is_empty() {
        problems.truncate(REPORTED_PROBLEMS);
        return Err(Problem::Protocol(problems));
    }
    Ok(IndexReport {
        index,
        duplicate_ids,
    })
}

// Project manifest ----------------------------------------------------------------------------

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, Problem> {
    let value = read(bytes, "project")?;
    check_manifest_counts(&value)?;
    check_schema(&MANIFEST_SCHEMA, &value)?;
    let manifest: Manifest = typed(value)?;
    let mut problems = manifest_rules(&manifest);
    if problems.is_empty() {
        Ok(manifest)
    } else {
        problems.truncate(REPORTED_PROBLEMS);
        Err(Problem::Protocol(problems))
    }
}

fn check_manifest_counts(value: &Value) -> Result<(), Problem> {
    let count = |value: Option<&Value>| value.and_then(Value::as_array).map_or(0, Vec::len);
    let refuse = |what: &str, found: usize, limit: usize| {
        Err(Problem::Policy(format!(
            "{found} {what}; this crawler reads at most {limit}"
        )))
    };
    let media = count(value.pointer("/project/media"));
    if media > policy::MAXIMUM_MEDIA_PER_PROJECT {
        return refuse("media items", media, policy::MAXIMUM_MEDIA_PER_PROJECT);
    }
    let releases = value.get("releases").and_then(Value::as_array);
    let release_count = releases.map_or(0, Vec::len);
    if release_count > policy::MAXIMUM_RELEASES_PER_PROJECT {
        return refuse(
            "releases",
            release_count,
            policy::MAXIMUM_RELEASES_PER_PROJECT,
        );
    }
    for release in releases.into_iter().flatten() {
        let limits = [
            (
                "artifacts in one release",
                count(release.get("artifacts")),
                policy::MAXIMUM_ARTIFACTS_PER_RELEASE,
            ),
            (
                "relationships in one release",
                count(release.get("relationships")),
                policy::MAXIMUM_RELATIONSHIPS_PER_RELEASE,
            ),
            (
                "components in one release",
                count(release.get("components")),
                policy::MAXIMUM_COMPONENTS_PER_RELEASE,
            ),
        ];
        for (what, found, limit) in limits {
            if found > limit {
                return refuse(what, found, limit);
            }
        }
        for artifact in release
            .get("artifacts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let sources = count(artifact.get("sources"));
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

/// The protocol's rules that JSON Schema cannot state.
pub fn manifest_rules(manifest: &Manifest) -> Vec<String> {
    let scheme = manifest.scheme();
    let mut problems = Vec::new();

    for (field, url) in &manifest.project.links {
        problems.extend(url_problem(&format!("project.links.{field}"), url));
    }

    let mut versions: Vec<(usize, Version)> = Vec::new();
    for (position, release) in manifest.releases.iter().enumerate() {
        match Version::parse(&release.version, scheme) {
            Ok(version) => versions.push((position, version)),
            Err(error) => problems.push(format!("releases[{position}]: {error}")),
        }
    }
    for pair in versions.windows(2) {
        let ((_, newer), (position, older)) = (&pair[0], &pair[1]);
        match newer.precedence(older) {
            Ordering::Greater => {}
            Ordering::Equal => problems.push(format!(
                "releases[{position}]: {older} has the same precedence as {newer}; two releases of one project never may"
            )),
            Ordering::Less => problems.push(format!(
                "releases[{position}]: {older} is listed after {newer}, but releases are newest first by precedence"
            )),
        }
    }

    for release in &manifest.releases {
        release_rules(
            release,
            scheme,
            &format!("release {}", release.version),
            &mut problems,
        );
    }

    let recomputed = manifest.recomputed_heads();
    let declared: BTreeMap<&String, &String> = manifest
        .channels
        .iter()
        .map(|(channel, head)| (channel, &head.version))
        .collect();
    let channels: BTreeSet<&String> = declared.keys().copied().chain(recomputed.keys()).collect();
    for channel in channels {
        let declared = declared.get(channel).copied();
        let expected = recomputed.get(channel);
        let agree = match (declared, expected) {
            (Some(declared), Some(expected)) => {
                match (
                    Version::parse(declared, scheme),
                    Version::parse(expected, scheme),
                ) {
                    (Ok(declared), Ok(expected)) => {
                        declared.precedence(&expected) == Ordering::Equal
                    }
                    _ => false,
                }
            }
            (None, None) => true,
            _ => false,
        };
        if !agree {
            problems.push(format!(
                "channels.{channel} says {}, but its highest available release is {}",
                declared.map_or("nothing", String::as_str),
                expected.map_or("nothing", String::as_str)
            ));
        }
    }
    problems
}

fn release_rules(release: &super::Release, scheme: Scheme, at: &str, problems: &mut Vec<String>) {
    let notice_matches =
        |present: bool, status: ReleaseStatus| present == (release.status == status);
    if !notice_matches(release.yanked.is_some(), ReleaseStatus::Yanked) {
        problems.push(format!(
            "{at}: `yanked` must be present exactly when status is yanked"
        ));
    }
    if !notice_matches(release.deprecated.is_some(), ReleaseStatus::Deprecated) {
        problems.push(format!(
            "{at}: `deprecated` must be present exactly when status is deprecated"
        ));
    }
    for notice in [&release.yanked, &release.deprecated].into_iter().flatten() {
        if let Some(replacement) = &notice.replacement
            && let Err(error) = Version::parse(replacement, scheme)
        {
            problems.push(format!("{at}: replacement {error}"));
        }
    }

    for (runtime, constraint) in &release.runtimes {
        if let Err(error) = Constraint::parse(constraint, Scheme::Numeric) {
            problems.push(format!("{at}: runtimes.{runtime}: {error}"));
        }
    }

    for relationship in &release.relationships {
        let kind = relationship.kind.token();
        if relationship.project.is_some() && relationship.capability.is_some() {
            problems.push(format!(
                "{at}: a {kind} relationship names both a project and a capability"
            ));
        }
        if relationship.capability.is_some()
            && matches!(
                relationship.kind,
                RelationshipKind::Compatible | RelationshipKind::Replaces
            )
        {
            problems.push(format!(
                "{at}: a {kind} relationship cannot name a capability"
            ));
        }
        if let Some(constraint) = &relationship.version {
            if relationship.project.is_none() {
                problems.push(format!(
                    "{at}: a version constraint needs a project to apply to"
                ));
            }
            if let Err(error) = Constraint::check_grammar(constraint) {
                problems.push(format!("{at}: {error}"));
            }
        }
        if let Some(url) = &relationship.url {
            problems.extend(url_problem(&format!("{at}: relationship url"), url));
        }
    }

    component_rules(release, at, problems);
}

/// Components, groups, artifacts and the OpenMW extension refer to each other by id within one
/// release. Every reference has to land.
fn component_rules(release: &super::Release, at: &str, problems: &mut Vec<String>) {
    let mut component_ids = BTreeSet::new();
    for component in &release.components {
        if !component_ids.insert(component.id.as_str()) {
            problems.push(format!("{at}: component {} is listed twice", component.id));
        }
    }
    let group_ids: BTreeSet<&str> = release
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    if group_ids.len() != release.groups.len() {
        problems.push(format!("{at}: a group id is listed twice"));
    }
    for component in &release.components {
        if let Some(group) = &component.group
            && !group_ids.contains(group.as_str())
        {
            problems.push(format!(
                "{at}: component {} names group {group}, which the release does not define",
                component.id
            ));
        }
        for reference in component.requires.iter().chain(&component.conflicts) {
            if !component_ids.contains(reference.as_str()) {
                problems.push(format!(
                    "{at}: component {} refers to component {reference}, which the release does not define",
                    component.id
                ));
            }
        }
    }

    let mut artifact_ids = BTreeSet::new();
    let mut program_platforms = BTreeSet::new();
    for artifact in &release.artifacts {
        if !artifact_ids.insert(artifact.id.as_str()) {
            problems.push(format!("{at}: artifact {} is listed twice", artifact.id));
        }
        // A program release has one binary artifact per platform and variant, and its
        // `platforms` lists every desktop one. Android and handheld builds are only on artifacts.
        if let (true, Some(platform)) = (artifact.is_program(), &artifact.platform) {
            let label = platform.key();
            if !program_platforms.insert(label.clone()) {
                problems.push(format!(
                    "{at}: two binary artifacts are built for {label}; a release has one per platform"
                ));
            }
            if platform.is_desktop() && !release.platforms.contains(platform) {
                problems.push(format!(
                    "{at}: binary artifact {} is built for {label}, which the release's platforms do not list",
                    artifact.id
                ));
            }
        }
    }

    if let Some(openmw) = release.openmw() {
        if let Some(lua_api) = &openmw.lua_api
            && let Err(error) = Constraint::parse(lua_api, Scheme::Numeric)
        {
            problems.push(format!("{at}: extensions.openmw.lua_api: {error}"));
        }
        for component in openmw.components.keys() {
            if !component_ids.contains(component.as_str()) {
                problems.push(format!(
                    "{at}: extensions.openmw describes component {component}, which the release does not define"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = include_str!("../../tests/fixtures/mod-template/dreamweave.json");
    const CANDLELIGHT: &str = include_str!("../../tests/fixtures/mod-template/candlelight.json");
    const TALLOW: &str = include_str!("../../tests/fixtures/mod-template/tallow.json");

    fn value(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn protocol_problems(value: &Value) -> Vec<String> {
        match parse_manifest(&bytes(value)) {
            Err(Problem::Protocol(problems)) => problems,
            other => panic!("expected protocol problems, got {other:?}"),
        }
    }

    #[test]
    fn the_mod_templates_own_documents_are_valid() {
        let report = parse_index(INDEX.as_bytes()).unwrap();
        assert_eq!(report.index.projects.len(), 2);
        assert!(report.duplicate_ids.is_empty());
        let candlelight = parse_manifest(CANDLELIGHT.as_bytes()).unwrap();
        assert_eq!(candlelight.project.name, "Candlelight");
        assert_eq!(candlelight.channels["stable"].version, "1.1.0");
        let openmw = candlelight.releases[0].openmw().unwrap();
        assert_eq!(openmw.lua_api.as_deref(), Some(">=60"));
        parse_manifest(TALLOW.as_bytes()).unwrap();
    }

    #[test]
    fn envelopes_are_read_before_anything_else() {
        assert_eq!(
            parse_manifest(b"{\"hello\": 1}").unwrap_err(),
            Problem::NotDreamWeave
        );
        assert!(matches!(
            parse_manifest(b"<html></html>").unwrap_err(),
            Problem::NotJson(_)
        ));
        assert_eq!(
            parse_manifest(INDEX.as_bytes()).unwrap_err(),
            Problem::WrongDocument {
                expected: "project",
                found: "index".to_owned()
            }
        );
        let mut future = value(INDEX);
        future["schema_version"] = "3".into();
        future["some_new_field"] = true.into();
        assert_eq!(
            parse_index(&bytes(&future)).unwrap_err(),
            Problem::UnsupportedVersion("3".to_owned())
        );
    }

    #[test]
    fn unknown_core_fields_are_errors() {
        let mut manifest = value(CANDLELIGHT);
        manifest["project"]["popularity"] = 11.into();
        assert!(matches!(
            parse_manifest(&bytes(&manifest)).unwrap_err(),
            Problem::Schema(_)
        ));
    }

    #[test]
    fn index_entries_are_checked() {
        let mut index = value(INDEX);
        index["projects"][0]["id"] = "4D0C9F6E-2B1A-4C8E-9F3A-7E5D1B2C6A90".into();
        assert!(matches!(
            parse_index(&bytes(&index)).unwrap_err(),
            Problem::Schema(_)
        ));

        let mut index = value(INDEX);
        index["projects"][0]["manifest_sha256"] = "not-a-digest".into();
        assert!(matches!(
            parse_index(&bytes(&index)).unwrap_err(),
            Problem::Schema(_)
        ));

        let mut index = value(INDEX);
        index["projects"][0]["manifest"] = "ftp://example.org/x.json".into();
        assert!(parse_index(&bytes(&index)).is_err());
    }

    #[test]
    fn a_project_listed_twice_is_set_aside() {
        let mut index = value(INDEX);
        let first = index["projects"][0].clone();
        index["projects"].as_array_mut().unwrap().push(first);
        let report = parse_index(&bytes(&index)).unwrap();
        assert_eq!(report.duplicate_ids.len(), 1);
        assert_eq!(report.entries().count(), 1);
    }

    #[test]
    fn too_many_projects_is_a_policy_refusal_not_a_protocol_error() {
        let mut index = value(INDEX);
        let entry = index["projects"][0].clone();
        index["projects"] = Value::Array(vec![entry; policy::MAXIMUM_PROJECTS_PER_SITE + 1]);
        let problem = parse_index(&bytes(&index)).unwrap_err();
        assert!(problem.is_policy_refusal(), "{problem}");
    }

    #[test]
    fn oversized_strings_are_a_policy_refusal() {
        let mut manifest = value(CANDLELIGHT);
        manifest["project"]["summary"] = "a".repeat(policy::MAXIMUM_STRING_BYTES + 1).into();
        assert!(
            parse_manifest(&bytes(&manifest))
                .unwrap_err()
                .is_policy_refusal()
        );
    }

    #[test]
    fn channel_heads_must_match_the_releases() {
        let mut manifest = value(CANDLELIGHT);
        manifest["channels"]["stable"]["version"] = "1.0.0".into();
        let problems = protocol_problems(&manifest);
        assert!(problems[0].contains("channels.stable"), "{problems:?}");
    }

    #[test]
    fn releases_are_newest_first_and_distinct() {
        let mut manifest = value(CANDLELIGHT);
        manifest["releases"].as_array_mut().unwrap().swap(1, 2);
        let problems = protocol_problems(&manifest);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("newest first")),
            "{problems:?}"
        );
    }

    #[test]
    fn versions_follow_the_projects_scheme() {
        let mut manifest = value(TALLOW);
        manifest["releases"][1]["version"] = "1.00".into();
        manifest["channels"]["stable"]["version"] = "1.00".into();
        let problems = protocol_problems(&manifest);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("leading zero")),
            "{problems:?}"
        );
    }

    #[test]
    fn relationship_rules() {
        let mut manifest = value(CANDLELIGHT);
        manifest["releases"][1]["relationships"][0]["capability"] = "dreamweave:scheduling".into();
        manifest["releases"][1]["relationships"][1]["version"] = ">=1".into();
        let problems = protocol_problems(&manifest);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("both a project and a capability"))
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("needs a project"))
        );
    }

    #[test]
    fn invalid_version_constraints_are_rejected() {
        // The schema allows leading zeros, since decimal versions need them; runtime
        // constraints are numeric, so the rules refuse this one.
        let mut manifest = value(CANDLELIGHT);
        manifest["releases"][1]["runtimes"]["openmw"] = ">=0.049".into();
        let problems = protocol_problems(&manifest);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("runtimes.openmw"))
        );

        let mut manifest = value(CANDLELIGHT);
        manifest["releases"][1]["relationships"][0]["version"] = "^1.0".into();
        assert!(matches!(
            parse_manifest(&bytes(&manifest)).unwrap_err(),
            Problem::Schema(_)
        ));
    }

    #[test]
    fn yank_notices_follow_status() {
        let mut manifest = value(CANDLELIGHT);
        manifest["releases"][2]["status"] = "yanked".into();
        let problems = protocol_problems(&manifest);
        assert!(problems.iter().any(|problem| problem.contains("`yanked`")));

        manifest["releases"][2]["yanked"] =
            serde_json::json!({ "reason": "Broke saves.", "replacement": "1.1.0" });
        parse_manifest(&bytes(&manifest)).unwrap();
    }

    #[test]
    fn unknown_critical_extensions_are_carried_not_refused() {
        let mut manifest = value(TALLOW);
        manifest["releases"][1]["extensions"]["org.example.launcher"] =
            serde_json::json!({ "profile": "default" });
        manifest["releases"][1]["critical_extensions"]
            .as_array_mut()
            .unwrap()
            .push("org.example.launcher".into());
        let parsed = parse_manifest(&bytes(&manifest)).unwrap();
        assert_eq!(
            parsed.releases[1].extensions["org.example.launcher"]["profile"],
            "default"
        );
    }

    #[test]
    fn component_references_must_resolve() {
        let mut manifest = value(CANDLELIGHT);
        manifest["releases"][1]["components"][1]["requires"] =
            serde_json::json!(["no-such-component"]);
        let problems = protocol_problems(&manifest);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("no-such-component"))
        );
    }

    const PROGRAM: &str = include_str!("../../tests/fixtures/programs/lantern-forge.json");

    #[test]
    fn programs_are_one_binary_artifact_per_listed_platform() {
        let program = parse_manifest(PROGRAM.as_bytes()).unwrap();
        let artifacts = &program.releases[1].artifacts;
        assert!(artifacts.iter().all(crate::protocol::Artifact::is_program));
        assert_eq!(artifacts[0].platform.as_ref().unwrap().os, "linux");

        let mut missing = value(PROGRAM);
        missing["releases"][1]["artifacts"][0]
            .as_object_mut()
            .unwrap()
            .remove("platform");
        assert!(matches!(
            parse_manifest(&bytes(&missing)).unwrap_err(),
            Problem::Schema(_)
        ));

        let mut unlisted = value(PROGRAM);
        unlisted["releases"][1]["platforms"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        let problems = protocol_problems(&unlisted);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("do not list")),
            "{problems:?}"
        );

        let mut twice = value(PROGRAM);
        twice["releases"][1]["artifacts"][1]["platform"] =
            serde_json::json!({ "os": "linux", "arch": "x86_64" });
        let problems = protocol_problems(&twice);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("one per platform")),
            "{problems:?}"
        );
    }

    #[test]
    fn handheld_and_android_builds_are_listed_only_on_their_artifacts() {
        let mut program = value(PROGRAM);
        let artifacts = program["releases"][1]["artifacts"].as_array_mut().unwrap();
        let mut android = artifacts[0].clone();
        android["id"] = "android-arm64".into();
        android["platform"] = serde_json::json!({ "os": "android", "arch": "aarch64" });
        let mut portmaster = artifacts[0].clone();
        portmaster["id"] = "linux-arm64-portmaster".into();
        portmaster["platform"] =
            serde_json::json!({ "os": "linux", "arch": "aarch64", "variant": "portmaster" });
        let mut muos = portmaster.clone();
        muos["id"] = "linux-arm64-muos".into();
        muos["platform"]["variant"] = "muos".into();
        artifacts.extend([android, portmaster, muos]);
        let parsed = parse_manifest(&bytes(&program)).unwrap();
        let keys: Vec<String> = parsed.releases[1]
            .artifacts
            .iter()
            .filter_map(|artifact| {
                artifact
                    .platform
                    .as_ref()
                    .map(crate::protocol::Platform::key)
            })
            .collect();
        assert!(
            keys.contains(&"linux/aarch64+portmaster".to_owned()),
            "{keys:?}"
        );
        assert!(keys.contains(&"linux/x86_64".to_owned()), "{keys:?}");

        let mut twice = program.clone();
        twice["releases"][1]["artifacts"][4]["platform"]["variant"] = "portmaster".into();
        let problems = protocol_problems(&twice);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("one per platform")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_library_release_is_one_crate_artifact() {
        let mut library = value(PROGRAM);
        let release = &mut library["releases"][1];
        release["platforms"] = serde_json::json!([]);
        release["artifacts"] = serde_json::json!([{
            "id": "crate",
            "format": "crate",
            "filename": "lantern-forge-1.0.0.crate",
            "media_type": "application/gzip",
            "size": 1024,
            "digests": { "sha256": "0".repeat(64) },
            "sources": [{ "url": "https://static.crates.io/crates/lantern-forge/lantern-forge-1.0.0.crate", "kind": "publisher" }],
            "signatures": []
        }]);
        let parsed = parse_manifest(&bytes(&library)).unwrap();
        let artifact = &parsed.releases[1].artifacts[0];
        assert!(artifact.is_crate() && !artifact.is_program());
    }

    #[test]
    fn envelope_identifies_documents_for_discovery() {
        assert_eq!(
            envelope(INDEX.as_bytes()),
            Some(Envelope {
                schema_version: "2".to_owned(),
                document: "index".to_owned()
            })
        );
        assert_eq!(envelope(b"<!DOCTYPE html>"), None);
    }
}
