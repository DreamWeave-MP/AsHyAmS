//! Reading events: what kind of change each one is, and the same changes as lines a person can
//! scan. Tags are computed when the site is built, not stored, so a better classification
//! applies to the whole history on the next build.
//!
//! Nothing here interprets. A dependency tightening is reported as a dependency tightening; it
//! is never called breaking unless the publisher's own notes say so. The only judgement made is
//! which facts are unusual enough to surface first, and those are named for what they are.

use std::{collections::BTreeSet, fmt::Write as _};

use serde::Serialize;

use crate::{
    diff::{Change, ReleaseChange, TargetRef},
    protocol::{DEVELOPMENT_CHANNEL, ReleaseStatus},
    state::{Event, EventKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tag {
    Release,
    Breaking,
    Migration,
    Dependencies,
    Runtime,
    Status,
    Artifacts,
    Anomaly,
    Capabilities,
    Components,
    Metadata,
    Development,
    Observed,
    Listing,
    Moved,
}

impl Tag {
    /// In the order the updates page offers them.
    pub const ALL: [Self; 15] = [
        Self::Release,
        Self::Breaking,
        Self::Migration,
        Self::Dependencies,
        Self::Runtime,
        Self::Status,
        Self::Artifacts,
        Self::Anomaly,
        Self::Capabilities,
        Self::Components,
        Self::Metadata,
        Self::Development,
        Self::Observed,
        Self::Listing,
        Self::Moved,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Breaking => "breaking",
            Self::Migration => "migration",
            Self::Dependencies => "dependencies",
            Self::Runtime => "runtime",
            Self::Status => "status",
            Self::Artifacts => "artifacts",
            Self::Anomaly => "anomaly",
            Self::Capabilities => "capabilities",
            Self::Components => "components",
            Self::Metadata => "metadata",
            Self::Development => "development",
            Self::Observed => "observed",
            Self::Listing => "listing",
            Self::Moved => "moved",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Release => "Releases",
            Self::Breaking => "Breaking notes",
            Self::Migration => "Migration notes",
            Self::Dependencies => "Dependencies",
            Self::Runtime => "Runtimes and platforms",
            Self::Status => "Yanks and deprecations",
            Self::Artifacts => "Artifacts",
            Self::Anomaly => "Anomalies",
            Self::Capabilities => "Capabilities",
            Self::Components => "Components",
            Self::Metadata => "Project metadata",
            Self::Development => "Development builds",
            Self::Observed => "First observations",
            Self::Listing => "Withdrawn and restored",
            Self::Moved => "Host moves",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Release => "A release outside the rolling development channel appeared.",
            Self::Breaking => "A new release whose publisher lists breaking changes in its notes.",
            Self::Migration => "A new release whose publisher wrote migration instructions.",
            Self::Dependencies => {
                "A current release's requires, recommends, conflicts, compatible or replaces changed."
            }
            Self::Runtime => {
                "Runtime requirements, the Lua API, platforms, required content or critical extensions changed."
            }
            Self::Status => "A release was yanked, deprecated, or restored to available.",
            Self::Artifacts => "Artifacts, their sources, signatures or source revisions changed.",
            Self::Anomaly => {
                "Unusual publication facts: bytes changed under an unchanged version, a release vanished, the versioning scheme changed."
            }
            Self::Capabilities => "A current release started or stopped providing a capability.",
            Self::Components => "Installable components or their selection rules changed.",
            Self::Metadata => {
                "Name, summary, status, tags, links, people, media or release dates changed."
            }
            Self::Development => "The rolling development build moved. Expected on every push.",
            Self::Observed => "This index read a claim for the first time.",
            Self::Listing => "A site stopped listing a project, or listed it again.",
            Self::Moved => "A claim arrived from a site its enrolled source used to lead to.",
        }
    }
}

fn release_change_tags(changes: &[ReleaseChange], tags: &mut BTreeSet<Tag>) {
    for change in changes {
        tags.insert(match change {
            ReleaseChange::Runtime { .. }
            | ReleaseChange::LuaApi { .. }
            | ReleaseChange::Platforms { .. }
            | ReleaseChange::RequiredContent { .. }
            | ReleaseChange::CriticalExtensions { .. }
            | ReleaseChange::ExtensionAdded { .. }
            | ReleaseChange::ExtensionRemoved { .. }
            | ReleaseChange::ExtensionChanged { .. } => Tag::Runtime,
            ReleaseChange::CapabilityAdded { .. } | ReleaseChange::CapabilityRemoved { .. } => {
                Tag::Capabilities
            }
            ReleaseChange::RelationshipAdded { .. }
            | ReleaseChange::RelationshipRemoved { .. }
            | ReleaseChange::RelationshipConstraint { .. }
            | ReleaseChange::RelationshipKind { .. } => Tag::Dependencies,
            ReleaseChange::ComponentAdded { .. }
            | ReleaseChange::ComponentRemoved { .. }
            | ReleaseChange::ComponentSelection { .. }
            | ReleaseChange::GroupSelection { .. } => Tag::Components,
            ReleaseChange::ArtifactAdded { .. }
            | ReleaseChange::ArtifactRemoved { .. }
            | ReleaseChange::ArtifactDigest { .. }
            | ReleaseChange::ArtifactSize { .. }
            | ReleaseChange::ArtifactFile { .. }
            | ReleaseChange::ArtifactSourceAdded { .. }
            | ReleaseChange::ArtifactSourceRemoved { .. }
            | ReleaseChange::SignatureAdded { .. }
            | ReleaseChange::SignatureRemoved { .. }
            | ReleaseChange::SourceRevision { .. } => Tag::Artifacts,
        });
    }
}

/// An amendment that changed bytes under a version that is not a rolling development build.
fn digest_anomaly(channel: &str, changes: &[ReleaseChange]) -> bool {
    channel != DEVELOPMENT_CHANNEL
        && changes
            .iter()
            .any(|change| matches!(change, ReleaseChange::ArtifactDigest { .. }))
}

pub fn tags(event: &Event) -> BTreeSet<Tag> {
    let mut tags = BTreeSet::new();
    match event.kind {
        EventKind::Observed if event.moved_from.is_none() => {
            tags.insert(Tag::Observed);
            return tags;
        }
        EventKind::Withdrawn | EventKind::Restored => {
            tags.insert(Tag::Listing);
        }
        _ => {}
    }
    if event.moved_from.is_some() {
        tags.insert(Tag::Moved);
    }
    for change in &event.changes {
        match change {
            Change::ProjectField { field, .. } if field == "versioning" => {
                tags.insert(Tag::Anomaly);
                tags.insert(Tag::Metadata);
            }
            Change::ProjectField { .. }
            | Change::Tags { .. }
            | Change::Maintainers { .. }
            | Change::Credits { .. }
            | Change::Link { .. }
            | Change::NexusMods { .. }
            | Change::Media { .. }
            | Change::ReleaseDate { .. }
            | Change::ReleaseNotes { .. }
            | Change::ReleaseChannel { .. } => {
                tags.insert(Tag::Metadata);
            }
            Change::ChannelHead {
                channel, changes, ..
            } => {
                if channel == DEVELOPMENT_CHANNEL {
                    tags.insert(Tag::Development);
                }
                release_change_tags(changes, &mut tags);
            }
            Change::ReleaseAdded { channel, notes, .. } => {
                if channel == DEVELOPMENT_CHANNEL {
                    tags.insert(Tag::Development);
                } else {
                    tags.insert(Tag::Release);
                }
                if let Some(notes) = notes {
                    if notes
                        .breaking
                        .as_ref()
                        .is_some_and(|items| !items.is_empty())
                    {
                        tags.insert(Tag::Breaking);
                    }
                    if notes.migration.is_some() {
                        tags.insert(Tag::Migration);
                    }
                }
            }
            Change::ReleaseRemoved { channel, .. } => {
                tags.insert(if channel == DEVELOPMENT_CHANNEL {
                    Tag::Development
                } else {
                    Tag::Anomaly
                });
            }
            Change::ReleaseStatus { .. } => {
                tags.insert(Tag::Status);
            }
            Change::ReleaseAmended {
                channel, changes, ..
            } => {
                if channel == DEVELOPMENT_CHANNEL {
                    tags.insert(Tag::Development);
                }
                if digest_anomaly(channel, changes) {
                    tags.insert(Tag::Anomaly);
                }
                release_change_tags(changes, &mut tags);
            }
        }
    }
    tags
}

/// Nothing but the development build moving: kept out of the front page's recent changes.
pub fn is_development_only(event: &Event) -> bool {
    let tags = tags(event);
    tags.contains(&Tag::Development)
        && tags.iter().all(|tag| {
            matches!(
                tag,
                Tag::Development | Tag::Artifacts | Tag::Components | Tag::Runtime | Tag::Dependencies | Tag::Capabilities
            )
        })
        && !event.changes.iter().any(|change| {
            matches!(change, Change::ChannelHead { channel, .. } if channel != DEVELOPMENT_CHANNEL)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineClass {
    Added,
    Removed,
    Changed,
    Anomaly,
    Note,
}

/// One fact, ready for a template: a subject, optionally a before and an after, a detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    pub class: LineClass,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// A target inside the network, as a relationship's project id; the site resolves it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetRef>,
}

impl Line {
    fn new(class: LineClass, subject: impl Into<String>) -> Self {
        Self {
            class,
            subject: subject.into(),
            before: None,
            after: None,
            detail: None,
            target: None,
        }
    }

    fn values(mut self, before: Option<String>, after: Option<String>) -> Self {
        self.before = before;
        self.after = after;
        self
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn target(mut self, target: &TargetRef) -> Self {
        self.target = Some(target.clone());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Section {
    pub title: String,
    pub lines: Vec<Line>,
}

fn runtime_label(runtime: &str) -> String {
    match runtime {
        "openmw" => "OpenMW".to_owned(),
        "mwse" => "MWSE".to_owned(),
        "tes3mp" => "TES3MP".to_owned(),
        "morrowind" => "Morrowind.exe".to_owned(),
        other => other.to_owned(),
    }
}

fn list(items: &[String]) -> String {
    items.join(", ")
}

fn added_removed(subject: &str, added: &[String], removed: &[String]) -> Vec<Line> {
    let mut lines = Vec::new();
    if !added.is_empty() {
        lines.push(Line::new(LineClass::Added, subject).detail(list(added)));
    }
    if !removed.is_empty() {
        lines.push(Line::new(LineClass::Removed, subject).detail(list(removed)));
    }
    lines
}

/// Runtime, Lua API, platform, content and extension changes.
fn runtime_lines(change: &ReleaseChange) -> Option<Vec<Line>> {
    Some(match change {
        ReleaseChange::Runtime {
            runtime,
            before,
            after,
        } => vec![
            Line::new(
                LineClass::Changed,
                format!("{} runtime", runtime_label(runtime)),
            )
            .values(before.clone(), after.clone()),
        ],
        ReleaseChange::LuaApi { before, after } => {
            vec![
                Line::new(LineClass::Changed, "OpenMW Lua API")
                    .values(before.clone(), after.clone()),
            ]
        }
        ReleaseChange::Platforms { added, removed } => added_removed("Platforms", added, removed),
        ReleaseChange::RequiredContent { added, removed } => {
            added_removed("Required content files", added, removed)
        }
        ReleaseChange::CriticalExtensions { added, removed } => {
            added_removed("Critical extensions", added, removed)
        }
        ReleaseChange::ExtensionAdded { namespace } => {
            vec![Line::new(
                LineClass::Added,
                format!("Extension {namespace}"),
            )]
        }
        ReleaseChange::ExtensionRemoved { namespace } => {
            vec![Line::new(
                LineClass::Removed,
                format!("Extension {namespace}"),
            )]
        }
        ReleaseChange::ExtensionChanged { namespace } => vec![
            Line::new(LineClass::Changed, format!("Extension {namespace}"))
                .detail("its data changed"),
        ],
        _ => return None,
    })
}

fn dependency_line(change: &ReleaseChange) -> Option<Line> {
    Some(match change {
        ReleaseChange::RelationshipAdded {
            kind,
            target,
            version,
        } => Line::new(
            LineClass::Added,
            format!("{} {}", kind.token(), target.name),
        )
        .values(None, version.clone())
        .target(target),
        ReleaseChange::RelationshipRemoved {
            kind,
            target,
            version,
        } => Line::new(
            LineClass::Removed,
            format!("{} {}", kind.token(), target.name),
        )
        .values(version.clone(), None)
        .target(target),
        ReleaseChange::RelationshipConstraint {
            kind,
            target,
            before,
            after,
        } => {
            let any = || "any version".to_owned();
            Line::new(
                LineClass::Changed,
                format!("{} {}", kind.token(), target.name),
            )
            .values(
                Some(before.clone().unwrap_or_else(any)),
                Some(after.clone().unwrap_or_else(any)),
            )
            .target(target)
        }
        ReleaseChange::RelationshipKind {
            target,
            before,
            after,
        } => Line::new(
            LineClass::Changed,
            format!("Relationship to {}", target.name),
        )
        .values(
            Some(before.token().to_owned()),
            Some(after.token().to_owned()),
        )
        .target(target),
        _ => return None,
    })
}

fn capability_or_component_line(change: &ReleaseChange) -> Option<(bool, Line)> {
    Some(match change {
        ReleaseChange::CapabilityAdded { capability } => (
            true,
            Line::new(LineClass::Added, capability.clone()).detail("now provided"),
        ),
        ReleaseChange::CapabilityRemoved { capability } => (
            true,
            Line::new(LineClass::Removed, capability.clone()).detail("no longer provided"),
        ),
        ReleaseChange::ComponentAdded { component, name } => (
            false,
            Line::new(LineClass::Added, format!("{name} ({component})")),
        ),
        ReleaseChange::ComponentRemoved { component, name } => (
            false,
            Line::new(LineClass::Removed, format!("{name} ({component})")),
        ),
        ReleaseChange::ComponentSelection { component, details } => (
            false,
            Line::new(LineClass::Changed, format!("Component {component}"))
                .detail(details.join("; ")),
        ),
        ReleaseChange::GroupSelection {
            group,
            before,
            after,
        } => (
            false,
            Line::new(
                LineClass::Changed,
                format!("Selection rule of group {group}"),
            )
            .values(before.clone(), after.clone()),
        ),
        _ => return None,
    })
}

/// Lines for what one release contains or needs, grouped into titled sections.
pub fn release_lines(changes: &[ReleaseChange]) -> Vec<Section> {
    let mut runtime = Vec::new();
    let mut dependencies = Vec::new();
    let mut capabilities = Vec::new();
    let mut components = Vec::new();
    let mut artifacts = Vec::new();
    for change in changes {
        if let Some(lines) = runtime_lines(change) {
            runtime.extend(lines);
        } else if let Some(line) = dependency_line(change) {
            dependencies.push(line);
        } else if let Some((capability, line)) = capability_or_component_line(change) {
            if capability {
                capabilities.push(line);
            } else {
                components.push(line);
            }
        } else {
            artifacts.push(artifact_line(change));
        }
    }
    [
        ("Runtimes and platforms", runtime),
        ("Dependencies", dependencies),
        ("Capabilities", capabilities),
        ("Components", components),
        ("Artifacts", artifacts),
    ]
    .into_iter()
    .filter(|(_, lines)| !lines.is_empty())
    .map(|(title, lines)| Section {
        title: title.to_owned(),
        lines,
    })
    .collect()
}

fn artifact_line(change: &ReleaseChange) -> Line {
    match change {
        ReleaseChange::ArtifactAdded {
            artifact,
            filename,
            size,
            sha256,
        } => Line::new(LineClass::Added, format!("Artifact {artifact}"))
            .detail(format!("{filename}, {size} bytes, sha256 {sha256}")),
        ReleaseChange::ArtifactRemoved { artifact, filename } => {
            Line::new(LineClass::Removed, format!("Artifact {artifact}")).detail(filename.clone())
        }
        ReleaseChange::ArtifactDigest {
            artifact,
            before,
            after,
        } => Line::new(LineClass::Anomaly, format!("Artifact {artifact} sha256"))
            .values(Some(before.clone()), Some(after.clone()))
            .detail("Published artifact digest changed without a version change."),
        ReleaseChange::ArtifactSize {
            artifact,
            before,
            after,
        } => Line::new(LineClass::Changed, format!("Artifact {artifact} size")).values(
            Some(format!("{before} bytes")),
            Some(format!("{after} bytes")),
        ),
        ReleaseChange::ArtifactFile { artifact, details } => {
            Line::new(LineClass::Changed, format!("Artifact {artifact}")).detail(details.join("; "))
        }
        ReleaseChange::ArtifactSourceAdded {
            artifact,
            url,
            kind,
        } => {
            Line::new(LineClass::Added, format!("{kind} source for {artifact}")).detail(url.clone())
        }
        ReleaseChange::ArtifactSourceRemoved {
            artifact,
            url,
            kind,
        } => Line::new(LineClass::Removed, format!("{kind} source for {artifact}"))
            .detail(url.clone()),
        ReleaseChange::SignatureAdded {
            artifact,
            format,
            url,
        } => Line::new(
            LineClass::Added,
            format!("{format} signature for {artifact}"),
        )
        .detail(url.clone()),
        ReleaseChange::SignatureRemoved {
            artifact,
            format,
            url,
        } => Line::new(
            LineClass::Removed,
            format!("{format} signature for {artifact}"),
        )
        .detail(url.clone()),
        ReleaseChange::SourceRevision {
            field,
            before,
            after,
        } => Line::new(LineClass::Changed, format!("Source {field}"))
            .values(before.clone(), after.clone()),
        _ => Line::new(LineClass::Note, "Release contents changed"),
    }
}

fn status_label(status: ReleaseStatus) -> String {
    status.token().to_owned()
}

/// A one-line summary for lists: the most telling change first.
pub fn headline(event: &Event) -> String {
    match event.kind {
        EventKind::Withdrawn => return "no longer listed by its site".to_owned(),
        EventKind::Restored if event.changes.is_empty() => {
            return "listed again by its site".to_owned();
        }
        EventKind::Observed if event.moved_from.is_some() && event.changes.is_empty() => {
            return "same claim, new site".to_owned();
        }
        EventKind::Observed if event.moved_from.is_none() => return "first observed".to_owned(),
        _ => {}
    }
    let heads: Vec<String> = event
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::ChannelHead {
                channel,
                before,
                after,
                ..
            } => Some(format!(
                "{channel} {} → {}",
                before.as_deref().unwrap_or("none"),
                after.as_deref().unwrap_or("none")
            )),
            _ => None,
        })
        .collect();
    if !heads.is_empty() {
        return heads.join(", ");
    }
    for change in &event.changes {
        match change {
            Change::ReleaseStatus { version, after, .. } => {
                return format!("{version} {}", status_label(*after));
            }
            Change::ReleaseAmended { version, .. } => return format!("{version} amended"),
            Change::ReleaseRemoved { version, .. } => return format!("{version} vanished"),
            _ => {}
        }
    }
    match event.changes.len() {
        0 => "no structural changes".to_owned(),
        1 => "1 change".to_owned(),
        count => format!("{count} changes"),
    }
}

/// A release-level change as a line, and whether it is unusual.
fn release_line(change: &Change) -> Option<(bool, Line)> {
    Some(match change {
        Change::ReleaseAdded {
            version,
            channel,
            date,
            status,
            ..
        } => {
            let mut detail = format!("{channel}, {}", status.token());
            if let Some(date) = date {
                let _ = write!(detail, ", dated {date} by its publisher");
            }
            (
                false,
                Line::new(LineClass::Added, format!("Release {version}")).detail(detail),
            )
        }
        Change::ReleaseRemoved { version, channel } if channel == DEVELOPMENT_CHANNEL => (
            false,
            Line::new(LineClass::Removed, format!("Development build {version}"))
                .detail("replaced, as development builds are"),
        ),
        Change::ReleaseRemoved { version, channel } => (
            true,
            Line::new(LineClass::Anomaly, format!("Release {version} ({channel})"))
                .detail("Previously indexed release is no longer present in the current manifest."),
        ),
        Change::ReleaseStatus {
            version,
            before,
            after,
            reason,
            replacement,
            ..
        } => {
            let mut line = Line::new(LineClass::Changed, format!("Release {version}"))
                .values(Some(status_label(*before)), Some(status_label(*after)));
            let mut detail = reason.clone().unwrap_or_default();
            if let Some(replacement) = replacement {
                if !detail.is_empty() {
                    detail.push(' ');
                }
                let _ = write!(
                    detail,
                    "The publisher names {replacement} as the replacement."
                );
            }
            if !detail.is_empty() {
                line = line.detail(detail);
            }
            (false, line)
        }
        Change::ReleaseChannel {
            version,
            before,
            after,
        } => (
            false,
            Line::new(LineClass::Changed, format!("Release {version} channel"))
                .values(Some(before.clone()), Some(after.clone())),
        ),
        Change::ReleaseDate {
            version,
            before,
            after,
        } => (
            false,
            Line::new(LineClass::Changed, format!("Release {version} date"))
                .values(before.clone(), after.clone()),
        ),
        Change::ReleaseNotes { version } => (
            false,
            Line::new(LineClass::Changed, format!("Release {version} notes"))
                .detail("edited by the publisher"),
        ),
        _ => return None,
    })
}

/// Every change of an event as titled sections of lines. Unusual facts come first, then channel
/// moves and amendments, then releases, then project metadata.
pub fn sections(event: &Event) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut anomalies = Vec::new();
    let mut releases = Vec::new();
    let mut project = Vec::new();
    for change in &event.changes {
        match change {
            Change::ChannelHead {
                channel,
                before,
                after,
                changes,
            } => {
                sections.push(Section {
                    title: format!("Channel {channel}"),
                    lines: vec![
                        Line::new(LineClass::Changed, format!("{channel} head"))
                            .values(before.clone(), after.clone()),
                    ],
                });
                for mut section in release_lines(changes) {
                    section.title = format!("{channel}: {}", section.title);
                    sections.push(section);
                }
            }
            Change::ReleaseAmended {
                version,
                channel,
                changes,
            } => {
                if digest_anomaly(channel, changes) {
                    anomalies.push(
                        Line::new(LineClass::Anomaly, format!("Release {version}"))
                            .detail("Published artifact digest changed without a version change."),
                    );
                }
                for mut section in release_lines(changes) {
                    section.title = format!("{version} amended: {}", section.title);
                    sections.push(section);
                }
            }
            other => match release_line(other) {
                Some((true, line)) => anomalies.push(line),
                Some((false, line)) => releases.push(line),
                None => project.push(project_line(other)),
            },
        }
    }
    let mut ordered = Vec::new();
    if !anomalies.is_empty() {
        ordered.push(Section {
            title: "Unusual".to_owned(),
            lines: anomalies,
        });
    }
    ordered.extend(sections);
    if !releases.is_empty() {
        ordered.push(Section {
            title: "Releases".to_owned(),
            lines: releases,
        });
    }
    if !project.is_empty() {
        ordered.push(Section {
            title: "Project".to_owned(),
            lines: project,
        });
    }
    ordered
}

fn project_line(change: &Change) -> Line {
    let joined = |label: &str, added: &[String], removed: &[String]| {
        let mut parts = Vec::new();
        if !added.is_empty() {
            parts.push(format!("added {}", list(added)));
        }
        if !removed.is_empty() {
            parts.push(format!("removed {}", list(removed)));
        }
        Line::new(LineClass::Changed, label).detail(parts.join("; "))
    };
    match change {
        Change::ProjectField {
            field,
            before,
            after,
        } => {
            let class = if field == "versioning" {
                LineClass::Anomaly
            } else {
                LineClass::Changed
            };
            let line =
                Line::new(class, format!("Project {field}")).values(before.clone(), after.clone());
            if field == "versioning" {
                line.detail(
                    "Versions before and after this change may not order the way they used to.",
                )
            } else {
                line
            }
        }
        Change::Tags { added, removed } => joined("Tags", added, removed),
        Change::Maintainers { added, removed } => joined("Maintainers", added, removed),
        Change::Credits { added, removed } => joined("Credits", added, removed),
        Change::Link {
            link,
            before,
            after,
        } => Line::new(LineClass::Changed, format!("Link: {link}"))
            .values(before.clone(), after.clone()),
        Change::NexusMods { before, after } => {
            Line::new(LineClass::Changed, "Nexus Mods page").values(before.clone(), after.clone())
        }
        Change::Media { added, removed } => joined("Media", added, removed),
        _ => Line::new(LineClass::Note, "Changed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Notes, ProjectId};

    fn event(kind: EventKind, changes: Vec<Change>) -> Event {
        Event {
            id: "0".repeat(20),
            kind,
            project: ProjectId("4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90".to_owned()),
            origin: "site".to_owned(),
            name: "Candlelight".to_owned(),
            observed_at: "2026-09-28T00:00:00Z".to_owned(),
            before: Some("a".repeat(64)),
            after: Some("b".repeat(64)),
            changes,
            moved_from: None,
        }
    }

    #[test]
    fn a_stable_release_with_breaking_notes() {
        let event = event(
            EventKind::Changed,
            vec![
                Change::ChannelHead {
                    channel: "stable".to_owned(),
                    before: Some("1.1.0".to_owned()),
                    after: Some("2.0.0".to_owned()),
                    changes: vec![ReleaseChange::Runtime {
                        runtime: "openmw".to_owned(),
                        before: Some(">=0.49".to_owned()),
                        after: Some(">=0.50".to_owned()),
                    }],
                },
                Change::ReleaseAdded {
                    version: "2.0.0".to_owned(),
                    channel: "stable".to_owned(),
                    date: Some("2026-10-01".to_owned()),
                    status: ReleaseStatus::Available,
                    notes: Some(Notes {
                        breaking: Some(vec!["Settings moved.".to_owned()]),
                        migration: Some("Reset your settings.".to_owned()),
                        ..Notes::default()
                    }),
                },
            ],
        );
        let tags = tags(&event);
        for tag in [Tag::Release, Tag::Breaking, Tag::Migration, Tag::Runtime] {
            assert!(tags.contains(&tag), "{tags:?}");
        }
        assert!(!tags.contains(&Tag::Anomaly));
        assert_eq!(headline(&event), "stable 1.1.0 → 2.0.0");
        let sections = sections(&event);
        assert_eq!(sections[0].title, "Channel stable");
        assert_eq!(sections[1].title, "stable: Runtimes and platforms");
        assert_eq!(sections[1].lines[0].subject, "OpenMW runtime");
    }

    #[test]
    fn a_dependency_change_is_never_called_breaking() {
        let event = event(
            EventKind::Changed,
            vec![Change::ChannelHead {
                channel: "stable".to_owned(),
                before: Some("1.1.0".to_owned()),
                after: Some("1.2.0".to_owned()),
                changes: vec![ReleaseChange::RelationshipRemoved {
                    kind: crate::protocol::RelationshipKind::Requires,
                    target: TargetRef {
                        project: None,
                        capability: None,
                        name: "Tallow".to_owned(),
                        url: None,
                    },
                    version: Some(">=1.0".to_owned()),
                }],
            }],
        );
        let tags = tags(&event);
        assert!(tags.contains(&Tag::Dependencies));
        assert!(!tags.contains(&Tag::Breaking));
    }

    #[test]
    fn digests_changing_under_a_version_are_anomalies_except_in_development() {
        let amended = |channel: &str| {
            event(
                EventKind::Changed,
                vec![Change::ReleaseAmended {
                    version: "1.1.0".to_owned(),
                    channel: channel.to_owned(),
                    changes: vec![ReleaseChange::ArtifactDigest {
                        artifact: "fomod".to_owned(),
                        before: "a".repeat(64),
                        after: "b".repeat(64),
                    }],
                }],
            )
        };
        assert!(tags(&amended("stable")).contains(&Tag::Anomaly));
        assert_eq!(sections(&amended("stable"))[0].title, "Unusual");
        assert!(!tags(&amended("development")).contains(&Tag::Anomaly));
        assert!(is_development_only(&amended("development")));
    }

    #[test]
    fn vanished_releases_are_anomalies_and_replaced_development_builds_are_not() {
        let removed = |channel: &str| {
            event(
                EventKind::Changed,
                vec![Change::ReleaseRemoved {
                    version: "1.0.0".to_owned(),
                    channel: channel.to_owned(),
                }],
            )
        };
        assert!(tags(&removed("stable")).contains(&Tag::Anomaly));
        assert!(!tags(&removed("development")).contains(&Tag::Anomaly));
    }
}
