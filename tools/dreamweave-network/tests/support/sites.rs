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
pub const SITE_GENERATOR: &str = include_str!("../fixtures/jess/site-generator.json");
pub const WORDS: &str = include_str!("../fixtures/jess/words.json");
pub const STYLE: &str = include_str!("../fixtures/jess/style.json");
pub const THUMBNAIL: &[u8] = include_bytes!("../fixtures/thumbnail.png");
pub const CANDLELIGHT_ID: &str = "4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90";
pub const TALLOW_ID: &str = "9b7e3f21-6c4d-4a8b-b1e2-3f5a7c9d0e14";
pub const SITE_GENERATOR_ID: &str = "7f3c1a2e-5b6d-4e8f-9a0b-1c2d3e4f5a6b";
pub const WORDS_ID: &str = "0a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d";
pub const STYLE_ID: &str = "5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e";

pub fn candlelight() -> Value {
    serde_json::from_str(CANDLELIGHT).unwrap()
}

pub fn tallow() -> Value {
    serde_json::from_str(TALLOW).unwrap()
}

pub fn program() -> Value {
    serde_json::from_str(PROGRAM).unwrap()
}

/// A `schema_version` 3 project whose releases are the Jess package `org.dreamweave.site`.
pub fn site_generator() -> Value {
    serde_json::from_str(SITE_GENERATOR).unwrap()
}

/// A `schema_version` 3 project publishing a fictional `org.example.words`, newest version yanked.
pub fn words() -> Value {
    serde_json::from_str(WORDS).unwrap()
}

/// A `schema_version` 3 project publishing `org.example.style`, the package words depends on.
pub fn style() -> Value {
    serde_json::from_str(STYLE).unwrap()
}

/// The Jess package id a manifest's releases publish, if any: what its index entry repeats.
fn jess_package(manifest: &Value) -> Option<Value> {
    manifest["releases"]
        .as_array()?
        .iter()
        .flat_map(|release| release["artifacts"].as_array().into_iter().flatten())
        .find(|artifact| artifact["format"] == "jpk")
        .map(|artifact| artifact["jess"]["package"].clone())
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
        let mut entry = json!({
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
        });
        if let Some(package) = jess_package(manifest) {
            entry["jess_package"] = package;
        }
        entry
    }

    /// A site index: `schema_version` 3 when an entry uses what version 3 added, 2 otherwise, so
    /// the version 2 fixtures keep testing version 2 documents.
    pub fn index(&self, entries: &[Value]) -> Value {
        let version = if entries
            .iter()
            .any(|entry| entry.get("jess_package").is_some())
        {
            "3"
        } else {
            "2"
        };
        json!({
            "schema_version": version,
            "document": "index",
            "generator": "fixture",
            "site": { "name": "Fixture site", "url": self.url("") },
            "projects": entries,
        })
    }

    /// Points every media URL at this server, which serves a small PNG there, so no test ever
    /// fetches a publisher's real image, and every JPK source at this server too, so nothing in
    /// a test names a real download (the index never fetches one either way).
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
        for release in manifest["releases"].as_array_mut().into_iter().flatten() {
            for artifact in release["artifacts"].as_array_mut().into_iter().flatten() {
                if artifact["format"] != "jpk" {
                    continue;
                }
                let name = artifact["filename"].as_str().unwrap().to_owned();
                for source in artifact["sources"].as_array_mut().into_iter().flatten() {
                    let directory = if source["kind"] == "mirror" {
                        "mirror"
                    } else {
                        "packages"
                    };
                    source["url"] = self.url(&format!("{directory}/{name}")).into();
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
