//! The reviewed inputs: which sites this index reads (`network/sources.toml`) and what it has
//! decided on its own account (`network/curation.toml`). Both change by pull request, so both
//! are validated strictly: a typo here is our bug, and it fails the build.

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use url::Url;

use crate::{address::AddressPolicy, fetch, protocol::ProjectId};

pub const SOURCES_PATH: &str = "network/sources.toml";
pub const CURATION_PATH: &str = "network/curation.toml";

/// A site this index reads. Listing a site says "this index observes it", nothing more: not
/// that it is safe, good, maintained, or vouched for by anybody.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Any URL the discovery algorithm can start from: a project page, the site's front page,
    /// its `dreamweave.json`, or a manifest.
    pub url: String,
    /// When the entry was reviewed in.
    pub added: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcesFile {
    #[serde(default)]
    source: Vec<Source>,
}

/// This index's own decisions, shown on the site as the index's, never as the publisher's.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Curation {
    #[serde(default)]
    pub migration: Vec<Migration>,
    #[serde(default)]
    pub featured: Vec<Featured>,
}

/// An acknowledged host move: the claim for `project` published by the site whose index is
/// `from` continues as the claim published at `to`. Without one of these (or a redirect the
/// crawler saw itself), the same project id at two sites is an identity conflict.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    pub project: ProjectId,
    pub from: String,
    pub to: String,
    pub reason: String,
    pub reviewed: String,
}

/// A project this index chooses to put on its front page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Featured {
    pub project: ProjectId,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub sources: Vec<Source>,
    pub curation: Curation,
}

impl Config {
    /// `addresses` is the policy sources are reviewed under: always `PublicOnly`, except in a
    /// local failure drill against loopback fixtures.
    pub fn load(root: &Path, addresses: AddressPolicy) -> Result<Self> {
        let sources_path = root.join(SOURCES_PATH);
        let text = fs::read_to_string(&sources_path).with_context(|| {
            format!(
                "read {}; run this from the repository root",
                sources_path.display()
            )
        })?;
        let sources: SourcesFile =
            toml::from_str(&text).with_context(|| format!("parse {SOURCES_PATH}"))?;
        let curation_path = root.join(CURATION_PATH);
        let curation = if curation_path.exists() {
            toml::from_str(&fs::read_to_string(&curation_path)?)
                .with_context(|| format!("parse {CURATION_PATH}"))?
        } else {
            Curation::default()
        };
        let config = Self {
            sources: sources.source,
            curation,
        };
        config.validate(addresses)?;
        Ok(config)
    }

    pub fn validate(&self, addresses: AddressPolicy) -> Result<()> {
        let mut seen = BTreeSet::new();
        for source in &self.sources {
            let url = source_url(&source.url, addresses)
                .with_context(|| format!("{SOURCES_PATH}: source {:?}", source.url))?;
            if !seen.insert(url.to_string()) {
                bail!("{SOURCES_PATH}: {url} is listed twice");
            }
            check_date(&source.added)
                .with_context(|| format!("{SOURCES_PATH}: source {url}: added"))?;
        }
        for migration in &self.curation.migration {
            let at = format!("{CURATION_PATH}: migration of {}", migration.project);
            check_project_id(&migration.project).context(at.clone())?;
            let from = source_url(&migration.from, addresses).context(at.clone())?;
            let to = source_url(&migration.to, addresses).context(at.clone())?;
            if from == to {
                bail!("{at}: from and to are the same site index");
            }
            check_date(&migration.reviewed).context(at.clone())?;
            if migration.reason.trim().is_empty() {
                bail!("{at}: give the reason, so the next maintainer knows why");
            }
        }
        let mut featured = BTreeSet::new();
        for item in &self.curation.featured {
            check_project_id(&item.project)
                .with_context(|| format!("{CURATION_PATH}: featured {}", item.project))?;
            if !featured.insert(&item.project) {
                bail!("{CURATION_PATH}: {} is featured twice", item.project);
            }
        }
        Ok(())
    }
}

/// Parses a URL a maintainer wrote and applies the crawler's URL checks to it, so a source that
/// could never be fetched is rejected at review time rather than at 03:00 in CI.
pub fn source_url(text: &str, addresses: AddressPolicy) -> Result<Url> {
    let url = Url::parse(text.trim()).with_context(|| format!("{text:?} is not a URL"))?;
    fetch::check_url(&url, addresses).map_err(|error| anyhow::anyhow!("{error}"))?;
    // The resolver refuses these at crawl time anyway; saying so at review time is kinder.
    if let (AddressPolicy::PublicOnly, Some(url::Host::Domain(domain))) = (addresses, url.host()) {
        let local = ["localhost", "local", "internal", "lan", "home.arpa"]
            .iter()
            .any(|suffix| domain == *suffix || domain.ends_with(&format!(".{suffix}")));
        if local || !domain.contains('.') {
            bail!("{domain} is a local network name, not a public site");
        }
    }
    Ok(url)
}

pub fn check_project_id(id: &ProjectId) -> Result<()> {
    if crate::protocol::is_project_id(&id.0) {
        Ok(())
    } else {
        bail!("{id:?} is not a canonical lowercase UUID")
    }
}

fn check_date(text: &str) -> Result<()> {
    time::Date::parse(
        text,
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map(|_| ())
    .with_context(|| format!("{text:?} is not a YYYY-MM-DD date"))
}

/// The TOML block `add` appends to `sources.toml`. Appending text keeps the file's comments and
/// layout exactly as reviewers left them.
pub fn source_entry(url: &Url, added: &str, note: Option<&str>) -> String {
    let quote = |text: &str| toml::Value::String(text.to_owned()).to_string();
    let mut entry = format!(
        "\n[[source]]\nurl = {}\nadded = {}\n",
        quote(url.as_str()),
        quote(added)
    );
    if let Some(note) = note {
        entry.push_str("note = ");
        entry.push_str(&quote(note));
        entry.push('\n');
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(sources: &str, curation: &str) -> Result<Config> {
        let sources: SourcesFile = toml::from_str(sources)?;
        let config = Config {
            sources: sources.source,
            curation: toml::from_str(curation)?,
        };
        config.validate(AddressPolicy::PublicOnly)?;
        Ok(config)
    }

    #[test]
    fn a_reviewed_source_list() {
        let config = config(
            r#"
            [[source]]
            url = "https://dreamweave-mp.github.io/DreamWeave-Mod-Template/"
            added = "2026-09-28"
            note = "The template's own examples."
            "#,
            "",
        )
        .unwrap();
        assert_eq!(config.sources.len(), 1);
    }

    #[test]
    fn sources_that_could_never_be_fetched_fail_review() {
        for url in [
            "http://localhost/mods/",
            "http://192.168.0.10/",
            "file:///srv/mods/dreamweave.json",
            "https://user:pass@example.org/",
            "not a url",
        ] {
            let text = format!("[[source]]\nurl = {url:?}\nadded = \"2026-09-28\"\n");
            assert!(config(&text, "").is_err(), "{url} passed review");
        }
    }

    #[test]
    fn duplicates_typos_and_bad_dates_fail() {
        let twice = "[[source]]\nurl = \"https://example.org/mods/\"\nadded = \"2026-09-28\"\n\
                     [[source]]\nurl = \"https://example.org/mods/\"\nadded = \"2026-09-28\"\n";
        assert!(config(twice, "").is_err());
        let typo =
            "[[source]]\nurl = \"https://example.org/\"\nadded = \"2026-09-28\"\nnotes = \"x\"\n";
        assert!(config(typo, "").is_err());
        let date = "[[source]]\nurl = \"https://example.org/\"\nadded = \"yesterday\"\n";
        assert!(config(date, "").is_err());
    }

    #[test]
    fn curation_is_checked() {
        let migration = r#"
            [[migration]]
            project = "4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90"
            from = "https://old.example.org/dreamweave.json"
            to = "https://new.example.org/dreamweave.json"
            reason = "The author moved the site and said so on the old page."
            reviewed = "2026-09-28"
        "#;
        config("", migration).unwrap();
        assert!(config("", &migration.replace("4d0c9f6e", "4D0C9F6E")).is_err());
        assert!(config("", &migration.replace("new.example.org", "old.example.org")).is_err());
    }

    #[test]
    fn appended_entries_are_valid_toml() {
        let url = Url::parse("https://example.org/mods/").unwrap();
        let entry = source_entry(&url, "2026-09-28", Some("Says \"hello\"."));
        let config = config(&entry, "").unwrap();
        assert_eq!(config.sources[0].note.as_deref(), Some("Says \"hello\"."));
    }
}
