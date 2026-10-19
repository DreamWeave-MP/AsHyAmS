//! DreamWeave sites served by the fixture server: an index whose entries point at this server
//! and advertise the exact digests of the manifest bytes it serves.

#![allow(dead_code)]

use dreamweave_network::{
    address::AddressPolicy,
    config::{Config, Curation, Source},
    crawl::{Crawler, Report},
    fetch::Fetcher,
    state::{State, sha256_hex},
};
use serde_json::{Value, json};

use super::Server;

pub const CANDLELIGHT: &str = include_str!("../fixtures/mod-template/candlelight.json");
pub const TALLOW: &str = include_str!("../fixtures/mod-template/tallow.json");
pub const PROGRAM: &str = include_str!("../fixtures/programs/lantern-forge.json");
pub const FEED: &str = include_str!("../fixtures/jess/jess-packages.json");
pub const THUMBNAIL: &[u8] = include_bytes!("../fixtures/thumbnail.png");
pub const CANDLELIGHT_ID: &str = "4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90";
pub const TALLOW_ID: &str = "9b7e3f21-6c4d-4a8b-b1e2-3f5a7c9d0e14";

pub fn candlelight() -> Value {
    serde_json::from_str(CANDLELIGHT).unwrap()
}

pub fn tallow() -> Value {
    serde_json::from_str(TALLOW).unwrap()
}

pub fn program() -> Value {
    serde_json::from_str(PROGRAM).unwrap()
}

/// The fixture Jess package feed: `org.dreamweave.site` and a fictional `org.example.words`.
pub fn feed() -> Value {
    serde_json::from_str(FEED).unwrap()
}

pub fn pretty(value: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(value).unwrap()
}

/// A site at `base` (for example `/mods/`) on `server`.
pub struct Site<'a> {
    pub server: &'a Server,
    pub base: String,
}

impl<'a> Site<'a> {
    pub fn new(server: &'a Server, base: &str) -> Self {
        Self {
            server,
            base: base.to_owned(),
        }
    }

    pub fn url(&self, path: &str) -> String {
        self.server.url(&format!("{}{path}", self.base)).to_string()
    }

    pub fn manifest_path(&self, id: &str) -> String {
        format!("{}dreamweave/projects/{id}.json", self.base)
    }

    pub fn entry(&self, manifest: &Value, bytes: &[u8]) -> Value {
        let id = manifest["project"]["id"].as_str().unwrap();
        let channels: serde_json::Map<String, Value> = manifest["channels"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(channel, head)| (channel.clone(), head["version"].clone()))
            .collect();
        json!({
            "id": id,
            "name": manifest["project"]["name"],
            "summary": manifest["project"]["summary"],
            "type": manifest["project"]["type"],
            "status": manifest["project"]["status"],
            "page": self.url(&format!("{id}/")),
            "manifest": self.url(&format!("dreamweave/projects/{id}.json")),
            "manifest_sha256": sha256_hex(bytes),
            "updated": manifest["releases"][0]["date"],
            "channels": channels,
        })
    }

    pub fn index(&self, entries: &[Value]) -> Value {
        json!({
            "schema_version": "2",
            "document": "index",
            "generator": "fixture",
            "site": { "name": "Fixture site", "url": self.url("") },
            "projects": entries,
        })
    }

    /// Points every media URL at this server, which serves a small PNG there, so no test ever
    /// fetches a publisher's real image.
    pub fn localized(&self, manifest: &Value) -> Value {
        let mut manifest = manifest.clone();
        let id = manifest["project"]["id"].as_str().unwrap().to_owned();
        if let Some(items) = manifest["project"]["media"].as_array_mut() {
            for (position, item) in items.iter_mut().enumerate() {
                for field in ["url", "thumbnail"] {
                    if item.get(field).is_some() {
                        let path = format!("{}media/{id}-{position}-{field}.png", self.base);
                        self.server.serve(&path, "image/png", THUMBNAIL.to_vec());
                        item[field] = self.server.url(&path).to_string().into();
                    }
                }
            }
        }
        manifest
    }

    /// Serves every manifest and an index that lists them all, plus a front page linking it.
    pub fn publish(&self, manifests: &[Value]) {
        let mut entries = Vec::new();
        for manifest in manifests {
            let manifest = &self.localized(manifest);
            let bytes = pretty(manifest);
            let id = manifest["project"]["id"].as_str().unwrap();
            self.server
                .serve_json(&self.manifest_path(id), bytes.clone());
            entries.push(self.entry(manifest, &bytes));
        }
        self.serve_index(&self.index(&entries));
        self.server.serve(
            &self.base,
            "text/html",
            format!(
                r#"<!DOCTYPE html><html><head><link rel="alternate" type="application/vnd.dreamweave.index+json" href="{}"></head><body>Fixture</body></html>"#,
                self.url("dreamweave.json")
            ),
        );
    }

    pub fn serve_index(&self, index: &Value) {
        self.server
            .serve_json(&format!("{}dreamweave.json", self.base), pretty(index));
    }

    pub fn feed_path(&self) -> String {
        format!("{}jess-packages.json", self.base)
    }

    /// Serves a Jess package feed beside the site index, pointing its archive URLs at this
    /// server so nothing in a test names a real download.
    pub fn publish_feed(&self, feed: &Value) {
        let mut feed = feed.clone();
        if let Some(packages) = feed["packages"].as_array_mut() {
            for package in packages {
                for version in package["versions"].as_array_mut().into_iter().flatten() {
                    let name = version["artifact"]["filename"].as_str().unwrap().to_owned();
                    for source in version["artifact"]["sources"]
                        .as_array_mut()
                        .into_iter()
                        .flatten()
                    {
                        let directory = if source["kind"] == "mirror" {
                            "mirror"
                        } else {
                            "packages"
                        };
                        source["url"] = self.url(&format!("{directory}/{name}")).into();
                    }
                }
            }
        }
        feed["publisher"]["url"] = self.url("").into();
        self.server.serve_json(&self.feed_path(), pretty(&feed));
    }

    pub fn withdraw_feed(&self) {
        self.server.remove(&self.feed_path());
    }
}

pub fn config(sources: &[String]) -> Config {
    Config {
        sources: sources
            .iter()
            .map(|url| Source {
                url: url.clone(),
                added: "2026-09-28".to_owned(),
                note: None,
            })
            .collect(),
        curation: Curation::default(),
    }
}

pub fn crawler() -> Crawler {
    let mut crawler = Crawler::new(Fetcher::new(AddressPolicy::LoopbackOnly).unwrap());
    crawler.retry_delays = vec![std::time::Duration::ZERO; 2];
    crawler
}

pub async fn refresh(config: &Config, state: &mut State, now: &str) -> Report {
    crawler().refresh(config, state, now).await
}
