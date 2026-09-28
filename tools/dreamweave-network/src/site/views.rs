//! What the templates render, computed here so the templates only lay it out. Tera is good at
//! markup and bad at logic; every decision about what a page says is made in Rust, where it can
//! be tested.
//!
//! Paths are site-relative (`projects/…/`) and go through Zola's `get_url`. External URLs are
//! absolute and come from publishers; templates escape both.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use serde::Serialize;

use super::format::{
    constraint_label, date_of, format_label, origin_label, platform_label, plural, runtime_label,
    short_digest, size_label, slug, time_label,
};
use crate::{
    diff::Change,
    events::{self, Line, LineClass, Tag},
    markdown,
    network::{self, Claim, Edge, Network, Resolution, capability_slug, claim_path},
    protocol::{
        Artifact, DEVELOPMENT_CHANNEL, Manifest, Notes, ProjectStatus, RelationshipKind, Release,
        ReleaseStatus, SourceKind,
    },
    state::{ClaimHealth, ClaimKey, Event, EventKind, OriginHealth},
};

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fact {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Fact {
    pub fn text(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.to_owned(),
            value: value.into(),
            code: false,
            url: None,
            path: None,
        }
    }

    pub fn code(label: &str, value: impl Into<String>) -> Self {
        Self {
            code: true,
            ..Self::text(label, value)
        }
    }

    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Head {
    pub channel: String,
    pub version: String,
    pub date: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Image {
    pub src: String,
    pub alt: String,
    pub original: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagLink {
    pub label: String,
    pub path: String,
}

/// A project claim in a list.
// A view model: every flag is a separate fact the card shows.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub path: String,
    pub project: String,
    pub origin: String,
    pub origin_label: String,
    pub origin_path: String,
    pub name: String,
    pub summary: Option<String>,
    pub type_token: String,
    pub type_label: String,
    pub status: String,
    pub health: String,
    pub health_label: String,
    pub cached: bool,
    pub conflict: bool,
    pub superseded: bool,
    /// The current release ships programs, one archive per platform, rather than game data.
    pub program: bool,
    pub current: Option<Head>,
    pub heads: Vec<Head>,
    pub compatibility: Vec<String>,
    pub requires: usize,
    pub used_by: usize,
    pub provides: Vec<String>,
    pub tags: Vec<TagLink>,
    pub image: Option<Image>,
    pub page: Option<String>,
    /// The newest release date its publisher gave.
    pub updated: Option<String>,
    pub last_changed: Option<String>,
    /// Space-separated tokens the catalog's filter reads: `type:mod status:active tag:lua`.
    pub filters: String,
}

pub fn heads(manifest: &Manifest, current: Option<&network::Current>) -> Vec<Head> {
    let mut heads: Vec<Head> = manifest
        .channels
        .iter()
        .map(|(channel, head)| Head {
            channel: channel.clone(),
            version: head.version.clone(),
            date: manifest
                .release(&head.version)
                .and_then(|release| release.date.clone()),
            current: current.is_some_and(|current| &current.channel == channel),
        })
        .collect();
    heads.sort_by_key(|head| {
        (
            !head.current,
            head.channel == DEVELOPMENT_CHANNEL,
            head.channel.clone(),
        )
    });
    heads
}

pub fn compatibility(release: Option<&Release>) -> Vec<String> {
    let Some(release) = release else {
        return Vec::new();
    };
    let mut facts: Vec<String> = release
        .runtimes
        .iter()
        .map(|(runtime, constraint)| {
            format!(
                "{} {}",
                runtime_label(runtime),
                constraint_label(constraint)
            )
        })
        .collect();
    if let Some(lua_api) = release.openmw().and_then(|openmw| openmw.lua_api) {
        facts.push(format!("Lua API {lua_api}"));
    }
    if release.platforms.is_empty() {
        if !release.runtimes.is_empty() {
            facts.push("any platform".to_owned());
        }
    } else {
        facts.extend(
            release
                .platforms
                .iter()
                .map(|platform| platform_label(&platform.os, &platform.arch)),
        );
    }
    facts
}

pub fn tag_path(tag: &str) -> String {
    format!("browse/tag-{}/", slug(tag))
}

pub fn origin_path(origin: &str) -> String {
    format!("origins/{origin}/")
}

pub fn card(network: &Network, claim: &Claim) -> Card {
    let manifest = claim
        .manifest
        .as_ref()
        .expect("cards are made for listed claims");
    let project = &manifest.project;
    let release = claim.current_release();
    let origin = network.origins.get(&claim.key.origin);
    let heads = heads(manifest, claim.current.as_ref());
    let current = heads.iter().find(|head| head.current).cloned();
    let compatibility = compatibility(release);
    let provides = release
        .map(|release| release.provides.clone())
        .unwrap_or_default();
    let requires = network
        .forward_edges(&claim.key)
        .iter()
        .filter(|edge| edge.kind() == RelationshipKind::Requires)
        .count();
    let used_by = network.used_by(&claim.key).len();
    let mut filters = vec![
        format!("type:{}", project.project_type.token()),
        format!("status:{}", project.status.token()),
        format!("health:{}", claim.record.health.token()),
        format!("game:{}", project.game),
    ];
    filters.extend(project.tags.iter().map(|tag| format!("tag:{}", slug(tag))));
    filters.extend(
        manifest
            .channels
            .keys()
            .map(|channel| format!("channel:{channel}")),
    );
    let program = release.is_some_and(|release| release.artifacts.iter().any(Artifact::is_program));
    if program {
        filters.push("format:program".to_owned());
    }
    if let Some(release) = release {
        filters.extend(
            release
                .runtimes
                .keys()
                .map(|runtime| format!("runtime:{runtime}")),
        );
    }
    Card {
        path: claim.path(),
        project: claim.key.project.to_string(),
        origin: claim.key.origin.clone(),
        origin_label: origin.map_or_else(|| claim.key.origin.clone(), origin_label),
        origin_path: origin_path(&claim.key.origin),
        name: project.name.clone(),
        summary: project.summary.clone(),
        type_token: project.project_type.token().to_owned(),
        type_label: project.project_type.label().to_owned(),
        status: project.status.token().to_owned(),
        health: claim.record.health.token().to_owned(),
        health_label: claim.record.health.label().to_owned(),
        cached: claim.record.health != ClaimHealth::Current,
        conflict: claim.conflict,
        superseded: claim.superseded_by.is_some(),
        program,
        current,
        heads,
        compatibility,
        requires,
        used_by,
        provides,
        tags: project
            .tags
            .iter()
            .map(|tag| TagLink {
                label: tag.clone(),
                path: tag_path(tag),
            })
            .collect(),
        image: claim.record.media.as_ref().and_then(|media| {
            Some(Image {
                src: format!("network-{}", media.file.as_ref()?),
                alt: media.alt.clone(),
                original: media.url.clone(),
            })
        }),
        page: project.links.get("page").cloned(),
        updated: manifest
            .releases
            .iter()
            .filter_map(|release| release.date.clone())
            .max(),
        last_changed: claim.record.last_changed.as_deref().map(time_label),
        filters: filters.join(" "),
    }
}

// Relationships -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RelationshipItem {
    pub kind: String,
    pub name: String,
    /// One network claim, or a capability page.
    pub path: Option<String>,
    /// Several claims for one id: an identity conflict, shown as such.
    pub targets: Vec<Link>,
    pub external: Option<String>,
    pub constraint: Option<String>,
    pub satisfied: Option<bool>,
    pub satisfied_label: Option<String>,
    pub reason: Option<String>,
    /// `project`, `capability`, `conflict` or `external`.
    pub state: String,
    pub via: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelationshipGroup {
    pub kind: String,
    pub label: String,
    pub items: Vec<RelationshipItem>,
}

fn kind_label(kind: RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::Requires => "Requires",
        RelationshipKind::Recommends => "Recommends",
        RelationshipKind::Conflicts => "Conflicts with",
        RelationshipKind::Compatible => "Compatible with",
        RelationshipKind::Replaces => "Replaces",
    }
}

fn target_link(network: &Network, key: &ClaimKey) -> Link {
    let claim = &network.claims[key];
    let origin = network
        .origins
        .get(&key.origin)
        .map_or_else(|| key.origin.clone(), origin_label);
    Link {
        label: format!("{} at {origin}", claim.name()),
        url: claim.path(),
    }
}

fn satisfied_label(network: &Network, edge: &Edge) -> Option<String> {
    let satisfied = edge.satisfied?;
    let target = network.claims.get(edge.targets().first()?)?;
    let current = target.current.as_ref()?;
    Some(if satisfied {
        format!("satisfied by {} {}", current.channel, current.version)
    } else {
        format!("not satisfied by {} {}", current.channel, current.version)
    })
}

pub fn relationship_item(network: &Network, edge: &Edge) -> RelationshipItem {
    let relationship = &edge.relationship;
    let targets = edge.targets();
    let (state, path, links, via) = match &edge.resolution {
        Resolution::Project(targets) if targets.len() == 1 => {
            ("project", Some(claim_path(&targets[0])), Vec::new(), None)
        }
        Resolution::Project(targets) => (
            "conflict",
            None,
            targets
                .iter()
                .map(|key| target_link(network, key))
                .collect(),
            None,
        ),
        Resolution::Capability(_) => {
            let capability = relationship.capability.clone().unwrap_or_default();
            (
                "capability",
                Some(format!("capabilities/{}/", capability_slug(&capability))),
                targets
                    .iter()
                    .map(|key| target_link(network, key))
                    .collect(),
                Some(format!("capability {capability}")),
            )
        }
        Resolution::Unresolved => (
            "external",
            relationship
                .capability
                .as_ref()
                .map(|capability| format!("capabilities/{}/", capability_slug(capability))),
            Vec::new(),
            relationship
                .project
                .as_ref()
                .map(|project| {
                    format!("project id {project}, not published by any site this index reads")
                })
                .or_else(|| {
                    relationship.capability.as_ref().map(|capability| {
                        format!("capability {capability}, provided by nothing indexed")
                    })
                }),
        ),
    };
    let name = relationship
        .name
        .clone()
        .or_else(|| {
            targets
                .first()
                .map(|key| network.claims[key].name().to_owned())
        })
        .or_else(|| relationship.capability.clone())
        .or_else(|| relationship.project.as_ref().map(ToString::to_string))
        .unwrap_or_else(|| "unnamed".to_owned());
    RelationshipItem {
        kind: relationship.kind.token().to_owned(),
        name,
        path,
        targets: links,
        external: relationship.url.clone(),
        constraint: relationship.version.clone(),
        satisfied: edge.satisfied,
        satisfied_label: satisfied_label(network, edge),
        reason: relationship.reason.clone(),
        state: state.to_owned(),
        via,
    }
}

pub fn relationship_groups(network: &Network, edges: &[&Edge]) -> Vec<RelationshipGroup> {
    RelationshipKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let items: Vec<RelationshipItem> = edges
                .iter()
                .filter(|edge| edge.kind() == kind)
                .map(|edge| relationship_item(network, edge))
                .collect();
            (!items.is_empty()).then(|| RelationshipGroup {
                kind: kind.token().to_owned(),
                label: kind_label(kind).to_owned(),
                items,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct ReverseItem {
    pub kind: String,
    pub label: String,
    pub name: String,
    pub path: String,
    pub constraint: Option<String>,
    pub satisfied: Option<bool>,
    pub via: Option<String>,
}

pub fn reverse_items(network: &Network, key: &ClaimKey) -> Vec<ReverseItem> {
    let mut items: Vec<ReverseItem> = network
        .reverse_edges(key)
        .into_iter()
        .map(|edge| ReverseItem {
            kind: edge.kind().token().to_owned(),
            label: match edge.kind() {
                RelationshipKind::Requires => "required by",
                RelationshipKind::Recommends => "recommended by",
                RelationshipKind::Conflicts => "conflicts with",
                RelationshipKind::Compatible => "marked compatible by",
                RelationshipKind::Replaces => "replaced by",
            }
            .to_owned(),
            name: network.claims[&edge.from].name().to_owned(),
            path: claim_path(&edge.from),
            constraint: edge.relationship.version.clone(),
            satisfied: edge.satisfied,
            via: edge
                .relationship
                .capability
                .as_ref()
                .map(|capability| format!("through capability {capability}")),
        })
        .collect();
    items.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.cmp(&right.name))
    });
    items
}

// Releases and artifacts ----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SourceLink {
    pub url: String,
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignatureView {
    pub format: String,
    pub url: String,
    pub issuer: Option<String>,
    pub identity: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactView {
    pub id: String,
    pub filename: String,
    pub format: String,
    pub format_label: String,
    pub media_type: String,
    pub size: u64,
    pub size_label: String,
    /// The one platform a program archive is built for.
    pub platform: Option<String>,
    pub program: bool,
    pub sha256: String,
    pub short_sha256: String,
    pub download: Option<String>,
    pub sources: Vec<SourceLink>,
    pub signatures: Vec<SignatureView>,
    pub layout: Vec<Fact>,
}

pub fn artifact_view(artifact: &Artifact) -> ArtifactView {
    let sources: Vec<SourceLink> = artifact
        .sources
        .iter()
        .map(|source| SourceLink {
            url: source.url.clone(),
            kind: match source.kind {
                SourceKind::Publisher => "publisher",
                SourceKind::Mirror => "mirror",
            }
            .to_owned(),
            name: source.name.clone(),
        })
        .collect();
    let mut layout = Vec::new();
    if let Some(declared) = &artifact.layout {
        for (label, value) in [
            ("Release document", &declared.release_document),
            ("Offline documentation", &declared.documentation),
            ("Installer", &declared.installer),
        ] {
            if let Some(value) = value {
                layout.push(Fact::code(label, value.clone()));
            }
        }
    }
    ArtifactView {
        id: artifact.id.clone(),
        filename: artifact.filename.clone(),
        format: artifact.format.clone(),
        format_label: format_label(&artifact.format),
        media_type: artifact.media_type.clone(),
        size: artifact.size,
        size_label: size_label(artifact.size),
        platform: artifact
            .platform
            .as_ref()
            .map(|platform| platform_label(&platform.os, &platform.arch)),
        program: artifact.is_program(),
        sha256: artifact.digests.sha256.clone(),
        short_sha256: short_digest(&artifact.digests.sha256),
        // The publisher's own source first when there is one; the order in the manifest
        // carries no meaning, so this is a presentation choice and nothing more.
        download: sources
            .iter()
            .find(|source| source.kind == "publisher")
            .or_else(|| sources.first())
            .map(|source| source.url.clone()),
        sources,
        signatures: artifact
            .signatures
            .iter()
            .map(|signature| SignatureView {
                format: signature.format.clone(),
                url: signature.url.clone(),
                issuer: signature.issuer.clone(),
                identity: signature.identity.clone(),
            })
            .collect(),
        layout,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceView {
    pub repository: String,
    pub tag: Option<String>,
    pub release: Option<String>,
    pub revision: Option<String>,
    pub revision_url: Option<String>,
}

// A view model: every flag is a separate fact the page shows about the release.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseView {
    pub anchor: String,
    pub version: String,
    pub channel: String,
    pub date: Option<String>,
    pub status: String,
    pub head: bool,
    pub current: bool,
    pub notice: Option<String>,
    pub summary: Option<String>,
    /// Sanitized publisher notes, everything but the summary.
    pub notes_html: Option<String>,
    pub breaking: bool,
    pub migration: bool,
    pub source: SourceView,
    pub compatibility: Vec<String>,
    pub provides: Vec<String>,
    pub relationships: usize,
    pub artifacts: Vec<ArtifactView>,
    pub critical_extensions: Vec<String>,
}

pub fn notes_html(notes: &Notes) -> Option<String> {
    let mut html = String::new();
    if let Some(highlights) = &notes.highlights {
        html.push_str(&markdown::block(highlights));
    }
    let lists = [
        ("Breaking changes", "breaking", &notes.breaking),
        ("Added", "added", &notes.added),
        ("Changed", "changed", &notes.changed),
        ("Fixed", "fixed", &notes.fixed),
        ("Known issues", "known-issues", &notes.known_issues),
    ];
    for (title, class, items) in lists {
        let Some(items) = items else {
            continue;
        };
        if items.is_empty() {
            continue;
        }
        let _ = write!(
            html,
            "<h4>{title}</h4><ul class=\"net-notes net-notes--{class}\">"
        );
        for item in items {
            let _ = write!(html, "<li>{}</li>", markdown::inline(item));
        }
        html.push_str("</ul>");
    }
    if let Some(migration) = &notes.migration {
        html.push_str("<h4>Migration</h4>");
        html.push_str(&markdown::block(migration));
    }
    if let Some(text) = &notes.notes {
        html.push_str(&markdown::block(text));
    }
    (!html.is_empty()).then_some(html)
}

pub fn release_anchor(version: &str) -> String {
    format!("release-{}", slug(version))
}

pub fn release_view(
    manifest: &Manifest,
    release: &Release,
    current: Option<&network::Current>,
) -> ReleaseView {
    let head = manifest.channels.get(&release.channel).is_some_and(|head| {
        manifest
            .release(&head.version)
            .is_some_and(|found| found.version == release.version)
    });
    let notice = release
        .yanked
        .as_ref()
        .map(|notice| ("Yanked", notice))
        .or_else(|| {
            release
                .deprecated
                .as_ref()
                .map(|notice| ("Deprecated", notice))
        })
        .map(|(label, notice)| {
            let mut text = format!("{label}: {}", notice.reason);
            if let Some(replacement) = &notice.replacement {
                let _ = write!(
                    text,
                    " The publisher names {replacement} as the replacement."
                );
            }
            text
        });
    let revision_url = release.source.revision.as_ref().and_then(|revision| {
        release
            .source
            .repository
            .starts_with("https://github.com/")
            .then(|| {
                format!(
                    "{}/commit/{revision}",
                    release.source.repository.trim_end_matches('/')
                )
            })
    });
    ReleaseView {
        anchor: release_anchor(&release.version),
        version: release.version.clone(),
        channel: release.channel.clone(),
        date: release.date.clone(),
        status: release.status.token().to_owned(),
        head,
        current: current.is_some_and(|current| current.version == release.version),
        notice,
        summary: release
            .notes
            .as_ref()
            .and_then(|notes| notes.summary.clone()),
        notes_html: release.notes.as_ref().and_then(notes_html),
        breaking: release
            .notes
            .as_ref()
            .and_then(|notes| notes.breaking.as_ref())
            .is_some_and(|items| !items.is_empty()),
        migration: release
            .notes
            .as_ref()
            .is_some_and(|notes| notes.migration.is_some()),
        source: SourceView {
            repository: release.source.repository.clone(),
            tag: release.source.tag.clone(),
            release: release.source.release.clone(),
            revision: release.source.revision.clone(),
            revision_url,
        },
        compatibility: compatibility(Some(release)),
        provides: release.provides.clone(),
        relationships: release.relationships.len(),
        artifacts: release.artifacts.iter().map(artifact_view).collect(),
        critical_extensions: release.critical_extensions.clone(),
    }
}

// Events --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub id: String,
    pub path: String,
    pub name: String,
    pub claim_path: Option<String>,
    pub origin_label: String,
    pub kind: String,
    pub observed_at: String,
    pub observed_label: String,
    pub headline: String,
    pub tags: Vec<String>,
    pub anomaly: bool,
    pub development_only: bool,
}

pub fn event_kind_label(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Observed => "observed",
        EventKind::Changed => "changed",
        EventKind::Withdrawn => "withdrawn",
        EventKind::Restored => "restored",
    }
}

pub fn event_row(network: &Network, event: &Event) -> EventRow {
    let key = ClaimKey {
        project: event.project.clone(),
        origin: event.origin.clone(),
    };
    let tags = events::tags(event);
    EventRow {
        id: event.id.clone(),
        path: format!("updates/{}/", event.id),
        name: event.name.clone(),
        claim_path: network.claims.contains_key(&key).then(|| claim_path(&key)),
        origin_label: network
            .origins
            .get(&event.origin)
            .map_or_else(|| event.origin.clone(), origin_label),
        kind: event_kind_label(event.kind).to_owned(),
        observed_at: event.observed_at.clone(),
        observed_label: time_label(&event.observed_at),
        headline: events::headline(event),
        anomaly: tags.contains(&Tag::Anomaly),
        development_only: events::is_development_only(event),
        tags: tags.into_iter().map(|tag| tag.token().to_owned()).collect(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LineView {
    #[serde(flatten)]
    pub line: Line,
    pub path: Option<String>,
    pub external: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionView {
    pub title: String,
    pub anomaly: bool,
    pub lines: Vec<LineView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotesView {
    pub version: String,
    pub channel: String,
    pub summary: Option<String>,
    pub html: Option<String>,
    pub breaking: bool,
    pub migration: bool,
}

pub fn event_sections(network: &Network, event: &Event) -> Vec<SectionView> {
    events::sections(event)
        .into_iter()
        .map(|section| SectionView {
            anomaly: section
                .lines
                .iter()
                .any(|line| line.class == LineClass::Anomaly),
            title: section.title,
            lines: section
                .lines
                .into_iter()
                .map(|line| {
                    let path = line.target.as_ref().and_then(|target| {
                        if let Some(project) = &target.project {
                            network.claims_of(project).first().map(claim_path)
                        } else {
                            target.capability.as_ref().map(|capability| {
                                format!("capabilities/{}/", capability_slug(capability))
                            })
                        }
                    });
                    let external = line.target.as_ref().and_then(|target| target.url.clone());
                    LineView {
                        line,
                        path,
                        external,
                    }
                })
                .collect(),
        })
        .collect()
}

pub fn event_notes(event: &Event) -> Vec<NotesView> {
    event
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::ReleaseAdded {
                version,
                channel,
                notes: Some(notes),
                ..
            } => Some(NotesView {
                version: version.clone(),
                channel: channel.clone(),
                summary: notes.summary.clone(),
                html: notes_html(notes),
                breaking: notes
                    .breaking
                    .as_ref()
                    .is_some_and(|items| !items.is_empty()),
                migration: notes.migration.is_some(),
            }),
            _ => None,
        })
        .collect()
}

// Release activity ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseRow {
    pub name: String,
    pub claim_path: String,
    pub release_path: String,
    pub origin_label: String,
    pub version: String,
    pub channel: String,
    pub status: String,
    /// The date the publisher gave the release.
    pub date: Option<String>,
    /// When this index first saw the release in a manifest. Never the same thing as `date`.
    pub first_observed: String,
    pub head: bool,
    pub breaking: bool,
    pub migration: bool,
    pub project_status: String,
}

/// When this index first held a manifest listing each release of a claim: the earliest event
/// that added it, else the claim's first observation.
fn first_seen(network: &Network, key: &ClaimKey) -> BTreeMap<String, String> {
    let mut seen = BTreeMap::new();
    for event in network.events.iter().rev() {
        if event.project != key.project || event.origin != key.origin {
            continue;
        }
        for change in &event.changes {
            if let Change::ReleaseAdded { version, .. } = change {
                seen.entry(version.clone())
                    .or_insert_with(|| event.observed_at.clone());
            }
        }
    }
    seen
}

pub fn release_rows(network: &Network) -> Vec<ReleaseRow> {
    let mut rows = Vec::new();
    for claim in network.listed() {
        let manifest = claim
            .manifest
            .as_ref()
            .expect("listed claims hold a manifest");
        let seen = first_seen(network, &claim.key);
        let origin = network
            .origins
            .get(&claim.key.origin)
            .map_or_else(|| claim.key.origin.clone(), origin_label);
        for release in &manifest.releases {
            let view = release_view(manifest, release, claim.current.as_ref());
            rows.push(ReleaseRow {
                name: manifest.project.name.clone(),
                claim_path: claim.path(),
                release_path: format!("{}#{}", claim.path(), view.anchor),
                origin_label: origin.clone(),
                version: release.version.clone(),
                channel: release.channel.clone(),
                status: release.status.token().to_owned(),
                date: release.date.clone(),
                first_observed: seen.get(&release.version).map_or_else(
                    || date_of(&claim.record.first_observed),
                    |time| date_of(time),
                ),
                head: view.head,
                breaking: view.breaking,
                migration: view.migration,
                project_status: manifest.project.status.token().to_owned(),
            });
        }
    }
    rows.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| right.first_observed.cmp(&left.first_observed))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| right.version.cmp(&left.version))
    });
    rows
}

// Health --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Stat {
    pub label: String,
    pub value: usize,
    pub path: Option<String>,
    /// `ok`, `warn`, `danger` or empty: what the number means, never its only signal.
    pub state: String,
}

pub fn stat(label: &str, value: usize, path: Option<&str>, state: &str) -> Stat {
    Stat {
        label: label.to_owned(),
        value,
        path: path.map(str::to_owned),
        state: state.to_owned(),
    }
}

pub fn network_stats(network: &Network) -> Vec<Stat> {
    let listed = network.listed();
    let projects: BTreeSet<&str> = listed
        .iter()
        .map(|claim| claim.key.project.0.as_str())
        .collect();
    let active = listed
        .iter()
        .filter(|claim| {
            claim
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.project.status == ProjectStatus::Active)
        })
        .count();
    let foundations = listed
        .iter()
        .filter(|claim| {
            claim.manifest.as_ref().is_some_and(|manifest| {
                matches!(
                    manifest.project.project_type,
                    crate::protocol::ProjectType::Library | crate::protocol::ProjectType::Framework
                )
            })
        })
        .count();
    let releases: usize = listed
        .iter()
        .filter_map(|claim| claim.manifest.as_ref())
        .map(|manifest| manifest.releases.len())
        .sum();
    let resolved_edges = network
        .edges
        .iter()
        .filter(|edge| edge.resolution != Resolution::Unresolved)
        .count();
    let stale = listed
        .iter()
        .filter(|claim| claim.record.health != ClaimHealth::Current)
        .count();
    let unavailable = network
        .origins
        .values()
        .filter(|origin| origin.health != OriginHealth::Healthy)
        .count();
    let warn = |count: usize| if count == 0 { "ok" } else { "warn" };
    vec![
        stat("sites", network.origins.len(), Some("origins/"), ""),
        stat("project claims", listed.len(), Some("projects/"), ""),
        stat("distinct project ids", projects.len(), None, ""),
        stat("active", active, Some("browse/status-active/"), ""),
        stat(
            "libraries and frameworks",
            foundations,
            Some("browse/foundations/"),
            "",
        ),
        stat("releases", releases, Some("releases/"), ""),
        stat(
            "resolved relationships",
            resolved_edges,
            Some("dependencies/"),
            "",
        ),
        stat(
            "capabilities",
            network.capabilities.len(),
            Some("capabilities/"),
            "",
        ),
        stat("unresolved targets", network.gaps.len(), Some("gaps/"), ""),
        stat("stale claims", stale, Some("health/"), warn(stale)),
        stat(
            "unavailable sites",
            unavailable,
            Some("health/"),
            warn(unavailable),
        ),
        stat(
            "identity conflicts",
            network.conflicts.len(),
            Some("conflicts/"),
            if network.conflicts.is_empty() {
                "ok"
            } else {
                "danger"
            },
        ),
    ]
}

pub fn status_counts(network: &Network) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for claim in network.claims.values() {
        *counts
            .entry(claim.record.health.token().to_owned())
            .or_default() += 1;
    }
    counts.into_iter().collect()
}

pub fn release_status_counts(manifest: &Manifest) -> String {
    let yanked = manifest
        .releases
        .iter()
        .filter(|release| release.status == ReleaseStatus::Yanked)
        .count();
    let total = plural(manifest.releases.len(), "release", "releases");
    if yanked == 0 {
        total
    } else {
        format!("{total}, {yanked} yanked")
    }
}
