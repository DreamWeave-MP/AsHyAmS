//! What this index has observed, as plain files in `state/`, a checkout of the `network-state`
//! branch.
//!
//! ```text
//! state/
//!   network.json                            when the last crawl ran, and with what
//!   sources/<source>.json                   one per enrolled URL: where it led, last attempt
//!   origins/<origin>/origin.json            one per site index: health, validators, issues
//!   origins/<origin>/dreamweave.json        that index's last good bytes, exactly as served
//!   claims/<project>/<origin>/claim.json    one per (project id, site): health, digests, times
//!   claims/<project>/<origin>/manifest.json the claim's last good manifest, exactly as served
//!   events/<event>.json                     one per observed transition, never rewritten
//!   media/<sha256>.<ext>                    cached card images, by digest
//! ```
//!
//! Manifests are kept byte for byte so that `sha256sum manifest.json` still prints the digest the
//! claim records. Git holds every previous version, which is the only history this index needs:
//! the state branch is the operations database, and `git log` is its query language.
//!
//! Losing this directory loses events and "first observed" dates. It does not lose the network:
//! a refresh from nothing recovers every claim its publishers still publish.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{
    diff::Change,
    fetch::Validators,
    protocol::{IndexEntry, ProjectId},
};

pub const STATE_FORMAT: u32 = 1;
pub const DEFAULT_STATE_DIRECTORY: &str = "state";

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        text.push(char::from(b"0123456789abcdef"[usize::from(byte & 0xf)]));
    }
    text
}

/// A file-name-safe id a person can still read: host and path, then eight hex digits of the
/// full URL's digest so two URLs that slug alike stay apart.
pub fn readable_id(readable: &Url, hashed: &str) -> String {
    let mut slug = String::new();
    let text = format!(
        "{}{}",
        readable.host_str().unwrap_or_default(),
        readable.path()
    );
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug = slug.trim_matches('-').to_owned();
    if slug.len() > 56 {
        slug.truncate(56);
        slug = slug.trim_end_matches('-').to_owned();
    }
    format!("{slug}-{}", &sha256_hex(hashed.as_bytes())[..8])
}

pub fn source_id(source: &Url) -> String {
    readable_id(source, source.as_str())
}

/// An origin is a site: the directory its index lives in, which on GitHub Pages is a path below
/// the host, not the host. Two projects on one site share an origin; two sites on one host do not.
pub fn origin_id(index_url: &Url) -> String {
    let directory = index_url.join("./").unwrap_or_else(|_| index_url.clone());
    readable_id(&directory, index_url.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ClaimKey {
    pub project: ProjectId,
    pub origin: String,
}

// Records -------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkRecord {
    pub format: u32,
    pub crawler: String,
    /// When the last refresh started. Everything the site says is "as of" this.
    pub observed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceHealth {
    Resolved,
    Unreachable,
    Refused,
    NoIndex,
    BrokenLink,
    UnsupportedVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub url: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub url: String,
    /// The origin this source led to the last time it led anywhere.
    pub origin: Option<String>,
    pub health: SourceHealth,
    pub problem: Option<String>,
    pub method: Option<String>,
    /// What discovery tried on the last attempt, request by request.
    pub trail: Vec<AttemptRecord>,
    pub last_attempt: String,
    pub last_success: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OriginHealth {
    Healthy,
    Unreachable,
    Refused,
    NoIndex,
    InvalidIndex,
    UnsupportedVersion,
}

impl OriginHealth {
    pub fn token(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unreachable => "unreachable",
            Self::Refused => "refused",
            Self::NoIndex => "no-index",
            Self::InvalidIndex => "invalid-index",
            Self::UnsupportedVersion => "unsupported-version",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unreachable => "unreachable",
            Self::Refused => "refused by crawler policy",
            Self::NoIndex => "no site index found",
            Self::InvalidIndex => "invalid site index",
            Self::UnsupportedVersion => "newer protocol",
        }
    }
}

/// Evidence that a site replaced another one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Move {
    pub origin: String,
    pub index_url: String,
    pub observed_at: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginRecord {
    pub id: String,
    pub index_url: String,
    pub site_name: Option<String>,
    pub site_url: Option<String>,
    pub generator: Option<String>,
    pub sources: Vec<String>,
    /// URLs that redirected on the way to the index, on the last successful discovery.
    pub redirects: Vec<String>,
    pub validators: Validators,
    pub index_sha256: Option<String>,
    pub health: OriginHealth,
    pub problem: Option<String>,
    /// Publication issues that did not stop the crawl: a project listed twice, say.
    pub issues: Vec<String>,
    pub previous: Vec<Move>,
    pub first_observed: String,
    pub last_attempt: String,
    pub last_success: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimHealth {
    /// The manifest this index holds is the one the site advertises right now.
    Current,
    /// The site itself could not be read on the last crawl.
    OriginUnavailable,
    /// The site index was read, the manifest could not be.
    ManifestUnreachable,
    /// The site publishes a manifest that fails the protocol, or an index entry that is ambiguous.
    Invalid,
    /// The manifest's bytes did not match the digest the site index advertises, even after
    /// waiting for the deployment to settle.
    Inconsistent,
    /// This crawler's resource policy refused the manifest.
    Refused,
    /// The site's index no longer lists the project.
    Withdrawn,
}

impl ClaimHealth {
    pub fn token(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::OriginUnavailable => "origin-unavailable",
            Self::ManifestUnreachable => "manifest-unreachable",
            Self::Invalid => "invalid",
            Self::Inconsistent => "inconsistent",
            Self::Refused => "refused",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::OriginUnavailable => "origin unavailable",
            Self::ManifestUnreachable => "manifest unreachable",
            Self::Invalid => "invalid publication",
            Self::Inconsistent => "inconsistent deployment",
            Self::Refused => "refused by crawler policy",
            Self::Withdrawn => "withdrawn",
        }
    }
}

/// A manifest this index read and refused, so the same bytes are not fetched again every crawl.
/// A new crawler version re-reads it, since the rules that refused it may have changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rejection {
    pub sha256: String,
    pub crawler: String,
    pub problem: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRecord {
    /// The publisher's URL, kept so the original is always one click away.
    pub url: String,
    pub alt: String,
    /// `media/<sha256>.<ext>` when cached.
    pub file: Option<String>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRecord {
    pub project: ProjectId,
    pub origin: String,
    /// The site index's entry for the project, as last read.
    pub entry: IndexEntry,
    pub advertised_sha256: String,
    /// The digest of `manifest.json` beside this record. `None` until a manifest was accepted.
    pub ingested_sha256: Option<String>,
    pub rejected: Option<Rejection>,
    pub health: ClaimHealth,
    pub problem: Option<String>,
    pub media: Option<MediaRecord>,
    pub first_observed: String,
    pub last_changed: Option<String>,
    pub last_attempt: String,
    pub last_success: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    /// The first accepted manifest of a claim.
    Observed,
    /// A new manifest replaced the one this index held.
    Changed,
    /// The site's index stopped listing the project.
    Withdrawn,
    /// A withdrawn project is listed again.
    Restored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: String,
    pub kind: EventKind,
    pub project: ProjectId,
    pub origin: String,
    /// The project's name when the event happened.
    pub name: String,
    /// When this index saw it. Not when anybody published anything.
    pub observed_at: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub changes: Vec<Change>,
    /// Set when the claim arrived from another origin by a move this index recognized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
}

/// The same transition always gets the same id, so a rerun over the same observations can never
/// produce a second copy of an event.
pub fn event_id(
    kind: EventKind,
    project: &ProjectId,
    origin: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> String {
    let kind = serde_json::to_value(kind).expect("an event kind serializes");
    let text = format!(
        "dreamweave-network event 1\n{}\n{project}\n{origin}\n{}\n{}",
        kind.as_str().unwrap_or_default(),
        before.unwrap_or_default(),
        after.unwrap_or_default()
    );
    sha256_hex(text.as_bytes())[..20].to_owned()
}

// The state directory -------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct State {
    pub network: Option<NetworkRecord>,
    pub sources: BTreeMap<String, SourceRecord>,
    pub origins: BTreeMap<String, OriginRecord>,
    /// Last good site index bytes per origin.
    pub indexes: BTreeMap<String, Vec<u8>>,
    pub claims: BTreeMap<ClaimKey, ClaimRecord>,
    /// Last good manifest bytes per claim.
    pub manifests: BTreeMap<ClaimKey, Vec<u8>>,
    pub events: BTreeMap<String, Event>,
    /// Cached images added by this run, by file name under `media/`.
    pub new_media: BTreeMap<String, Vec<u8>>,
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

pub fn to_json<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("state records serialize");
    bytes.push(b'\n');
    bytes
}

fn sorted_entries(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("list {}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    Ok(entries)
}

impl State {
    /// Reads a state directory. A directory that does not exist is an empty network, which is
    /// how the first crawl, and a recovery from nothing, begin.
    pub fn load(directory: &Path) -> Result<Self> {
        let mut state = Self::default();
        if !directory.exists() {
            return Ok(state);
        }
        let network = directory.join("network.json");
        if network.exists() {
            let record: NetworkRecord = read_json(&network)?;
            if record.format != STATE_FORMAT {
                bail!(
                    "{} is state format {}; this crawler reads format {STATE_FORMAT}",
                    network.display(),
                    record.format
                );
            }
            state.network = Some(record);
        }
        for path in sorted_entries(&directory.join("sources"))? {
            let record: SourceRecord = read_json(&path)?;
            state.sources.insert(file_stem(&path)?, record);
        }
        for path in sorted_entries(&directory.join("origins"))? {
            let record: OriginRecord = read_json(&path.join("origin.json"))?;
            if record.id != file_name(&path)? {
                bail!("{} holds origin {}", path.display(), record.id);
            }
            let index = path.join("dreamweave.json");
            if index.exists() {
                state.indexes.insert(record.id.clone(), fs::read(&index)?);
            }
            state.origins.insert(record.id.clone(), record);
        }
        for project_directory in sorted_entries(&directory.join("claims"))? {
            for path in sorted_entries(&project_directory)? {
                let record: ClaimRecord = read_json(&path.join("claim.json"))?;
                let key = ClaimKey {
                    project: record.project.clone(),
                    origin: record.origin.clone(),
                };
                if key.project.0 != file_name(&project_directory)?
                    || key.origin != file_name(&path)?
                {
                    bail!(
                        "{} holds the claim of {} at {}",
                        path.display(),
                        key.project,
                        key.origin
                    );
                }
                let manifest = path.join("manifest.json");
                if manifest.exists() {
                    state.manifests.insert(key.clone(), fs::read(&manifest)?);
                }
                state.claims.insert(key, record);
            }
        }
        for path in sorted_entries(&directory.join("events"))? {
            let event: Event = read_json(&path)?;
            if event.id != file_stem(&path)? {
                bail!("{} holds event {}", path.display(), event.id);
            }
            state.events.insert(event.id.clone(), event);
        }
        Ok(state)
    }

    /// Writes the state so that the directory holds exactly what `self` says, and nothing a
    /// removed source left behind. Events are append-only and never removed. Files whose bytes
    /// did not change are not rewritten.
    pub fn save(&self, directory: &Path) -> Result<()> {
        let mut files: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
        if let Some(network) = &self.network {
            files.insert(PathBuf::from("network.json"), to_json(network));
        }
        files.insert(PathBuf::from("README.md"), STATE_README.as_bytes().to_vec());
        for (id, record) in &self.sources {
            files.insert(PathBuf::from(format!("sources/{id}.json")), to_json(record));
        }
        for (id, record) in &self.origins {
            files.insert(
                PathBuf::from(format!("origins/{id}/origin.json")),
                to_json(record),
            );
        }
        for (id, bytes) in &self.indexes {
            files.insert(
                PathBuf::from(format!("origins/{id}/dreamweave.json")),
                bytes.clone(),
            );
        }
        for (key, record) in &self.claims {
            files.insert(
                PathBuf::from(format!("claims/{}/{}/claim.json", key.project, key.origin)),
                to_json(record),
            );
        }
        for (key, bytes) in &self.manifests {
            files.insert(
                PathBuf::from(format!(
                    "claims/{}/{}/manifest.json",
                    key.project, key.origin
                )),
                bytes.clone(),
            );
        }
        for (id, event) in &self.events {
            files.insert(PathBuf::from(format!("events/{id}.json")), to_json(event));
        }
        let media: BTreeSet<String> = self
            .claims
            .values()
            .filter_map(|claim| claim.media.as_ref()?.file.clone())
            .collect();
        for (name, bytes) in &self.new_media {
            files.insert(PathBuf::from(name), bytes.clone());
        }

        for (relative, bytes) in &files {
            let path = directory.join(relative);
            if fs::read(&path).is_ok_and(|existing| &existing == bytes) {
                continue;
            }
            fs::create_dir_all(path.parent().context("state files live in a directory")?)?;
            fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
        }
        for managed in ["sources", "origins", "claims"] {
            prune(directory, &directory.join(managed), &files)?;
        }
        for path in sorted_entries(&directory.join("media"))? {
            let name = format!("media/{}", file_name(&path)?);
            if !media.contains(&name) {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    pub fn manifest_bytes(&self, key: &ClaimKey) -> Option<&[u8]> {
        self.manifests.get(key).map(Vec::as_slice)
    }
}

fn prune(root: &Path, directory: &Path, keep: &BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    for path in sorted_entries(directory)? {
        if path.is_dir() {
            prune(root, &path, keep)?;
            if sorted_entries(&path)?.is_empty() {
                fs::remove_dir(&path)?;
            }
        } else if !keep.contains_key(path.strip_prefix(root)?) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no UTF-8 name", path.display()))?
        .to_owned())
}

fn file_stem(path: &Path) -> Result<String> {
    Ok(path
        .file_stem()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no UTF-8 name", path.display()))?
        .to_owned())
}

const STATE_README: &str = "# DreamWeave Network state

This branch is written by `dreamweave-network refresh`, normally from the scheduled workflow on
`main`. It is what the index observed, not what anybody published: every manifest here is a
cached copy of a claim its own site makes, and the site stays the authority.

Do not edit it by hand. To change what the index reads, edit `network/sources.toml` or
`network/curation.toml` on `main`. To recover from a broken state, delete the branch: the next
refresh rebuilds every current claim from the enrolled sites. Only the event history and the
first-observed dates are lost.

The layout and every record are described on the network site under About → State.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_readable_and_distinct() {
        let index =
            Url::parse("https://dreamweave-mp.github.io/DreamWeave-Mod-Template/dreamweave.json")
                .unwrap();
        let id = origin_id(&index);
        assert!(
            id.starts_with("dreamweave-mp-github-io-dreamweave-mod-template-"),
            "{id}"
        );
        let other =
            Url::parse("https://dreamweave-mp.github.io/DreamWeave-Mod-Template-/dreamweave.json")
                .unwrap();
        assert_ne!(origin_id(&other), id);
    }

    #[test]
    fn event_ids_depend_only_on_the_transition() {
        let project = ProjectId("4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90".to_owned());
        let first = event_id(
            EventKind::Changed,
            &project,
            "site-1",
            Some("aa"),
            Some("bb"),
        );
        assert_eq!(
            first,
            event_id(
                EventKind::Changed,
                &project,
                "site-1",
                Some("aa"),
                Some("bb")
            )
        );
        assert_ne!(
            first,
            event_id(
                EventKind::Changed,
                &project,
                "site-2",
                Some("aa"),
                Some("bb")
            )
        );
        assert_ne!(
            first,
            event_id(
                EventKind::Restored,
                &project,
                "site-1",
                Some("aa"),
                Some("bb")
            )
        );
        assert_eq!(first.len(), 20);
    }

    #[test]
    fn sha256_matches_the_reference_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
