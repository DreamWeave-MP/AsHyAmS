//! Jess packages as DreamWeave projects publish them, protocol `schema_version` "3": a project
//! whose releases carry a `jpk` artifact is a Jess package, and the artifact's `jess` block says
//! which one, with the package's API, its content digest and its declared dependencies.
//!
//! The same three rules apply as to any claim. The publisher's manifest is the authority: this
//! index copies what it says and says where it came from. A package id is a claim, never proof
//! of who may publish it: the same id from two sites is shown as two claims, `(package id,
//! origin)`, kept apart from the project claims they come from. And nothing here is fetched or
//! run: the archive URLs are the publisher's, and only a Jess client downloads and verifies them.
//!
//! A version carries two digests and they are different facts. `content_digest` is Jess's own,
//! over the package's canonical content, the digest `jess.lock` pins, which is the same however
//! the archive is packed. `artifact.digests.sha256` is the digest of the archive file as served.
//! A client checks both.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::protocol::{Artifact, JPK_MEDIA_TYPE, Manifest, Notice, ProjectId, ReleaseStatus};
pub use crate::protocol::{PackageId, is_package_id};
use crate::version::Version;

/// The media type of a JPK archive, as a manifest and a publisher's server name it.
pub const ARTIFACT_MEDIA_TYPE: &str = JPK_MEDIA_TYPE;

/// The Jess package one project publishes: every release of the project that carries a `jpk`
/// artifact, in the manifest's order, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Package {
    pub id: PackageId,
    /// The project whose releases these are, on the same site.
    pub project: ProjectId,
    pub name: String,
    pub summary: Option<String>,
    /// Newest first. Yanked and deprecated versions stay listed and say so.
    pub versions: Vec<PackageVersion>,
}

impl Package {
    /// The newest version the publisher still offers for new installs, if any. Never a
    /// pre-release (`0.1.2-dev.6`, a development build): Jess chooses one only when a
    /// requirement names it exactly, so it is listed, and it is not what a new install gets.
    pub fn current(&self) -> Option<&PackageVersion> {
        self.versions
            .iter()
            .find(|version| version.status == ReleaseStatus::Available && !version.prerelease)
    }

    pub fn version(&self, text: &str) -> Option<&PackageVersion> {
        self.versions.iter().find(|version| version.version == text)
    }
}

/// One release of a Jess package, as its manifest describes it: the release's own facts, the
/// package facts from the artifact's `jess` block, and the archive.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageVersion {
    pub version: String,
    /// Whether `version` carries pre-release identifiers under the project's scheme: a fact read
    /// off the version text, so the document doesn't repeat it.
    #[serde(skip)]
    pub prerelease: bool,
    pub channel: String,
    pub status: ReleaseStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yanked: Option<Notice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Notice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub jess_api: String,
    /// Jess's content digest: what `jess.lock` pins. Not the archive's digest.
    pub content_digest: String,
    /// The package's declared `[dependencies]`, by id, as Jess requirements. Copied, never
    /// resolved here.
    pub dependencies: BTreeMap<String, String>,
    /// The archive, with its own digest, its sources and its signatures.
    pub artifact: Artifact,
}

/// The Jess package `manifest` publishes, or `None` when no release of it carries a `jpk`
/// artifact. The parser has already held every jpk artifact of one manifest to one package id.
pub fn package_of(manifest: &Manifest) -> Option<Package> {
    let mut id = None;
    let mut versions = Vec::new();
    for release in &manifest.releases {
        let Some(artifact) = release
            .artifacts
            .iter()
            .find(|artifact| artifact.is_package())
        else {
            continue;
        };
        let Some(jess) = &artifact.jess else {
            continue;
        };
        id.get_or_insert_with(|| jess.package.clone());
        // The parser has already held every release's version to the project's scheme.
        let prerelease = Version::parse(&release.version, manifest.scheme())
            .is_ok_and(|version| version.is_prerelease());
        versions.push(PackageVersion {
            version: release.version.clone(),
            prerelease,
            channel: release.channel.clone(),
            status: release.status,
            yanked: release.yanked.clone(),
            deprecated: release.deprecated.clone(),
            date: release.date.clone(),
            jess_api: jess.jess_api.clone(),
            content_digest: jess.content_digest.clone(),
            dependencies: jess.dependencies.clone(),
            artifact: artifact.clone(),
        });
    }
    Some(Package {
        id: id?,
        project: manifest.project.id.clone(),
        name: manifest.project.name.clone(),
        summary: manifest.project.summary.clone(),
        versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse::parse_manifest;

    const SITE_GENERATOR: &str = include_str!("../tests/fixtures/jess/site-generator.json");
    const WORDS: &str = include_str!("../tests/fixtures/jess/words.json");
    const TALLOW: &str = include_str!("../tests/fixtures/mod-template/tallow.json");

    #[test]
    fn a_project_with_jpk_releases_is_a_package_and_one_without_is_not() {
        let generator = package_of(&parse_manifest(SITE_GENERATOR.as_bytes()).unwrap()).unwrap();
        assert_eq!(generator.id.0, "org.dreamweave.site");
        assert_eq!(generator.versions.len(), 3);
        assert_eq!(generator.versions[0].version, "0.1.2-dev.6");
        assert!(generator.versions[0].prerelease);
        assert_eq!(
            generator.current().unwrap().version,
            "0.1.1",
            "a development build is listed, and never current: only an exact requirement chooses it"
        );
        assert_eq!(generator.version("0.1.0").unwrap().channel, "stable");
        let words = package_of(&parse_manifest(WORDS.as_bytes()).unwrap()).unwrap();
        assert_eq!(words.versions[0].status, ReleaseStatus::Yanked);
        assert_eq!(
            words.current().unwrap().version,
            "1.2.0",
            "a yanked version is never current"
        );
        assert_eq!(words.versions[0].dependencies["org.example.style"], "^1.0");
        assert_eq!(words.versions[0].artifact.media_type, ARTIFACT_MEDIA_TYPE);
        assert!(package_of(&parse_manifest(TALLOW.as_bytes()).unwrap()).is_none());
    }

    #[test]
    fn package_ids_are_dotted_lowercase_words() {
        assert!(is_package_id("org.dreamweave.site"));
        assert!(is_package_id("io.github.someone.my-tool2"));
        assert!(!is_package_id("site"));
        assert!(!is_package_id("Org.Dreamweave.Site"));
        assert!(!is_package_id(".org.dreamweave"));
        assert!(!is_package_id("org dreamweave"));
    }
}
