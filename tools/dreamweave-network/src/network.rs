//! The network as this index sees it: claims, their current releases, the dependency and
//! capability graphs, identity conflicts and the edges that lead out of the network. Derived from
//! the state and the curation file on every build; nothing here is stored.
//!
//! Every relationship is read from one release per claim, the claim's *current* release:
//!
//! 1. the head of `stable`, when the project has one;
//! 2. otherwise the highest-precedence head among its other channels, `development` excepted;
//! 3. otherwise the head of `development`.
//!
//! `stable` and `development` are the two channel names the protocol itself gives meaning to.
//! Every other channel is whatever its publisher says it is, so none is preferred over another
//! except by version. Historical releases keep their own relationships and are shown on the
//! claim's page; they never leak into the current graph.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use url::Url;

use crate::{
    config::Curation,
    protocol::{
        DEVELOPMENT_CHANNEL, Manifest, ProjectId, Relationship, RelationshipKind, Release,
        STABLE_CHANNEL,
    },
    state::{ClaimHealth, ClaimKey, ClaimRecord, Event, OriginRecord, SourceRecord, State},
    version::{Constraint, Version},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Current {
    pub channel: String,
    pub version: String,
}

/// Picks the release this index treats as a claim's current one. See the module docs.
pub fn current_release(manifest: &Manifest) -> Option<Current> {
    let head = |channel: &str| {
        manifest.channels.get(channel).map(|head| Current {
            channel: channel.to_owned(),
            version: head.version.clone(),
        })
    };
    if let Some(stable) = head(STABLE_CHANNEL) {
        return Some(stable);
    }
    let scheme = manifest.scheme();
    let other = manifest
        .channels
        .iter()
        .filter(|(channel, _)| channel.as_str() != DEVELOPMENT_CHANNEL)
        .filter_map(|(channel, head)| {
            Some((
                Version::parse(&head.version, scheme).ok()?,
                channel.clone(),
                head.version.clone(),
            ))
        })
        .max_by(|left, right| left.0.precedence(&right.0));
    if let Some((_, channel, version)) = other {
        return Some(Current { channel, version });
    }
    head(DEVELOPMENT_CHANNEL)
}

/// One claim with everything the build knows about it.
#[derive(Debug, Clone)]
pub struct Claim {
    pub key: ClaimKey,
    pub record: ClaimRecord,
    /// The last good manifest. `None` only for claims that never published a valid one.
    pub manifest: Option<Manifest>,
    pub current: Option<Current>,
    /// Another origin publishes a claim for the same project id.
    pub conflict: bool,
    /// A reviewed migration says this claim continues at another origin.
    pub superseded_by: Option<ClaimKey>,
}

impl Claim {
    pub fn name(&self) -> &str {
        self.manifest
            .as_ref()
            .map_or(&self.record.entry.name, |manifest| &manifest.project.name)
    }

    pub fn current_release(&self) -> Option<&Release> {
        let manifest = self.manifest.as_ref()?;
        manifest.release(&self.current.as_ref()?.version)
    }

    /// Listed in the catalog: holds a manifest and has not been withdrawn by its site.
    pub fn is_listed(&self) -> bool {
        self.manifest.is_some() && self.record.health != ClaimHealth::Withdrawn
    }

    pub fn path(&self) -> String {
        claim_path(&self.key)
    }
}

pub fn claim_path(key: &ClaimKey) -> String {
    format!("projects/{}/{}/", key.project, key.origin)
}

pub fn capability_slug(capability: &str) -> String {
    let mut slug = String::new();
    for character in capability.chars() {
        if character.is_ascii_alphanumeric() || character == '-' {
            slug.push(character);
        } else if character == ':' {
            slug.push_str("--");
        } else {
            slug.push('-');
        }
    }
    slug
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub project: ProjectId,
    pub claims: Vec<ClaimKey>,
}

/// Where a relationship lands inside the network, if anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Claims for the target's project id. More than one means the id is in conflict.
    Project(Vec<ClaimKey>),
    /// Claims whose current release provides the capability.
    Capability(Vec<ClaimKey>),
    /// Nothing indexed matches.
    Unresolved,
}

/// A relationship from one claim's current release.
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: ClaimKey,
    pub release: String,
    pub relationship: Relationship,
    pub resolution: Resolution,
    /// Whether the target's current release satisfies the constraint, under the target's own
    /// versioning scheme. `None` when there is no constraint or nothing to test it against.
    pub satisfied: Option<bool>,
}

impl Edge {
    pub fn kind(&self) -> RelationshipKind {
        self.relationship.kind
    }

    pub fn targets(&self) -> &[ClaimKey] {
        match &self.resolution {
            Resolution::Project(claims) | Resolution::Capability(claims) => claims,
            Resolution::Unresolved => &[],
        }
    }
}

/// A target outside the network, grouped only where its identity is clear.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "by", rename_all = "kebab-case")]
pub enum GapKey {
    /// A DreamWeave project id nobody in this index publishes.
    Project { project: ProjectId },
    /// A capability no current release provides.
    Capability { capability: String },
    /// No id, but a URL: grouped by the URL.
    Url { url: String },
    /// Nothing but a name. Grouped by exact text only, which says nothing about identity.
    Name { name: String },
}

#[derive(Debug, Clone)]
pub struct Gap {
    pub key: GapKey,
    pub names: BTreeSet<String>,
    pub urls: BTreeSet<String>,
    pub references: Vec<(ClaimKey, RelationshipKind)>,
}

#[derive(Debug, Clone, Default)]
pub struct Capability {
    pub providers: Vec<ClaimKey>,
    /// Current releases that require, recommend or conflict with the capability.
    pub references: Vec<(ClaimKey, RelationshipKind)>,
}

pub struct Network {
    pub observed_at: Option<String>,
    pub crawler: Option<String>,
    pub sources: Vec<SourceRecord>,
    pub origins: BTreeMap<String, OriginRecord>,
    pub claims: BTreeMap<ClaimKey, Claim>,
    pub conflicts: Vec<Conflict>,
    pub edges: Vec<Edge>,
    pub capabilities: BTreeMap<String, Capability>,
    pub gaps: Vec<Gap>,
    /// Newest first.
    pub events: Vec<Event>,
    pub curation: Curation,
}

impl Network {
    pub fn build(state: &State, curation: &Curation) -> Result<Self> {
        let mut claims = BTreeMap::new();
        for (key, record) in &state.claims {
            let manifest = match state.manifest_bytes(key) {
                Some(bytes) => {
                    Some(serde_json::from_slice::<Manifest>(bytes).with_context(|| {
                        format!("the held manifest of {} at {}", key.project, key.origin)
                    })?)
                }
                None => None,
            };
            if let Some(manifest) = &manifest
                && manifest.project.id != key.project
            {
                bail!(
                    "state holds a manifest for {} under the claim of {}",
                    manifest.project.id,
                    key.project
                );
            }
            let current = manifest.as_ref().and_then(current_release);
            claims.insert(
                key.clone(),
                Claim {
                    key: key.clone(),
                    record: record.clone(),
                    manifest,
                    current,
                    conflict: false,
                    superseded_by: None,
                },
            );
        }

        let mut network = Self {
            observed_at: state
                .network
                .as_ref()
                .map(|network| network.observed_at.clone()),
            crawler: state
                .network
                .as_ref()
                .map(|network| network.crawler.clone()),
            sources: state.sources.values().cloned().collect(),
            origins: state.origins.clone(),
            claims,
            conflicts: Vec::new(),
            edges: Vec::new(),
            capabilities: BTreeMap::new(),
            gaps: Vec::new(),
            events: state.events.values().cloned().collect(),
            curation: curation.clone(),
        };
        network.events.sort_by(|left, right| {
            right
                .observed_at
                .cmp(&left.observed_at)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        network.apply_migrations();
        network.find_conflicts();
        network.build_capabilities();
        network.build_edges();
        network.build_gaps();
        Ok(network)
    }

    /// Listed claims, in catalog order: name, then project id, then origin.
    pub fn listed(&self) -> Vec<&Claim> {
        let mut listed: Vec<&Claim> = self
            .claims
            .values()
            .filter(|claim| claim.is_listed())
            .collect();
        listed.sort_by(|left, right| {
            left.name()
                .to_lowercase()
                .cmp(&right.name().to_lowercase())
                .then_with(|| left.key.cmp(&right.key))
        });
        listed
    }

    /// The claims a relationship to `project` resolves to: every listed claim of the id that no
    /// reviewed migration supersedes.
    pub fn claims_of(&self, project: &ProjectId) -> Vec<ClaimKey> {
        self.claims
            .values()
            .filter(|claim| {
                &claim.key.project == project && claim.is_listed() && claim.superseded_by.is_none()
            })
            .map(|claim| claim.key.clone())
            .collect()
    }

    fn apply_migrations(&mut self) {
        for migration in &self.curation.migration {
            let origin_of = |index_url: &str| {
                let url = Url::parse(index_url).ok()?;
                self.origins
                    .values()
                    .find(|origin| Url::parse(&origin.index_url).ok().as_ref() == Some(&url))
                    .map(|origin| origin.id.clone())
            };
            let (Some(from), Some(to)) = (origin_of(&migration.from), origin_of(&migration.to))
            else {
                continue;
            };
            let from = ClaimKey {
                project: migration.project.clone(),
                origin: from,
            };
            let to = ClaimKey {
                project: migration.project.clone(),
                origin: to,
            };
            if self.claims.contains_key(&to)
                && let Some(claim) = self.claims.get_mut(&from)
            {
                claim.superseded_by = Some(to);
            }
        }
    }

    fn find_conflicts(&mut self) {
        let mut by_project: BTreeMap<ProjectId, Vec<ClaimKey>> = BTreeMap::new();
        for claim in self.claims.values() {
            if claim.is_listed() && claim.superseded_by.is_none() {
                by_project
                    .entry(claim.key.project.clone())
                    .or_default()
                    .push(claim.key.clone());
            }
        }
        for (project, claims) in by_project {
            if claims.len() < 2 {
                continue;
            }
            for key in &claims {
                if let Some(claim) = self.claims.get_mut(key) {
                    claim.conflict = true;
                }
            }
            self.conflicts.push(Conflict { project, claims });
        }
    }

    fn build_capabilities(&mut self) {
        let mut capabilities: BTreeMap<String, Capability> = BTreeMap::new();
        for claim in self.claims.values() {
            if !claim.is_listed() || claim.superseded_by.is_some() {
                continue;
            }
            let Some(release) = claim.current_release() else {
                continue;
            };
            for capability in &release.provides {
                capabilities
                    .entry(capability.clone())
                    .or_default()
                    .providers
                    .push(claim.key.clone());
            }
            for relationship in &release.relationships {
                if let Some(capability) = &relationship.capability {
                    capabilities
                        .entry(capability.clone())
                        .or_default()
                        .references
                        .push((claim.key.clone(), relationship.kind));
                }
            }
        }
        self.capabilities = capabilities;
    }

    fn satisfied(&self, relationship: &Relationship, targets: &[ClaimKey]) -> Option<bool> {
        let constraint = relationship.version.as_ref()?;
        let target = self.claims.get(targets.first()?)?;
        let manifest = target.manifest.as_ref()?;
        let version = Version::parse(&target.current.as_ref()?.version, manifest.scheme()).ok()?;
        let constraint = Constraint::parse(constraint, manifest.scheme()).ok()?;
        Some(constraint.allows(&version))
    }

    fn build_edges(&mut self) {
        let mut edges = Vec::new();
        for claim in self.claims.values() {
            if !claim.is_listed() || claim.superseded_by.is_some() {
                continue;
            }
            let Some(release) = claim.current_release() else {
                continue;
            };
            for relationship in &release.relationships {
                let resolution = if let Some(project) = &relationship.project {
                    let claims = self.claims_of(project);
                    if claims.is_empty() {
                        Resolution::Unresolved
                    } else {
                        Resolution::Project(claims)
                    }
                } else if let Some(capability) = &relationship.capability {
                    match self.capabilities.get(capability) {
                        Some(entry) if !entry.providers.is_empty() => {
                            Resolution::Capability(entry.providers.clone())
                        }
                        _ => Resolution::Unresolved,
                    }
                } else {
                    Resolution::Unresolved
                };
                let satisfied = match &resolution {
                    Resolution::Project(targets) => self.satisfied(relationship, targets),
                    _ => None,
                };
                edges.push(Edge {
                    from: claim.key.clone(),
                    release: release.version.clone(),
                    relationship: relationship.clone(),
                    resolution,
                    satisfied,
                });
            }
        }
        self.edges = edges;
    }

    fn build_gaps(&mut self) {
        let mut gaps: BTreeMap<GapKey, Gap> = BTreeMap::new();
        for edge in &self.edges {
            if edge.resolution != Resolution::Unresolved {
                continue;
            }
            let relationship = &edge.relationship;
            let key = if let Some(project) = &relationship.project {
                GapKey::Project {
                    project: project.clone(),
                }
            } else if let Some(capability) = &relationship.capability {
                GapKey::Capability {
                    capability: capability.clone(),
                }
            } else if let Some(url) = relationship.url.as_deref().and_then(normalize_url) {
                GapKey::Url { url }
            } else {
                GapKey::Name {
                    name: relationship.name.clone().unwrap_or_default(),
                }
            };
            let gap = gaps.entry(key.clone()).or_insert_with(|| Gap {
                key,
                names: BTreeSet::new(),
                urls: BTreeSet::new(),
                references: Vec::new(),
            });
            gap.names.extend(relationship.name.clone());
            gap.urls.extend(relationship.url.clone());
            gap.references.push((edge.from.clone(), relationship.kind));
        }
        let mut gaps: Vec<Gap> = gaps.into_values().collect();
        gaps.sort_by(|left, right| {
            right
                .references
                .len()
                .cmp(&left.references.len())
                .then_with(|| left.key.cmp(&right.key))
        });
        self.gaps = gaps;
    }

    /// Edges into a claim: whatever requires, recommends, conflicts with, is compatible with or
    /// replaces it, by id or through a capability it provides.
    pub fn reverse_edges(&self, key: &ClaimKey) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|edge| edge.targets().contains(key) && &edge.from != key)
            .collect()
    }

    pub fn forward_edges(&self, key: &ClaimKey) -> Vec<&Edge> {
        self.edges.iter().filter(|edge| &edge.from == key).collect()
    }

    /// Distinct claims that require this one, which is what "used by" means on this site.
    pub fn used_by(&self, key: &ClaimKey) -> BTreeSet<ClaimKey> {
        self.reverse_edges(key)
            .into_iter()
            .filter(|edge| edge.kind() == RelationshipKind::Requires)
            .map(|edge| edge.from.clone())
            .collect()
    }

    /// Candidate sources: URLs named by relationships that lead outside every enrolled site.
    /// Nothing follows them. A maintainer may inspect one and propose it.
    pub fn candidates(&self) -> Vec<(String, usize)> {
        let enrolled: Vec<Url> = self
            .origins
            .values()
            .filter_map(|origin| Url::parse(&origin.index_url).ok()?.join("./").ok())
            .collect();
        let mut candidates: BTreeMap<String, BTreeSet<ClaimKey>> = BTreeMap::new();
        for gap in &self.gaps {
            for url in &gap.urls {
                let Ok(parsed) = Url::parse(url) else {
                    continue;
                };
                let inside = enrolled.iter().any(|site| {
                    site.origin() == parsed.origin() && parsed.path().starts_with(site.path())
                });
                if !inside {
                    let references = candidates.entry(url.clone()).or_default();
                    references.extend(gap.references.iter().map(|(key, _)| key.clone()));
                }
            }
        }
        let mut candidates: Vec<(String, usize)> = candidates
            .into_iter()
            .map(|(url, references)| (url, references.len()))
            .collect();
        candidates.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        candidates
    }
}

/// A URL in a form that groups trivially different spellings: lowercase scheme and host (the
/// URL parser does that), no fragment, no trailing slash on the path.
fn normalize_url(text: &str) -> Option<String> {
    let mut url = Url::parse(text).ok()?;
    url.set_fragment(None);
    let mut normalized = url.to_string();
    if normalized.ends_with('/') && url.query().is_none() {
        normalized.pop();
    }
    Some(normalized)
}

/// Compares two versions of one claim's project for display ordering, newest first. Versions
/// that do not parse sort last; the parser never admits one, so that is a formality.
pub fn newest_first(manifest: &Manifest, left: &str, right: &str) -> Ordering {
    let scheme = manifest.scheme();
    match (Version::parse(left, scheme), Version::parse(right, scheme)) {
        (Ok(left), Ok(right)) => right.precedence(&left),
        _ => left.cmp(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse::parse_manifest;
    use serde_json::{Value, json};

    const CANDLELIGHT: &str = include_str!("../tests/fixtures/mod-template/candlelight.json");
    const TALLOW: &str = include_str!("../tests/fixtures/mod-template/tallow.json");

    fn manifest(value: &Value) -> Manifest {
        parse_manifest(&serde_json::to_vec(value).unwrap()).unwrap()
    }

    #[test]
    fn stable_wins_when_there_is_one() {
        let candlelight = manifest(&serde_json::from_str(CANDLELIGHT).unwrap());
        assert_eq!(
            current_release(&candlelight),
            Some(Current {
                channel: "stable".to_owned(),
                version: "1.1.0".to_owned()
            })
        );
    }

    #[test]
    fn without_stable_the_newest_named_channel_beats_development() {
        let mut value: Value = serde_json::from_str(TALLOW).unwrap();
        value["releases"][1]["channel"] = "beta".into();
        value["channels"] =
            json!({ "beta": { "version": "1.0.0" }, "development": { "version": "1.0.1-dev.0" } });
        assert_eq!(current_release(&manifest(&value)).unwrap().channel, "beta");

        value["releases"].as_array_mut().unwrap().remove(1);
        value["channels"] = json!({ "development": { "version": "1.0.1-dev.0" } });
        assert_eq!(
            current_release(&manifest(&value)).unwrap().channel,
            "development"
        );
    }

    #[test]
    fn capability_slugs_are_path_safe() {
        assert_eq!(
            capability_slug("dreamweave:dynamic-lights"),
            "dreamweave--dynamic-lights"
        );
        assert_eq!(capability_slug("org.example.thing"), "org-example-thing");
    }

    #[test]
    fn urls_group_trivially_different_spellings() {
        assert_eq!(
            normalize_url("https://WWW.Tamriel-Rebuilt.org/#download"),
            normalize_url("https://www.tamriel-rebuilt.org")
        );
    }
}
