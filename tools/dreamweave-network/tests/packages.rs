//! Jess packages through real crawls: read from the manifests of the projects that publish them
//! (protocol `schema_version` 3), kept last-good through broken publications, taken out of the
//! catalog with the project that published them, and shown as two claims when two sites publish
//! one id.

mod support;

use dreamweave_network::{
    catalog::{self, IndexInfo},
    config::Curation,
    network::Network,
    packages::PackageId,
    protocol::ReleaseStatus,
    state::{ClaimHealth, ClaimKey, State},
};
use serde_json::Value;
use support::{
    Server,
    sites::{
        SITE_GENERATOR_ID, Site, WORDS_ID, candlelight, config, pretty, refresh, site_generator,
        tallow, words,
    },
};

const FIRST: &str = "2026-10-19T00:00:00Z";
const SECOND: &str = "2026-10-19T06:00:00Z";

fn only_origin(state: &State) -> String {
    assert_eq!(state.origins.len(), 1, "{:?}", state.origins.keys());
    state.origins.keys().next().unwrap().clone()
}

fn index() -> IndexInfo {
    IndexInfo {
        name: "AsHyAmS".to_owned(),
        url: "https://dreamweave-mp.github.io/AsHyAmS".to_owned(),
        repository: "https://github.com/DreamWeave-MP/AsHyAmS".to_owned(),
    }
}

fn export(network: &Network) -> Value {
    let document = serde_json::to_value(catalog::jess_packages(network, &index())).unwrap();
    catalog::check_jess_packages(&document).unwrap();
    document
}

#[tokio::test]
async fn a_site_without_package_projects_has_no_package_claims() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.packages.len(), 0);
    assert_eq!(export(&network)["packages"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_project_whose_releases_are_jpks_is_a_package_claim_with_every_version() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), site_generator(), words()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let origin = only_origin(&state);

    // The index entry repeats the package id; the manifest is where the versions come from.
    let key = ClaimKey {
        project: dreamweave_network::protocol::ProjectId(SITE_GENERATOR_ID.to_owned()),
        origin: origin.clone(),
    };
    assert_eq!(
        state.claims[&key].entry.jess_package.as_ref().unwrap().0,
        "org.dreamweave.site"
    );

    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.claims.len(), 3);
    assert_eq!(network.packages.len(), 2);
    let generator = network
        .packages
        .values()
        .find(|claim| claim.key.id.0 == "org.dreamweave.site")
        .unwrap();
    assert_eq!(generator.project, key);
    assert_eq!(generator.package.versions.len(), 3);
    assert_eq!(generator.package.versions[0].version, "0.1.2-dev.6");
    assert!(generator.package.versions[0].prerelease);
    assert_eq!(
        generator.current().unwrap().version,
        "0.1.1",
        "a development build is listed, and never current: only an exact requirement chooses it"
    );
    assert!(!generator.conflict);
    let words = network
        .packages
        .values()
        .find(|claim| claim.key.id.0 == "org.example.words")
        .unwrap();
    assert_eq!(words.package.versions[0].status, ReleaseStatus::Yanked);
    assert_eq!(
        words.current().unwrap().version,
        "1.2.0",
        "a yanked version is never current"
    );
    assert!(
        words.package.versions[0].artifact.sources[0]
            .url
            .contains("/mods/packages/org.example.words-1.3.0.jpk")
    );

    let document = export(&network);
    let packages = document["packages"].as_array().unwrap();
    assert_eq!(packages.len(), 2);
    let exported = packages
        .iter()
        .find(|package| package["id"] == "org.dreamweave.site")
        .unwrap();
    assert_eq!(exported["project"], SITE_GENERATOR_ID);
    assert_eq!(exported["health"], "current");
    assert_eq!(exported["cached"], false);
    assert_eq!(exported["current"], "0.1.1");
    assert!(
        exported["manifest_url"]
            .as_str()
            .unwrap()
            .ends_with(&format!(
                "/mods/dreamweave/projects/{SITE_GENERATOR_ID}.json"
            ))
    );
    assert_eq!(
        exported["manifest_sha256"],
        state.claims[&key].ingested_sha256.clone().unwrap()
    );
    assert_eq!(exported["versions"][0]["version"], "0.1.2-dev.6");
    assert_eq!(exported["versions"][0]["channel"], "development");
    let version = &exported["versions"][1];
    assert_eq!(version["version"], "0.1.1");
    assert_eq!(version["artifact"]["format"], "jpk");
    assert_eq!(
        version["content_digest"],
        version["artifact"]["jess"]["content_digest"]
    );
}

#[tokio::test]
async fn a_broken_manifest_keeps_the_last_good_package_and_says_why() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[words()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    let mut broken = words();
    broken["releases"][0]["artifacts"][0]["jess"]["content_digest"] = "not-a-digest".into();
    site.publish(&[broken]);
    refresh(&config, &mut state, SECOND).await;
    let key = ClaimKey {
        project: dreamweave_network::protocol::ProjectId(WORDS_ID.to_owned()),
        origin: only_origin(&state),
    };
    let record = &state.claims[&key];
    assert_eq!(record.health, ClaimHealth::Invalid);
    assert!(
        record
            .problem
            .as_deref()
            .unwrap()
            .contains("does not match its JSON Schema"),
        "{:?}",
        record.problem
    );

    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(
        network.listed_packages().len(),
        1,
        "the last good package stays listed"
    );
    let document = export(&network);
    assert_eq!(document["packages"][0]["cached"], true);
    assert_eq!(document["packages"][0]["health"], "invalid");
    assert_eq!(document["packages"][0]["versions"][0]["version"], "1.3.0");
}

#[tokio::test]
async fn a_project_the_site_stops_listing_takes_its_package_out_of_the_catalog() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[site_generator(), words()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    site.publish(&[site_generator()]);
    refresh(&config, &mut state, SECOND).await;
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.packages.len(), 2, "known, not listed");
    assert_eq!(network.listed_packages().len(), 1);
    assert_eq!(network.listed_packages()[0].key.id.0, "org.dreamweave.site");
    let document = export(&network);
    assert_eq!(document["packages"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn one_id_from_two_sites_is_two_claims_and_a_conflict_that_says_whether_bytes_agree() {
    let server = Server::start();
    let first = Site::new(&server, "/alpha/");
    let second = Site::new(&server, "/beta/");
    first.publish(&[site_generator(), words()]);
    // The second site mirrors the generator exactly and publishes words 1.2.0 with other bytes.
    let mut other = words();
    other["releases"][1]["artifacts"][0]["jess"]["content_digest"] = "e".repeat(64).into();
    second.publish(&[site_generator(), other]);
    let config = config(&[first.url(""), second.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.packages.len(), 4);
    assert!(network.packages.values().all(|claim| claim.conflict));
    assert_eq!(network.package_conflicts.len(), 2);
    let site = network
        .package_conflicts
        .iter()
        .find(|conflict| conflict.id == PackageId("org.dreamweave.site".to_owned()))
        .unwrap();
    assert_eq!(site.claims.len(), 2);
    assert_eq!(
        site.shared_versions, 3,
        "the development build counts among the versions shared"
    );
    assert!(site.same_bytes, "a mirror names the same digests");
    let words = network
        .package_conflicts
        .iter()
        .find(|conflict| conflict.id == PackageId("org.example.words".to_owned()))
        .unwrap();
    assert_eq!(words.shared_versions, 2);
    assert!(!words.same_bytes, "a different content digest for 1.2.0");

    let document = export(&network);
    assert_eq!(document["conflicts"].as_array().unwrap().len(), 2);
    assert!(
        document["packages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|package| package["conflict"] == true)
    );
    let again = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(
        pretty(&export(&network)),
        pretty(&export(&again)),
        "the same state builds the same bytes"
    );
}

#[tokio::test]
async fn a_project_naming_two_packages_is_an_invalid_publication_and_no_claim() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    let mut two = site_generator();
    two["releases"][1]["artifacts"][0]["jess"]["package"] = "org.dreamweave.other".into();
    site.publish(&[two]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let key = ClaimKey {
        project: dreamweave_network::protocol::ProjectId(SITE_GENERATOR_ID.to_owned()),
        origin: only_origin(&state),
    };
    assert_eq!(state.claims[&key].health, ClaimHealth::Invalid);
    assert!(
        state.claims[&key]
            .problem
            .as_deref()
            .unwrap()
            .contains("one project is one package")
    );
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.packages.len(), 0);
}
