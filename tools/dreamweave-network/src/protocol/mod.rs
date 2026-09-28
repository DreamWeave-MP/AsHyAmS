//! The DreamWeave protocol documents this index reads, `schema_version` "2", as typed Rust.
//!
//! These types are the boundary between untrusted bytes and everything else. They are only ever
//! built by [`parse`], after the vendored JSON Schema and the protocol's own rules have accepted
//! the document. Every field the protocol defines is here and nothing else is: an unknown core
//! field is an error in schema version 2, so `deny_unknown_fields` is the specification talking,
//! not caution.

pub mod parse;

use std::{cmp::Ordering, collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::version::{Scheme, Version};

pub const SCHEMA_VERSION: &str = "2";
pub const INDEX_MEDIA_TYPE: &str = "application/vnd.dreamweave.index+json";
pub const PROJECT_MEDIA_TYPE: &str = "application/vnd.dreamweave.project+json";
pub const INDEX_FILE_NAME: &str = "dreamweave.json";
/// The channel the protocol names as the rolling build of a project's default branch.
pub const DEVELOPMENT_CHANNEL: &str = "development";
/// The channel the protocol names as the default.
pub const STABLE_CHANNEL: &str = "stable";

/// A project id: an RFC 9562 UUID in canonical lowercase form, chosen by the project's author.
/// It says which project a document is about. It says nothing about who may speak for it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub String);

/// Canonical lowercase RFC 9562 form: 8-4-4-4-12 lowercase hex digits.
pub fn is_project_id(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, length)| {
            group.len() == length
                && group
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

impl fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

// Site index ----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteIndex {
    pub schema_version: String,
    pub document: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    pub site: Site,
    pub projects: Vec<IndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub name: String,
    pub url: String,
}

/// A pointer and a change detector. Everything it says is repeated, authoritatively, in the
/// manifest it points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub id: ProjectId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(rename = "type")]
    pub project_type: String,
    pub status: String,
    pub page: String,
    pub manifest: String,
    pub manifest_sha256: String,
    pub updated: Option<String>,
    pub channels: BTreeMap<String, String>,
}

// Project manifest ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: String,
    pub document: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    pub project: Project,
    pub channels: BTreeMap<String, ChannelHead>,
    pub releases: Vec<Release>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelHead {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(rename = "type")]
    pub project_type: ProjectType,
    pub status: ProjectStatus,
    pub versioning: Scheme,
    pub game: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub tags: Vec<String>,
    pub maintainers: Vec<Person>,
    pub links: BTreeMap<String, String>,
    pub integrations: Integrations,
    pub media: Vec<Media>,
    pub credits: Vec<Credit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectType {
    Mod,
    Library,
    Framework,
    Tool,
    Assets,
    TotalConversion,
    Documentation,
}

impl ProjectType {
    pub const ALL: [Self; 7] = [
        Self::Mod,
        Self::Library,
        Self::Framework,
        Self::Tool,
        Self::Assets,
        Self::TotalConversion,
        Self::Documentation,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Library => "library",
            Self::Framework => "framework",
            Self::Tool => "tool",
            Self::Assets => "assets",
            Self::TotalConversion => "total-conversion",
            Self::Documentation => "documentation",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "Mod",
            Self::Library => "Library",
            Self::Framework => "Framework",
            Self::Tool => "Tool",
            Self::Assets => "Asset pack",
            Self::TotalConversion => "Total conversion",
            Self::Documentation => "Documentation",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Self::Mod => "Mods",
            Self::Library => "Libraries",
            Self::Framework => "Frameworks",
            Self::Tool => "Tools",
            Self::Assets => "Asset packs",
            Self::TotalConversion => "Total conversions",
            Self::Documentation => "Documentation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectStatus {
    Active,
    Maintenance,
    Experimental,
    Deprecated,
    Archived,
}

impl ProjectStatus {
    pub const ALL: [Self; 5] = [
        Self::Active,
        Self::Maintenance,
        Self::Experimental,
        Self::Deprecated,
        Self::Archived,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Maintenance => "maintenance",
            Self::Experimental => "experimental",
            Self::Deprecated => "deprecated",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integrations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nexusmods: Option<NexusMods>,
}

/// A location on Nexus Mods. Never identity: two projects may point at one Nexus page, and one
/// project may have none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NexusMods {
    pub game: String,
    pub mod_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub kind: MediaKind,
    pub alt: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credit {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub channel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub status: ReleaseStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yanked: Option<Notice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Notice>,
    pub source: SourceRevision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
    pub runtimes: BTreeMap<String, String>,
    pub platforms: Vec<Platform>,
    pub provides: Vec<String>,
    pub relationships: Vec<Relationship>,
    pub components: Vec<Component>,
    pub groups: Vec<Group>,
    /// Preserved exactly: the protocol requires an index to pass extension data along unchanged.
    pub extensions: BTreeMap<String, Value>,
    pub critical_extensions: Vec<String>,
    pub artifacts: Vec<Artifact>,
}

impl Release {
    /// The OpenMW extension, when the release declares one. The parser has already checked it
    /// against the schema, so a failure here is a bug.
    pub fn openmw(&self) -> Option<OpenmwExtension> {
        self.extensions.get("openmw").map(|value| {
            serde_json::from_value(value.clone())
                .expect("the openmw extension was validated by the manifest schema")
        })
    }

    pub fn is_development(&self) -> bool {
        self.channel == DEVELOPMENT_CHANNEL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseStatus {
    Available,
    Yanked,
    Deprecated,
}

impl ReleaseStatus {
    pub fn token(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Yanked => "yanked",
            Self::Deprecated => "deprecated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevision {
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// Publisher-written release notes. The Markdown strings are CommonMark from a stranger: they are
/// rendered through the sanitizer in `markdown`, never pasted into a page.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaking: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_issues: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipKind {
    Requires,
    Recommends,
    Conflicts,
    Compatible,
    Replaces,
}

impl RelationshipKind {
    pub const ALL: [Self; 5] = [
        Self::Requires,
        Self::Recommends,
        Self::Conflicts,
        Self::Compatible,
        Self::Replaces,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Self::Requires => "requires",
            Self::Recommends => "recommends",
            Self::Conflicts => "conflicts",
            Self::Compatible => "compatible",
            Self::Replaces => "replaces",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub kind: RelationshipKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// What a relationship points at. A relationship with neither a project nor a capability is
/// informational: people can read it, nothing can resolve it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(tag = "by", rename_all = "lowercase")]
pub enum Target {
    Project { id: ProjectId },
    Capability { capability: String },
    Informational { name: String, url: Option<String> },
}

impl Relationship {
    pub fn target(&self) -> Target {
        if let Some(id) = &self.project {
            Target::Project { id: id.clone() }
        } else if let Some(capability) = &self.capability {
            Target::Capability {
                capability: capability.clone(),
            }
        } else {
            Target::Informational {
                name: self.name.clone().unwrap_or_default(),
                url: self.url.clone(),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub path: String,
    pub required: bool,
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub requires: Vec<String>,
    pub conflicts: Vec<String>,
    pub suggested_with: Vec<ProjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub select: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub format: String,
    pub filename: String,
    pub media_type: String,
    pub size: u64,
    pub digests: Digests,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    pub sources: Vec<ArtifactSource>,
    pub signatures: Vec<Signature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Digests {
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_document: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Publisher,
    Mirror,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSource {
    pub url: String,
    pub kind: SourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub format: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

// The OpenMW extension ------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenmwExtension {
    pub components: BTreeMap<String, OpenmwComponent>,
    pub requires_content: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lua_api: Option<String>,
    pub settings: Vec<OpenmwSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenmwComponent {
    pub data_directories: Vec<String>,
    pub content_files: Vec<String>,
    pub groundcover_files: Vec<String>,
    pub fallback_archives: Vec<String>,
    pub fallback_entries: BTreeMap<String, String>,
    pub config: bool,
    pub requires_content: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenmwSetting {
    pub category: String,
    pub key: String,
    pub value: String,
}

// Channel heads -------------------------------------------------------------------------------

impl Manifest {
    pub fn scheme(&self) -> Scheme {
        self.project.versioning
    }

    pub fn release(&self, version: &str) -> Option<&Release> {
        let wanted = Version::parse(version, self.scheme()).ok()?;
        self.releases.iter().find(|release| {
            Version::parse(&release.version, self.scheme())
                .is_ok_and(|candidate| candidate.precedence(&wanted) == Ordering::Equal)
        })
    }

    /// Each channel's highest-precedence available release, recomputed from `releases`. The
    /// protocol says a client doing this MUST get the manifest's own `channels`; the parser
    /// holds publishers to that.
    pub fn recomputed_heads(&self) -> BTreeMap<String, String> {
        let scheme = self.scheme();
        let mut heads: BTreeMap<String, (Version, String)> = BTreeMap::new();
        for release in &self.releases {
            if release.status != ReleaseStatus::Available {
                continue;
            }
            let Ok(version) = Version::parse(&release.version, scheme) else {
                continue;
            };
            let replace = heads
                .get(&release.channel)
                .is_none_or(|(head, _)| version.precedence(head) == Ordering::Greater);
            if replace {
                heads.insert(release.channel.clone(), (version, release.version.clone()));
            }
        }
        heads
            .into_iter()
            .map(|(channel, (_, text))| (channel, text))
            .collect()
    }
}
