//! DreamWeave Network Catalog, format version 1: this index's observations as JSON for other
//! readers.
//!
//! It is a cache format, not a protocol. Every claim in it names the site that published it and
//! the manifest URL that remains the authority, so another index or a client can use this file
//! to find things and then read the publisher's own documents to believe them. Nothing here
//! extends or replaces the DreamWeave protocol, and nothing in the protocol changes to suit it.
//!
//! Published beside the site as `network-data/catalog.json` and `network-data/events.json`,
//! described by `schemas/dreamweave-network-catalog-1.schema.json`.

use std::{collections::BTreeMap, sync::LazyLock};

use anyhow::{Result, bail};
use serde::Serialize;

use crate::{
    events,
    network::{Current, Network, Resolution},
    protocol::{ProjectId, RelationshipKind},
    state::{ClaimHealth, ClaimKey, Event, OriginHealth},
};

pub const FORMAT: &str = "dreamweave-network-catalog";
pub const EVENTS_FORMAT: &str = "dreamweave-network-events";
pub const FORMAT_VERSION: u32 = 1;
pub const SCHEMA_PATH: &str = "static/schemas/dreamweave-network-catalog-1.schema.json";

const NOTICE: &str = "A cache of claims observed by this index. Each claim's manifest URL, on the site that published it, is the authority; this file is not.";

#[derive(Debug, Clone, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub url: String,
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ClaimRef {
    pub project: ProjectId,
    pub origin: String,
}

impl From<&ClaimKey> for ClaimRef {
    fn from(key: &ClaimKey) -> Self {
        Self {
            project: key.project.clone(),
            origin: key.origin.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Origin {
    pub id: String,
    pub index_url: String,
    pub site_name: Option<String>,
    pub site_url: Option<String>,
    pub sources: Vec<String>,
    pub health: OriginHealth,
    pub problem: Option<String>,
    pub first_observed: String,
    pub last_attempt: String,
    pub last_success: Option<String>,
    pub projects: Vec<ProjectId>,
}

#[derive(Debug, Serialize)]
pub struct Claim {
    pub project: ProjectId,
    pub origin: String,
    pub name: String,
    pub summary: Option<String>,
    #[serde(rename = "type")]
    pub project_type: String,
    pub status: String,
    pub versioning: String,
    pub game: String,
    pub license: Option<String>,
    pub tags: Vec<String>,
    pub page: String,
    pub manifest_url: String,
    /// The digest of the manifest this index holds and describes here.
    pub manifest_sha256: String,
    /// The digest the site index advertised on the last successful read. Differs from
    /// `manifest_sha256` exactly when the held manifest is not the advertised one.
    pub advertised_sha256: String,
    pub health: ClaimHealth,
    /// True when the data describes a manifest older than what the site currently advertises,
    /// or a site that could not be read.
    pub cached: bool,
    pub problem: Option<String>,
    pub first_observed: String,
    pub last_changed: Option<String>,
    pub last_attempt: String,
    pub last_success: Option<String>,
    pub channels: BTreeMap<String, String>,
    pub current: Option<Current>,
    pub runtimes: BTreeMap<String, String>,
    pub lua_api: Option<String>,
    pub provides: Vec<String>,
    pub conflict: bool,
    pub superseded_by: Option<ClaimRef>,
    pub network_page: String,
}

#[derive(Debug, Serialize)]
pub struct Conflict {
    pub project: ProjectId,
    pub claims: Vec<ClaimRef>,
}

#[derive(Debug, Serialize)]
pub struct Relationship {
    pub from: ClaimRef,
    pub release: String,
    pub kind: RelationshipKind,
    pub project: Option<ProjectId>,
    pub capability: Option<String>,
    pub name: Option<String>,
    pub url: Option<String>,
    pub version: Option<String>,
    pub resolved: Vec<ClaimRef>,
    pub satisfied: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct Reference {
    pub from: ClaimRef,
    pub kind: RelationshipKind,
}

#[derive(Debug, Serialize)]
pub struct Capability {
    pub capability: String,
    pub providers: Vec<ClaimRef>,
    pub referenced_by: Vec<Reference>,
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub format: &'static str,
    pub format_version: u32,
    pub notice: &'static str,
    pub index: IndexInfo,
    pub observed_at: Option<String>,
    pub crawler: Option<String>,
    pub origins: Vec<Origin>,
    pub claims: Vec<Claim>,
    pub conflicts: Vec<Conflict>,
    pub relationships: Vec<Relationship>,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Serialize)]
pub struct EventRecord<'a> {
    #[serde(flatten)]
    pub event: &'a Event,
    pub tags: Vec<events::Tag>,
    pub headline: String,
}

#[derive(Debug, Serialize)]
pub struct Events<'a> {
    pub format: &'static str,
    pub format_version: u32,
    pub notice: &'static str,
    pub index: IndexInfo,
    pub observed_at: Option<String>,
    pub events: Vec<EventRecord<'a>>,
}

fn claim_record(claim: &crate::network::Claim, index: &IndexInfo) -> Claim {
    let manifest = claim
        .manifest
        .as_ref()
        .expect("listed claims hold a manifest");
    let project = &manifest.project;
    let release = claim.current_release();
    Claim {
        project: claim.key.project.clone(),
        origin: claim.key.origin.clone(),
        name: project.name.clone(),
        summary: project.summary.clone(),
        project_type: project.project_type.token().to_owned(),
        status: project.status.token().to_owned(),
        versioning: project.versioning.name().to_owned(),
        game: project.game.clone(),
        license: project.license.clone(),
        tags: project.tags.clone(),
        page: project.links.get("page").cloned().unwrap_or_default(),
        manifest_url: claim.record.entry.manifest.clone(),
        manifest_sha256: claim.record.ingested_sha256.clone().unwrap_or_default(),
        advertised_sha256: claim.record.advertised_sha256.clone(),
        health: claim.record.health,
        cached: claim.record.health != ClaimHealth::Current,
        problem: claim.record.problem.clone(),
        first_observed: claim.record.first_observed.clone(),
        last_changed: claim.record.last_changed.clone(),
        last_attempt: claim.record.last_attempt.clone(),
        last_success: claim.record.last_success.clone(),
        channels: manifest
            .channels
            .iter()
            .map(|(channel, head)| (channel.clone(), head.version.clone()))
            .collect(),
        current: claim.current.clone(),
        runtimes: release
            .map(|release| release.runtimes.clone())
            .unwrap_or_default(),
        lua_api: release
            .and_then(crate::protocol::Release::openmw)
            .and_then(|openmw| openmw.lua_api),
        provides: release
            .map(|release| release.provides.clone())
            .unwrap_or_default(),
        conflict: claim.conflict,
        superseded_by: claim.superseded_by.as_ref().map(ClaimRef::from),
        network_page: format!("{}/{}", index.url.trim_end_matches('/'), claim.path()),
    }
}

pub fn catalog(network: &Network, index: &IndexInfo) -> Catalog {
    let origins = network
        .origins
        .values()
        .map(|origin| Origin {
            id: origin.id.clone(),
            index_url: origin.index_url.clone(),
            site_name: origin.site_name.clone(),
            site_url: origin.site_url.clone(),
            sources: origin.sources.clone(),
            health: origin.health,
            problem: origin.problem.clone(),
            first_observed: origin.first_observed.clone(),
            last_attempt: origin.last_attempt.clone(),
            last_success: origin.last_success.clone(),
            projects: network
                .claims
                .keys()
                .filter(|key| key.origin == origin.id)
                .map(|key| key.project.clone())
                .collect(),
        })
        .collect();

    let claims = network
        .listed()
        .into_iter()
        .map(|claim| claim_record(claim, index))
        .collect();

    let relationships = network
        .edges
        .iter()
        .map(|edge| Relationship {
            from: ClaimRef::from(&edge.from),
            release: edge.release.clone(),
            kind: edge.kind(),
            project: edge.relationship.project.clone(),
            capability: edge.relationship.capability.clone(),
            name: edge.relationship.name.clone(),
            url: edge.relationship.url.clone(),
            version: edge.relationship.version.clone(),
            resolved: match &edge.resolution {
                Resolution::Project(targets) | Resolution::Capability(targets) => {
                    targets.iter().map(ClaimRef::from).collect()
                }
                Resolution::Unresolved => Vec::new(),
            },
            satisfied: edge.satisfied,
        })
        .collect();

    Catalog {
        format: FORMAT,
        format_version: FORMAT_VERSION,
        notice: NOTICE,
        index: index.clone(),
        observed_at: network.observed_at.clone(),
        crawler: network.crawler.clone(),
        origins,
        claims,
        conflicts: network
            .conflicts
            .iter()
            .map(|conflict| Conflict {
                project: conflict.project.clone(),
                claims: conflict.claims.iter().map(ClaimRef::from).collect(),
            })
            .collect(),
        relationships,
        capabilities: network
            .capabilities
            .iter()
            .map(|(capability, entry)| Capability {
                capability: capability.clone(),
                providers: entry.providers.iter().map(ClaimRef::from).collect(),
                referenced_by: entry
                    .references
                    .iter()
                    .map(|(key, kind)| Reference {
                        from: ClaimRef::from(key),
                        kind: *kind,
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn events<'a>(network: &'a Network, index: &IndexInfo) -> Events<'a> {
    Events {
        format: EVENTS_FORMAT,
        format_version: FORMAT_VERSION,
        notice: NOTICE,
        index: index.clone(),
        observed_at: network.observed_at.clone(),
        events: network
            .events
            .iter()
            .map(|event| EventRecord {
                event,
                tags: events::tags(event).into_iter().collect(),
                headline: events::headline(event),
            })
            .collect(),
    }
}

static SCHEMA: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../static/schemas/dreamweave-network-catalog-1.schema.json"
    ))
    .expect("the catalog schema is JSON");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the catalog schema compiles")
});

/// Checks generated output against the published schema. A failure is this index's bug and
/// fails the build: publishing a file that breaks its own documented format helps nobody.
pub fn check(document: &serde_json::Value) -> Result<()> {
    let errors: Vec<String> = SCHEMA
        .iter_errors(document)
        .take(10)
        .map(|error| format!("at {}: {error}", error.instance_path()))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        bail!(
            "the generated catalog does not match its own schema: {}",
            errors.join("; ")
        )
    }
}
