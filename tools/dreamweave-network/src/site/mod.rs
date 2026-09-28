//! The static site's inputs, written from the network model: Zola content pages, the per-page
//! view data their templates render, and the public files under `network-data/`.
//!
//! ```text
//! pages/                  hand-written pages, copied into content/ first
//! content/                everything Zola renders; generated, never edited
//! data/network/           view data, one JSON file per generated page
//! static/network-data/    catalog.json, events.json, search.json, updates.xml
//! static/network-media/   cached card images, by digest
//! ```
//!
//! The output is a pure function of the state, the reviewed configuration and `pages/`. No
//! clock, no network, no randomness: the same inputs build the same bytes, and the only times
//! on the site are observation times the crawler recorded.

pub mod format;
mod pages;
pub mod views;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{catalog::IndexInfo, network::Network, state};

pub const PAGES_DIRECTORY: &str = "pages";
pub const CONTENT_DIRECTORY: &str = "content";
pub const VIEW_DIRECTORY: &str = "data/network";
pub const DATA_DIRECTORY: &str = "static/network-data";
pub const MEDIA_DIRECTORY: &str = "static/network-media";

#[derive(Debug, serde::Deserialize)]
struct ZolaConfig {
    base_url: String,
    title: String,
    #[serde(default)]
    extra: ZolaExtra,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ZolaExtra {
    #[serde(default)]
    repository: Option<String>,
}

/// Reads what the site says about itself from `zola.toml`.
pub fn index_info(root: &Path) -> Result<IndexInfo> {
    let path = root.join("zola.toml");
    let config: ZolaConfig = toml::from_str(
        &fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
    )
    .with_context(|| format!("parse {}", path.display()))?;
    Ok(IndexInfo {
        name: config.title,
        url: config.base_url.trim_end_matches('/').to_owned(),
        repository: config
            .extra
            .repository
            .unwrap_or_else(|| "https://github.com/DreamWeave-MP/AsHyAmS".to_owned()),
    })
}

#[derive(Debug, Default)]
pub struct Built {
    pub pages: usize,
    pub claims: usize,
    pub events: usize,
}

pub(crate) struct Writer {
    root: PathBuf,
    pages: usize,
}

/// Front matter of a generated page. `view` names the JSON its template loads.
pub(crate) struct Front<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub template: &'a str,
    pub view: Option<&'a str>,
}

impl Writer {
    fn file(&self, relative: &str, bytes: &[u8]) -> Result<()> {
        let path = self.root.join(relative);
        fs::create_dir_all(
            path.parent()
                .context("generated files live in a directory")?,
        )?;
        fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))
    }

    pub(crate) fn json(&self, relative: &str, value: &impl Serialize) -> Result<()> {
        self.file(relative, &state::to_json(value))
    }

    /// A view file under `data/network/`, returned as the path a template passes to `load_data`.
    pub(crate) fn view(&self, name: &str, value: &impl Serialize) -> Result<String> {
        let path = format!("{VIEW_DIRECTORY}/{name}.json");
        self.json(&path, value)?;
        Ok(path)
    }

    pub(crate) fn page(&mut self, content_path: &str, front: &Front<'_>) -> Result<()> {
        let quote = |text: &str| toml::Value::String(text.to_owned()).to_string();
        let mut text = format!(
            "+++\ntitle = {}\ndescription = {}\ntemplate = {}\n",
            quote(front.title),
            quote(front.description),
            quote(front.template)
        );
        if let Some(view) = front.view {
            text.push_str("\n[extra]\nview = ");
            text.push_str(&quote(view));
            text.push('\n');
        }
        text.push_str("+++\n");
        self.pages += 1;
        self.file(
            &format!("{CONTENT_DIRECTORY}/{content_path}"),
            text.as_bytes(),
        )
    }
}

fn reset(directory: &Path) -> Result<()> {
    if directory.exists() {
        fs::remove_dir_all(directory).with_context(|| format!("clear {}", directory.display()))?;
    }
    fs::create_dir_all(directory).with_context(|| format!("create {}", directory.display()))
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    let mut entries = fs::read_dir(source)
        .with_context(|| format!("read {}", source.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    for entry in entries {
        let destination = target.join(entry.file_name().context("a directory entry has a name")?);
        if entry.is_dir() {
            copy_tree(&entry, &destination)?;
        } else {
            fs::copy(&entry, &destination).with_context(|| format!("copy {}", entry.display()))?;
        }
    }
    Ok(())
}

/// Writes everything Zola needs. `state_directory` supplies the cached media.
pub fn build(root: &Path, state_directory: &Path, network: &Network) -> Result<Built> {
    let index = index_info(root)?;
    for directory in [
        CONTENT_DIRECTORY,
        VIEW_DIRECTORY,
        DATA_DIRECTORY,
        MEDIA_DIRECTORY,
    ] {
        reset(&root.join(directory))?;
    }
    let pages = root.join(PAGES_DIRECTORY);
    if pages.exists() {
        copy_tree(&pages, &root.join(CONTENT_DIRECTORY))?;
    }
    let mut writer = Writer {
        root: root.to_path_buf(),
        pages: 0,
    };
    pages::write_all(&mut writer, network, &index, &pages)?;

    for claim in network.claims.values() {
        let Some(file) = claim
            .record
            .media
            .as_ref()
            .and_then(|media| media.file.as_ref())
        else {
            continue;
        };
        let source = state_directory.join(file);
        let name = file.trim_start_matches("media/");
        fs::copy(&source, root.join(MEDIA_DIRECTORY).join(name)).with_context(|| {
            format!(
                "copy {}: the state names a cached image it does not hold",
                source.display()
            )
        })?;
    }
    Ok(Built {
        pages: writer.pages,
        claims: network.listed().len(),
        events: network.events.len(),
    })
}
