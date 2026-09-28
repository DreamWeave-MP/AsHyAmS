//! What changed between two manifests of one claim, as typed facts.
//!
//! Pretty-printed JSON diffs are useless here: key order, whitespace and array position would all
//! show up as "changes", and the things that matter (a stable channel moving, a dependency
//! tightening, an artifact's bytes changing under an unchanged version) would be lines in a wall
//! of text. The protocol is structured. This compares its meaning.
//!
//! No change is interpreted. A runtime requirement moving from `>=0.50` to `>=0.51` is reported
//! as exactly that. Whether it breaks anybody is for the publisher's own `notes.breaking` to say,
//! and those are carried along verbatim.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use serde::{Deserialize, Serialize};

use crate::{
    protocol::{
        Artifact, Manifest, Notes, ProjectId, Relationship, RelationshipKind, Release,
        ReleaseStatus, Target,
    },
    version::Version,
};

/// What a relationship points at, as stored in an event: enough to render and to link.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// The publisher's display name for the target, or the capability itself.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl TargetRef {
    fn of(relationship: &Relationship) -> Self {
        let name = relationship
            .name
            .clone()
            .or_else(|| relationship.capability.clone())
            .or_else(|| relationship.project.as_ref().map(ToString::to_string))
            .unwrap_or_default();
        Self {
            project: relationship.project.clone(),
            capability: relationship.capability.clone(),
            name,
            url: relationship.url.clone(),
        }
    }
}

/// A difference in what one release contains or needs, either between two releases (a channel
/// head moving) or within one release that was republished (an amendment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ReleaseChange {
    Runtime {
        runtime: String,
        before: Option<String>,
        after: Option<String>,
    },
    LuaApi {
        before: Option<String>,
        after: Option<String>,
    },
    Platforms {
        added: Vec<String>,
        removed: Vec<String>,
    },
    CapabilityAdded {
        capability: String,
    },
    CapabilityRemoved {
        capability: String,
    },
    RelationshipAdded {
        kind: RelationshipKind,
        target: TargetRef,
        version: Option<String>,
    },
    RelationshipRemoved {
        kind: RelationshipKind,
        target: TargetRef,
        version: Option<String>,
    },
    RelationshipConstraint {
        kind: RelationshipKind,
        target: TargetRef,
        before: Option<String>,
        after: Option<String>,
    },
    RelationshipKind {
        target: TargetRef,
        before: RelationshipKind,
        after: RelationshipKind,
    },
    ComponentAdded {
        component: String,
        name: String,
    },
    ComponentRemoved {
        component: String,
        name: String,
    },
    /// Required, default, group or path changed: which components a client installs.
    ComponentSelection {
        component: String,
        details: Vec<String>,
    },
    GroupSelection {
        group: String,
        before: Option<String>,
        after: Option<String>,
    },
    RequiredContent {
        added: Vec<String>,
        removed: Vec<String>,
    },
    ExtensionAdded {
        namespace: String,
    },
    ExtensionRemoved {
        namespace: String,
    },
    ExtensionChanged {
        namespace: String,
    },
    CriticalExtensions {
        added: Vec<String>,
        removed: Vec<String>,
    },
    ArtifactAdded {
        artifact: String,
        filename: String,
        size: u64,
        sha256: String,
    },
    ArtifactRemoved {
        artifact: String,
        filename: String,
    },
    /// Only ever reported within one release: new bytes under an unchanged version.
    ArtifactDigest {
        artifact: String,
        before: String,
        after: String,
    },
    ArtifactSize {
        artifact: String,
        before: u64,
        after: u64,
    },
    ArtifactFile {
        artifact: String,
        details: Vec<String>,
    },
    ArtifactSourceAdded {
        artifact: String,
        url: String,
        kind: String,
    },
    ArtifactSourceRemoved {
        artifact: String,
        url: String,
        kind: String,
    },
    SignatureAdded {
        artifact: String,
        format: String,
        url: String,
    },
    SignatureRemoved {
        artifact: String,
        format: String,
        url: String,
    },
    SourceRevision {
        field: String,
        before: Option<String>,
        after: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    /// A scalar field of `project`: name, summary, type, status, versioning, game, license.
    ProjectField {
        field: String,
        before: Option<String>,
        after: Option<String>,
    },
    Tags {
        added: Vec<String>,
        removed: Vec<String>,
    },
    Maintainers {
        added: Vec<String>,
        removed: Vec<String>,
    },
    Credits {
        added: Vec<String>,
        removed: Vec<String>,
    },
    Link {
        link: String,
        before: Option<String>,
        after: Option<String>,
    },
    NexusMods {
        before: Option<String>,
        after: Option<String>,
    },
    Media {
        added: Vec<String>,
        removed: Vec<String>,
    },
    ChannelHead {
        channel: String,
        before: Option<String>,
        after: Option<String>,
        /// The new head compared with the old one.
        changes: Vec<ReleaseChange>,
    },
    ReleaseAdded {
        version: String,
        channel: String,
        date: Option<String>,
        status: ReleaseStatus,
        notes: Option<Notes>,
    },
    /// A release the index saw before is gone from the manifest. Yanked and deprecated releases
    /// stay listed by protocol, so outside the rolling development channel this is unusual.
    ReleaseRemoved {
        version: String,
        channel: String,
    },
    ReleaseStatus {
        version: String,
        channel: String,
        before: ReleaseStatus,
        after: ReleaseStatus,
        reason: Option<String>,
        replacement: Option<String>,
    },
    ReleaseChannel {
        version: String,
        before: String,
        after: String,
    },
    ReleaseDate {
        version: String,
        before: Option<String>,
        after: Option<String>,
    },
    ReleaseNotes {
        version: String,
    },
    /// A published release's contents changed without a new version.
    ReleaseAmended {
        version: String,
        channel: String,
        changes: Vec<ReleaseChange>,
    },
}

/// Compares two manifests of the same claim. Output order is deterministic: project fields,
/// channels by name, then releases newest first.
pub fn diff(before: &Manifest, after: &Manifest) -> Vec<Change> {
    let mut changes = project_changes(before, after);
    changes.extend(channel_changes(before, after));
    changes.extend(release_changes(before, after));
    changes
}

fn field(changes: &mut Vec<Change>, name: &str, before: Option<String>, after: Option<String>) {
    if before != after {
        changes.push(Change::ProjectField {
            field: name.to_owned(),
            before,
            after,
        });
    }
}

fn set_difference(before: &[String], after: &[String]) -> (Vec<String>, Vec<String>) {
    let before: BTreeSet<&String> = before.iter().collect();
    let after: BTreeSet<&String> = after.iter().collect();
    (
        after
            .difference(&before)
            .map(|item| (*item).clone())
            .collect(),
        before
            .difference(&after)
            .map(|item| (*item).clone())
            .collect(),
    )
}

fn project_changes(before: &Manifest, after: &Manifest) -> Vec<Change> {
    let (old, new) = (&before.project, &after.project);
    let mut changes = Vec::new();
    field(
        &mut changes,
        "name",
        Some(old.name.clone()),
        Some(new.name.clone()),
    );
    field(
        &mut changes,
        "summary",
        old.summary.clone(),
        new.summary.clone(),
    );
    field(
        &mut changes,
        "type",
        Some(old.project_type.token().to_owned()),
        Some(new.project_type.token().to_owned()),
    );
    field(
        &mut changes,
        "status",
        Some(old.status.token().to_owned()),
        Some(new.status.token().to_owned()),
    );
    field(
        &mut changes,
        "versioning",
        Some(old.versioning.name().to_owned()),
        Some(new.versioning.name().to_owned()),
    );
    field(
        &mut changes,
        "game",
        Some(old.game.clone()),
        Some(new.game.clone()),
    );
    field(
        &mut changes,
        "license",
        old.license.clone(),
        new.license.clone(),
    );

    project_lists(before, after, &mut changes);
    changes
}

/// Tags, people, links, integrations and media: lists and maps, compared as sets.
fn project_lists(before: &Manifest, after: &Manifest, changes: &mut Vec<Change>) {
    let (old, new) = (&before.project, &after.project);
    let (added, removed) = set_difference(&old.tags, &new.tags);
    if !added.is_empty() || !removed.is_empty() {
        changes.push(Change::Tags { added, removed });
    }
    let names = |people: &[crate::protocol::Person]| -> Vec<String> {
        people.iter().map(|person| person.name.clone()).collect()
    };
    let (added, removed) = set_difference(&names(&old.maintainers), &names(&new.maintainers));
    if !added.is_empty() || !removed.is_empty() {
        changes.push(Change::Maintainers { added, removed });
    }
    let credits = |credits: &[crate::protocol::Credit]| -> Vec<String> {
        credits
            .iter()
            .map(|credit| match &credit.role {
                Some(role) => format!("{} ({role})", credit.name),
                None => credit.name.clone(),
            })
            .collect()
    };
    let (added, removed) = set_difference(&credits(&old.credits), &credits(&new.credits));
    if !added.is_empty() || !removed.is_empty() {
        changes.push(Change::Credits { added, removed });
    }
    let links: BTreeSet<&String> = old.links.keys().chain(new.links.keys()).collect();
    for link in links {
        let (before, after) = (old.links.get(link), new.links.get(link));
        if before != after {
            changes.push(Change::Link {
                link: link.clone(),
                before: before.cloned(),
                after: after.cloned(),
            });
        }
    }
    let nexus = |manifest: &Manifest| {
        manifest
            .project
            .integrations
            .nexusmods
            .as_ref()
            .map(|nexus| format!("{}/{}", nexus.game, nexus.mod_id))
    };
    if nexus(before) != nexus(after) {
        changes.push(Change::NexusMods {
            before: nexus(before),
            after: nexus(after),
        });
    }
    let media = |project: &crate::protocol::Project| -> Vec<String> {
        project.media.iter().map(|item| item.url.clone()).collect()
    };
    let (added, removed) = set_difference(&media(old), &media(new));
    if !added.is_empty() || !removed.is_empty() {
        changes.push(Change::Media { added, removed });
    }
}

/// Finds the release with the same precedence, or failing that (the project changed its
/// versioning scheme, which the project changes already report) the same text.
fn find_release<'a>(
    manifest: &'a Manifest,
    version: &str,
    scheme_of: &Manifest,
) -> Option<&'a Release> {
    if manifest.scheme() == scheme_of.scheme() {
        let wanted = Version::parse(version, manifest.scheme()).ok()?;
        manifest.releases.iter().find(|release| {
            Version::parse(&release.version, manifest.scheme())
                .is_ok_and(|candidate| candidate.precedence(&wanted) == Ordering::Equal)
        })
    } else {
        manifest
            .releases
            .iter()
            .find(|release| release.version == version)
    }
}

fn channel_changes(before: &Manifest, after: &Manifest) -> Vec<Change> {
    let channels: BTreeSet<&String> = before
        .channels
        .keys()
        .chain(after.channels.keys())
        .collect();
    let mut changes = Vec::new();
    for channel in channels {
        let old_head = before
            .channels
            .get(channel)
            .map(|head| head.version.clone());
        let new_head = after.channels.get(channel).map(|head| head.version.clone());
        let same = match (&old_head, &new_head) {
            (Some(old), Some(new)) if before.scheme() == after.scheme() => {
                match (
                    Version::parse(old, before.scheme()),
                    Version::parse(new, after.scheme()),
                ) {
                    (Ok(old), Ok(new)) => old.precedence(&new) == Ordering::Equal,
                    _ => old == new,
                }
            }
            (old, new) => old == new,
        };
        if same {
            continue;
        }
        let old_release = old_head
            .as_deref()
            .and_then(|version| find_release(before, version, before));
        let new_release = new_head
            .as_deref()
            .and_then(|version| find_release(after, version, after));
        let release_changes = match (old_release, new_release) {
            (Some(old), Some(new)) => compare_releases(old, new, false),
            _ => Vec::new(),
        };
        changes.push(Change::ChannelHead {
            channel: channel.clone(),
            before: old_head,
            after: new_head,
            changes: release_changes,
        });
    }
    changes
}

fn release_changes(before: &Manifest, after: &Manifest) -> Vec<Change> {
    let mut changes = Vec::new();
    for release in &after.releases {
        let Some(old) = find_release(before, &release.version, after) else {
            changes.push(Change::ReleaseAdded {
                version: release.version.clone(),
                channel: release.channel.clone(),
                date: release.date.clone(),
                status: release.status,
                notes: release.notes.clone(),
            });
            continue;
        };
        if old.status != release.status {
            let notice = release.yanked.as_ref().or(release.deprecated.as_ref());
            changes.push(Change::ReleaseStatus {
                version: release.version.clone(),
                channel: release.channel.clone(),
                before: old.status,
                after: release.status,
                reason: notice.map(|notice| notice.reason.clone()),
                replacement: notice.and_then(|notice| notice.replacement.clone()),
            });
        }
        if old.channel != release.channel {
            changes.push(Change::ReleaseChannel {
                version: release.version.clone(),
                before: old.channel.clone(),
                after: release.channel.clone(),
            });
        }
        if old.date != release.date {
            changes.push(Change::ReleaseDate {
                version: release.version.clone(),
                before: old.date.clone(),
                after: release.date.clone(),
            });
        }
        if old.notes != release.notes {
            changes.push(Change::ReleaseNotes {
                version: release.version.clone(),
            });
        }
        let amendments = compare_releases(old, release, true);
        if !amendments.is_empty() {
            changes.push(Change::ReleaseAmended {
                version: release.version.clone(),
                channel: release.channel.clone(),
                changes: amendments,
            });
        }
    }
    for release in &before.releases {
        if find_release(after, &release.version, before).is_none() {
            changes.push(Change::ReleaseRemoved {
                version: release.version.clone(),
                channel: release.channel.clone(),
            });
        }
    }
    changes
}

/// Compares what two releases contain and need. `same_release` is true for an amendment, where
/// changed artifact bytes are the whole story; between two versions they are expected.
pub fn compare_releases(old: &Release, new: &Release, same_release: bool) -> Vec<ReleaseChange> {
    let mut changes = Vec::new();

    let runtimes: BTreeSet<&String> = old.runtimes.keys().chain(new.runtimes.keys()).collect();
    for runtime in runtimes {
        let (before, after) = (old.runtimes.get(runtime), new.runtimes.get(runtime));
        if before != after {
            changes.push(ReleaseChange::Runtime {
                runtime: runtime.clone(),
                before: before.cloned(),
                after: after.cloned(),
            });
        }
    }
    let (old_openmw, new_openmw) = (old.openmw(), new.openmw());
    let lua_api = |openmw: &Option<crate::protocol::OpenmwExtension>| {
        openmw.as_ref().and_then(|openmw| openmw.lua_api.clone())
    };
    if lua_api(&old_openmw) != lua_api(&new_openmw) {
        changes.push(ReleaseChange::LuaApi {
            before: lua_api(&old_openmw),
            after: lua_api(&new_openmw),
        });
    }
    let platforms = |release: &Release| -> Vec<String> {
        release
            .platforms
            .iter()
            .map(|platform| format!("{}/{}", platform.os, platform.arch))
            .collect()
    };
    let (added, removed) = set_difference(&platforms(old), &platforms(new));
    if !added.is_empty() || !removed.is_empty() {
        changes.push(ReleaseChange::Platforms { added, removed });
    }

    let (added, removed) = set_difference(&old.provides, &new.provides);
    changes.extend(
        added
            .into_iter()
            .map(|capability| ReleaseChange::CapabilityAdded { capability }),
    );
    changes.extend(
        removed
            .into_iter()
            .map(|capability| ReleaseChange::CapabilityRemoved { capability }),
    );

    changes.extend(relationship_changes(&old.relationships, &new.relationships));
    component_changes(old, new, &mut changes);

    let content = |openmw: &Option<crate::protocol::OpenmwExtension>| -> Vec<String> {
        openmw
            .as_ref()
            .map(|openmw| openmw.requires_content.clone())
            .unwrap_or_default()
    };
    let (added, removed) = set_difference(&content(&old_openmw), &content(&new_openmw));
    if !added.is_empty() || !removed.is_empty() {
        changes.push(ReleaseChange::RequiredContent { added, removed });
    }

    let namespaces: BTreeSet<&String> =
        old.extensions.keys().chain(new.extensions.keys()).collect();
    for namespace in namespaces {
        match (old.extensions.get(namespace), new.extensions.get(namespace)) {
            (None, Some(_)) => changes.push(ReleaseChange::ExtensionAdded {
                namespace: namespace.clone(),
            }),
            (Some(_), None) => changes.push(ReleaseChange::ExtensionRemoved {
                namespace: namespace.clone(),
            }),
            // The OpenMW extension's meaningful parts are reported above; the rest (data
            // directories, content files, settings) is install detail.
            (Some(before), Some(after)) if before != after && namespace != "openmw" => {
                changes.push(ReleaseChange::ExtensionChanged {
                    namespace: namespace.clone(),
                });
            }
            _ => {}
        }
    }
    let (added, removed) = set_difference(&old.critical_extensions, &new.critical_extensions);
    if !added.is_empty() || !removed.is_empty() {
        changes.push(ReleaseChange::CriticalExtensions { added, removed });
    }

    artifact_changes(&old.artifacts, &new.artifacts, same_release, &mut changes);

    if same_release {
        let fields = [
            ("revision", &old.source.revision, &new.source.revision),
            ("tag", &old.source.tag, &new.source.tag),
        ];
        for (field, before, after) in fields {
            if before != after {
                changes.push(ReleaseChange::SourceRevision {
                    field: field.to_owned(),
                    before: before.clone(),
                    after: after.clone(),
                });
            }
        }
    }
    changes
}

fn relationship_changes(old: &[Relationship], new: &[Relationship]) -> Vec<ReleaseChange> {
    let group = |relationships: &[Relationship]| {
        let mut grouped: BTreeMap<Target, Vec<Relationship>> = BTreeMap::new();
        for relationship in relationships {
            grouped
                .entry(relationship.target())
                .or_default()
                .push(relationship.clone());
        }
        grouped
    };
    let (old, new) = (group(old), group(new));
    let targets: BTreeSet<&Target> = old.keys().chain(new.keys()).collect();
    let empty = Vec::new();
    let mut changes = Vec::new();
    for target in targets {
        let before = old.get(target).unwrap_or(&empty);
        let after = new.get(target).unwrap_or(&empty);
        if let ([previous], [current]) = (before.as_slice(), after.as_slice())
            && previous.kind != current.kind
        {
            changes.push(ReleaseChange::RelationshipKind {
                target: TargetRef::of(current),
                before: previous.kind,
                after: current.kind,
            });
            if previous.version != current.version {
                changes.push(ReleaseChange::RelationshipConstraint {
                    kind: current.kind,
                    target: TargetRef::of(current),
                    before: previous.version.clone(),
                    after: current.version.clone(),
                });
            }
            continue;
        }
        for kind in RelationshipKind::ALL {
            let previous = before.iter().find(|relationship| relationship.kind == kind);
            let current = after.iter().find(|relationship| relationship.kind == kind);
            match (previous, current) {
                (None, Some(current)) => changes.push(ReleaseChange::RelationshipAdded {
                    kind,
                    target: TargetRef::of(current),
                    version: current.version.clone(),
                }),
                (Some(previous), None) => changes.push(ReleaseChange::RelationshipRemoved {
                    kind,
                    target: TargetRef::of(previous),
                    version: previous.version.clone(),
                }),
                (Some(previous), Some(current)) if previous.version != current.version => {
                    changes.push(ReleaseChange::RelationshipConstraint {
                        kind,
                        target: TargetRef::of(current),
                        before: previous.version.clone(),
                        after: current.version.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    changes
}

fn component_changes(old: &Release, new: &Release, changes: &mut Vec<ReleaseChange>) {
    for component in &new.components {
        let Some(previous) = old.components.iter().find(|item| item.id == component.id) else {
            changes.push(ReleaseChange::ComponentAdded {
                component: component.id.clone(),
                name: component.name.clone(),
            });
            continue;
        };
        let mut details = Vec::new();
        if previous.required != component.required {
            details.push(format!(
                "required: {} → {}",
                previous.required, component.required
            ));
        }
        if previous.default != component.default {
            details.push(format!(
                "default: {} → {}",
                previous.default, component.default
            ));
        }
        if previous.group != component.group {
            details.push(format!(
                "group: {} → {}",
                previous.group.as_deref().unwrap_or("none"),
                component.group.as_deref().unwrap_or("none")
            ));
        }
        if previous.path != component.path {
            details.push(format!("path: {} → {}", previous.path, component.path));
        }
        if previous.requires != component.requires || previous.conflicts != component.conflicts {
            details.push("rules between components changed".to_owned());
        }
        if !details.is_empty() {
            changes.push(ReleaseChange::ComponentSelection {
                component: component.id.clone(),
                details,
            });
        }
    }
    for component in &old.components {
        if !new.components.iter().any(|item| item.id == component.id) {
            changes.push(ReleaseChange::ComponentRemoved {
                component: component.id.clone(),
                name: component.name.clone(),
            });
        }
    }
    let groups: BTreeSet<&String> = old
        .groups
        .iter()
        .chain(&new.groups)
        .map(|group| &group.id)
        .collect();
    for group in groups {
        let select = |release: &Release| {
            release
                .groups
                .iter()
                .find(|item| &item.id == group)
                .map(|item| item.select.clone())
        };
        if select(old) != select(new) {
            changes.push(ReleaseChange::GroupSelection {
                group: group.clone(),
                before: select(old),
                after: select(new),
            });
        }
    }
}

fn artifact_changes(
    old: &[Artifact],
    new: &[Artifact],
    same_release: bool,
    changes: &mut Vec<ReleaseChange>,
) {
    for artifact in new {
        let Some(previous) = old.iter().find(|item| item.id == artifact.id) else {
            changes.push(ReleaseChange::ArtifactAdded {
                artifact: artifact.id.clone(),
                filename: artifact.filename.clone(),
                size: artifact.size,
                sha256: artifact.digests.sha256.clone(),
            });
            continue;
        };
        if same_release && previous.digests.sha256 != artifact.digests.sha256 {
            changes.push(ReleaseChange::ArtifactDigest {
                artifact: artifact.id.clone(),
                before: previous.digests.sha256.clone(),
                after: artifact.digests.sha256.clone(),
            });
        }
        if previous.size != artifact.size {
            changes.push(ReleaseChange::ArtifactSize {
                artifact: artifact.id.clone(),
                before: previous.size,
                after: artifact.size,
            });
        }
        let mut details = Vec::new();
        if previous.format != artifact.format {
            details.push(format!("format: {} → {}", previous.format, artifact.format));
        }
        if same_release && previous.filename != artifact.filename {
            details.push(format!(
                "file name: {} → {}",
                previous.filename, artifact.filename
            ));
        }
        if !details.is_empty() {
            changes.push(ReleaseChange::ArtifactFile {
                artifact: artifact.id.clone(),
                details,
            });
        }
        if same_release {
            source_changes(previous, artifact, changes);
        }
    }
    for artifact in old {
        if !new.iter().any(|item| item.id == artifact.id) {
            changes.push(ReleaseChange::ArtifactRemoved {
                artifact: artifact.id.clone(),
                filename: artifact.filename.clone(),
            });
        }
    }
}

fn source_changes(old: &Artifact, new: &Artifact, changes: &mut Vec<ReleaseChange>) {
    let kind = |kind: crate::protocol::SourceKind| {
        match kind {
            crate::protocol::SourceKind::Publisher => "publisher",
            crate::protocol::SourceKind::Mirror => "mirror",
        }
        .to_owned()
    };
    for source in &new.sources {
        if !old.sources.iter().any(|item| item.url == source.url) {
            changes.push(ReleaseChange::ArtifactSourceAdded {
                artifact: new.id.clone(),
                url: source.url.clone(),
                kind: kind(source.kind),
            });
        }
    }
    for source in &old.sources {
        if !new.sources.iter().any(|item| item.url == source.url) {
            changes.push(ReleaseChange::ArtifactSourceRemoved {
                artifact: new.id.clone(),
                url: source.url.clone(),
                kind: kind(source.kind),
            });
        }
    }
    for signature in &new.signatures {
        if !old.signatures.iter().any(|item| item.url == signature.url) {
            changes.push(ReleaseChange::SignatureAdded {
                artifact: new.id.clone(),
                format: signature.format.clone(),
                url: signature.url.clone(),
            });
        }
    }
    for signature in &old.signatures {
        if !new.signatures.iter().any(|item| item.url == signature.url) {
            changes.push(ReleaseChange::SignatureRemoved {
                artifact: new.id.clone(),
                format: signature.format.clone(),
                url: signature.url.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::protocol::parse::parse_manifest;

    const CANDLELIGHT: &str = include_str!("../tests/fixtures/mod-template/candlelight.json");

    fn base() -> Value {
        serde_json::from_str(CANDLELIGHT).unwrap()
    }

    fn manifest(value: &Value) -> Manifest {
        parse_manifest(&serde_json::to_vec(value).unwrap()).unwrap()
    }

    fn changes(edit: impl FnOnce(&mut Value)) -> Vec<Change> {
        let before = base();
        let mut after = base();
        edit(&mut after);
        diff(&manifest(&before), &manifest(&after))
    }

    /// Copies Candlelight 1.1.0 into a new stable release, newest of all, and edits the copy.
    fn with_new_stable(after: &mut Value, version: &str, edit: impl FnOnce(&mut Value)) {
        let mut release = after["releases"][1].clone();
        release["version"] = version.into();
        release["date"] = "2026-10-01".into();
        edit(&mut release);
        after["releases"].as_array_mut().unwrap().insert(0, release);
        after["channels"]["stable"]["version"] = version.into();
    }

    #[test]
    fn an_unchanged_manifest_has_no_changes() {
        assert!(changes(|_| {}).is_empty());
    }

    #[test]
    fn key_order_and_formatting_are_not_changes() {
        let before = manifest(&base());
        let reordered: Value = serde_json::from_str(
            &serde_json::to_string(
                &serde_json::from_str::<serde_json::Map<String, Value>>(CANDLELIGHT).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(diff(&before, &manifest(&reordered)).is_empty());
    }

    #[test]
    fn a_new_release_moves_its_channel_and_carries_its_notes() {
        let found = changes(|after| {
            with_new_stable(after, "1.2.0", |release| {
                release["runtimes"]["openmw"] = ">=0.50".into();
                release["notes"] = json!({
                    "summary": "Storm lanterns.",
                    "breaking": ["Removes the old light schedule setting."],
                    "migration": "Delete `candlelight.cfg` before updating."
                });
            });
        });
        let Change::ChannelHead {
            channel,
            before,
            after,
            changes,
        } = &found[0]
        else {
            panic!("{found:?}")
        };
        assert_eq!(channel, "stable");
        assert_eq!(before.as_deref(), Some("1.1.0"));
        assert_eq!(after.as_deref(), Some("1.2.0"));
        assert_eq!(
            changes[0],
            ReleaseChange::Runtime {
                runtime: "openmw".to_owned(),
                before: Some(">=0.49".to_owned()),
                after: Some(">=0.50".to_owned()),
            }
        );
        let Change::ReleaseAdded { version, notes, .. } = &found[1] else {
            panic!("{found:?}")
        };
        assert_eq!(version, "1.2.0");
        let notes = notes.as_ref().unwrap();
        assert_eq!(notes.breaking.as_ref().unwrap().len(), 1);
        assert!(notes.migration.is_some());
    }

    #[test]
    fn dependencies_added_removed_and_tightened() {
        let found = changes(|after| {
            with_new_stable(after, "1.2.0", |release| {
                release["relationships"][0]["version"] = ">=1.1".into();
                release["relationships"].as_array_mut().unwrap().remove(1);
                release["relationships"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "kind": "requires",
                        "project": "0e8a52b1-5d7f-4c1e-9a2b-6f3c8d1e4a70",
                        "name": "Lantern Physics",
                        "version": ">=2.0"
                    }));
            });
        });
        let Change::ChannelHead { changes, .. } = &found[0] else {
            panic!()
        };
        assert!(changes.iter().any(|change| matches!(change,
            ReleaseChange::RelationshipConstraint { kind: RelationshipKind::Requires, before, after, .. }
                if before.as_deref() == Some(">=1.0") && after.as_deref() == Some(">=1.1"))));
        assert!(changes.iter().any(|change| matches!(change,
            ReleaseChange::RelationshipRemoved { kind: RelationshipKind::Recommends, target, .. }
                if target.name == "Tamriel Rebuilt")));
        assert!(changes.iter().any(|change| matches!(change,
            ReleaseChange::RelationshipAdded { kind: RelationshipKind::Requires, target, version }
                if target.name == "Lantern Physics" && version.as_deref() == Some(">=2.0"))));
    }

    #[test]
    fn a_relationship_changing_kind_is_one_change() {
        let found = changes(|after| {
            with_new_stable(after, "1.2.0", |release| {
                release["relationships"][0]["kind"] = "recommends".into();
            });
        });
        let Change::ChannelHead { changes, .. } = &found[0] else {
            panic!()
        };
        assert_eq!(
            changes
                .iter()
                .filter(|change| matches!(
                    change,
                    ReleaseChange::RelationshipKind {
                        before: RelationshipKind::Requires,
                        after: RelationshipKind::Recommends,
                        ..
                    }
                ))
                .count(),
            1,
            "{changes:?}"
        );
        assert!(!changes.iter().any(|change| matches!(
            change,
            ReleaseChange::RelationshipAdded { .. } | ReleaseChange::RelationshipRemoved { .. }
        )));
    }

    #[test]
    fn capabilities_and_components() {
        let found = changes(|after| {
            with_new_stable(after, "1.2.0", |release| {
                release["provides"] =
                    json!(["dreamweave:dynamic-lights", "dreamweave:weather-lights"]);
                release["components"][3]["default"] = true.into();
                release["components"][2]["default"] = false.into();
                release["components"].as_array_mut().unwrap().remove(1);
                let openmw = release["extensions"]["openmw"]["components"]
                    .as_object_mut()
                    .unwrap();
                openmw.remove("tamriel-rebuilt");
            });
        });
        let Change::ChannelHead { changes, .. } = &found[0] else {
            panic!()
        };
        assert!(changes.contains(&ReleaseChange::CapabilityAdded {
            capability: "dreamweave:weather-lights".to_owned()
        }));
        assert!(changes.iter().any(|change| matches!(change, ReleaseChange::ComponentRemoved { component, .. } if component == "tamriel-rebuilt")));
        assert!(changes.iter().any(|change| matches!(change, ReleaseChange::ComponentSelection { component, .. } if component == "flames-4k")));
    }

    #[test]
    fn a_yanked_release_stays_and_says_why() {
        let found = changes(|after| {
            after["releases"][2]["status"] = "yanked".into();
            after["releases"][2]["yanked"] =
                json!({ "reason": "Corrupts saves.", "replacement": "1.1.0" });
        });
        assert_eq!(
            found,
            vec![Change::ReleaseStatus {
                version: "1.0.0".to_owned(),
                channel: "stable".to_owned(),
                before: ReleaseStatus::Available,
                after: ReleaseStatus::Yanked,
                reason: Some("Corrupts saves.".to_owned()),
                replacement: Some("1.1.0".to_owned()),
            }]
        );
    }

    #[test]
    fn a_deprecated_head_moves_the_channel_back() {
        let found = changes(|after| {
            after["releases"][1]["status"] = "deprecated".into();
            after["releases"][1]["deprecated"] = json!({ "reason": "Use the development build." });
            after["channels"]["stable"]["version"] = "1.0.0".into();
        });
        assert!(
            matches!(&found[0], Change::ChannelHead { before, after, .. }
            if before.as_deref() == Some("1.1.0") && after.as_deref() == Some("1.0.0"))
        );
        assert!(matches!(
            &found[1],
            Change::ReleaseStatus {
                after: ReleaseStatus::Deprecated,
                ..
            }
        ));
    }

    #[test]
    fn a_vanished_release_is_reported() {
        let found = changes(|after| {
            after["releases"].as_array_mut().unwrap().remove(2);
        });
        assert_eq!(
            found,
            vec![Change::ReleaseRemoved {
                version: "1.0.0".to_owned(),
                channel: "stable".to_owned()
            }]
        );
    }

    #[test]
    fn new_bytes_under_the_same_version_are_an_amendment() {
        let found = changes(|after| {
            after["releases"][1]["artifacts"][0]["digests"]["sha256"] = "0".repeat(64).into();
            after["releases"][1]["artifacts"][0]["size"] = 1234.into();
            after["releases"][1]["artifacts"][0]["sources"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "url": "https://mirror.example.org/by-sha256/0000.zip", "kind": "mirror"
                }));
        });
        let Change::ReleaseAmended {
            version, changes, ..
        } = &found[0]
        else {
            panic!("{found:?}")
        };
        assert_eq!(version, "1.1.0");
        assert!(
            matches!(&changes[0], ReleaseChange::ArtifactDigest { after, .. } if after == &"0".repeat(64))
        );
        assert!(
            changes
                .iter()
                .any(|change| matches!(change, ReleaseChange::ArtifactSize { after: 1234, .. }))
        );
        assert!(changes.iter().any(|change| matches!(change, ReleaseChange::ArtifactSourceAdded { kind, .. } if kind == "mirror")));
    }

    #[test]
    fn a_development_build_replaced_is_a_head_move_plus_add_and_remove() {
        let found = changes(|after| {
            after["releases"][0]["version"] = "1.1.1-dev.1".into();
            after["channels"]["development"]["version"] = "1.1.1-dev.1".into();
        });
        assert!(
            matches!(&found[0], Change::ChannelHead { channel, .. } if channel == "development")
        );
        assert!(
            matches!(&found[1], Change::ReleaseAdded { version, .. } if version == "1.1.1-dev.1")
        );
        assert!(
            matches!(&found[2], Change::ReleaseRemoved { version, channel } if version == "1.1.1-dev.0" && channel == "development")
        );
    }

    #[test]
    fn project_metadata_changes() {
        let found = changes(|after| {
            after["project"]["name"] = "Candlelight Redux".into();
            after["project"]["status"] = "maintenance".into();
            after["project"]["tags"] = json!(["Lighting", "OpenMW", "Weather"]);
            after["project"]["links"]["issues"] = "https://example.org/issues".into();
        });
        assert!(found.contains(&Change::ProjectField {
            field: "name".to_owned(),
            before: Some("Candlelight".to_owned()),
            after: Some("Candlelight Redux".to_owned()),
        }));
        assert!(found.contains(&Change::Tags {
            added: vec!["Weather".to_owned()],
            removed: vec!["Lua".to_owned()],
        }));
        assert!(
            found
                .iter()
                .any(|change| matches!(change, Change::Link { link, .. } if link == "issues"))
        );
    }
}
