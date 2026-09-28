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
pub const CANDLELIGHT_ID: &str = "4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90";
pub const TALLOW_ID: &str = "9b7e3f21-6c4d-4a8b-b1e2-3f5a7c9d0e14";

pub fn candlelight() -> Value {
    serde_json::from_str(CANDLELIGHT).unwrap()
}

pub fn tallow() -> Value {
    serde_json::from_str(TALLOW).unwrap()
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

    /// Serves every manifest and an index that lists them all, plus a front page linking it.
    pub fn publish(&self, manifests: &[Value]) {
        let mut entries = Vec::new();
        for manifest in manifests {
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
    let mut crawler = Crawler::new(Fetcher::new(AddressPolicy::AllowLoopback).unwrap());
    crawler.retry_delays = vec![std::time::Duration::ZERO; 2];
    crawler
}

pub async fn refresh(config: &Config, state: &mut State, now: &str) -> Report {
    crawler().refresh(config, state, now).await
}
