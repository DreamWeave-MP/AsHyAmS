//! Jess package feeds through real crawls: observed beside the index, kept last-good through
//! outages and broken publications, withdrawn on a 404, and shown as two claims when two sites
//! list one id.

mod support;

use dreamweave_network::{
    catalog::{self, IndexInfo},
    config::Curation,
    network::Network,
    packages::PackageId,
    state::{FeedHealth, State},
};
use serde_json::json;
use support::{
    Server,
    sites::{Site, candlelight, config, feed, pretty, refresh, tallow},
};

const FIRST: &str = "2026-10-19T00:00:00Z";
const SECOND: &str = "2026-10-19T06:00:00Z";
const THIRD: &str = "2026-10-19T12:00:00Z";

fn only_origin(state: &State) -> String {
    assert_eq!(state.origins.len(), 1, "{:?}", state.origins.keys());
    state.origins.keys().next().unwrap().clone()
}

fn feed_requests(server: &Server) -> usize {
    server
        .requested_paths()
        .iter()
        .filter(|path| path.ends_with("jess-packages.json"))
        .count()
}

fn index() -> IndexInfo {
    IndexInfo {
        name: "AsHyAmS".to_owned(),
        url: "https://dreamweave-mp.github.io/AsHyAmS".to_owned(),
        repository: "https://github.com/DreamWeave-MP/AsHyAmS".to_owned(),
    }
}

#[tokio::test]
async fn a_site_without_a_feed_costs_one_request_and_no_record() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    let report = refresh(&config, &mut state, FIRST).await;
    assert_eq!(report.feeds_fetched, 0);
    assert_eq!(feed_requests(&server), 1);
    let origin = &state.origins[&only_origin(&state)];
    assert!(origin.jess_packages.is_none());
    assert_eq!(state.feeds.len(), 0);
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.packages.len(), 0);
}

#[tokio::test]
async fn a_feed_is_observed_kept_as_served_and_not_refetched_while_unchanged() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight()]);
    site.publish_feed(&feed());
    let config = config(&[site.url("")]);
    let mut state = State::default();

    let report = refresh(&config, &mut state, FIRST).await;
    assert_eq!(report.feeds_fetched, 1);
    assert!(report.summary().contains("1 Jess package feeds fetched"));
    let origin_id = only_origin(&state);
    let record = state.origins[&origin_id].jess_packages.as_ref().unwrap();
    assert_eq!(record.health, FeedHealth::Current);
    assert_eq!(record.first_observed, FIRST);
    assert_eq!(record.last_changed.as_deref(), Some(FIRST));
    assert!(record.url.ends_with("/mods/jess-packages.json"));
    let held = state.feed_bytes(&origin_id).unwrap();
    assert_eq!(
        record.ingested_sha256.as_deref(),
        Some(dreamweave_network::state::sha256_hex(held).as_str())
    );
    assert_eq!(
        state.verify(std::path::Path::new("/nonexistent")),
        Vec::<String>::new()
    );

    server.forget_requests();
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(report.feeds_fetched, 0);
    assert_eq!(
        feed_requests(&server),
        1,
        "one conditional request per crawl"
    );
    let record = state.origins[&origin_id].jess_packages.as_ref().unwrap();
    assert_eq!(record.health, FeedHealth::Current);
    assert_eq!(record.last_changed.as_deref(), Some(FIRST));
    assert_eq!(record.last_success.as_deref(), Some(SECOND));

    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.packages.len(), 2);
    let claim = network
        .packages
        .values()
        .find(|claim| claim.key.id.0 == "org.dreamweave.site")
        .unwrap();
    assert_eq!(claim.current().unwrap().version, "0.1.0");
    assert!(!claim.conflict);
    let words = network
        .packages
        .values()
        .find(|claim| claim.key.id.0 == "org.example.words")
        .unwrap();
    assert_eq!(
        words.current().unwrap().version,
        "1.2.0",
        "a yanked version is never current"
    );
}

#[tokio::test]
async fn a_broken_feed_keeps_the_last_good_one_and_says_why() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight()]);
    site.publish_feed(&feed());
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let origin_id = only_origin(&state);
    let good = state.feed_bytes(&origin_id).unwrap().to_vec();

    let mut broken = feed();
    broken["packages"][0]["versions"][0]["content_digest"] = "not-a-digest".into();
    site.publish_feed(&broken);
    refresh(&config, &mut state, SECOND).await;
    let record = state.origins[&origin_id].jess_packages.as_ref().unwrap();
    assert_eq!(record.health, FeedHealth::Invalid);
    assert!(
        record
            .problem
            .as_deref()
            .unwrap()
            .contains("does not match its JSON Schema"),
        "{:?}",
        record.problem
    );
    assert_eq!(state.feed_bytes(&origin_id).unwrap(), good.as_slice());
    assert!(record.rejected.is_some());

    // The same broken bytes again are not parsed again.
    server.forget_requests();
    refresh(&config, &mut state, THIRD).await;
    let record = state.origins[&origin_id].jess_packages.as_ref().unwrap();
    assert_eq!(record.health, FeedHealth::Invalid);
    assert_eq!(state.feed_bytes(&origin_id).unwrap(), good.as_slice());

    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(
        network.listed_packages().len(),
        2,
        "last good packages stay listed"
    );
    let document = serde_json::to_value(catalog::jess_packages(&network, &index())).unwrap();
    catalog::check_jess_packages(&document).unwrap();
    assert_eq!(document["packages"][0]["cached"], true);
    assert_eq!(document["packages"][0]["health"], "invalid");
}

#[tokio::test]
async fn an_unreachable_site_marks_the_feed_and_a_404_withdraws_it() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight()]);
    site.publish_feed(&feed());
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let origin_id = only_origin(&state);

    let routes = server.take_down();
    refresh(&config, &mut state, SECOND).await;
    let record = state.origins[&origin_id].jess_packages.as_ref().unwrap();
    assert_eq!(record.health, FeedHealth::OriginUnavailable);
    assert!(state.feed_bytes(&origin_id).is_some());
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.listed_packages().len(), 2, "cached, still listed");

    server.restore(routes);
    site.withdraw_feed();
    refresh(&config, &mut state, THIRD).await;
    let record = state.origins[&origin_id].jess_packages.as_ref().unwrap();
    assert_eq!(record.health, FeedHealth::Withdrawn);
    assert!(
        state.feed_bytes(&origin_id).is_some(),
        "the record and its bytes stay; Git keeps the history"
    );
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.listed_packages().len(), 0);
    assert_eq!(network.packages.len(), 2, "known, not listed");
    let document = serde_json::to_value(catalog::jess_packages(&network, &index())).unwrap();
    catalog::check_jess_packages(&document).unwrap();
    assert_eq!(document["packages"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_page_where_the_feed_would_be_is_no_feed() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight()]);
    server.serve(
        &site.feed_path(),
        "text/html",
        "<!DOCTYPE html><html><body>Not here</body></html>",
    );
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    assert!(state.origins[&only_origin(&state)].jess_packages.is_none());
}

#[tokio::test]
async fn one_id_from_two_sites_is_two_claims_and_a_conflict_that_says_whether_bytes_agree() {
    let server = Server::start();
    let first = Site::new(&server, "/alpha/");
    let second = Site::new(&server, "/beta/");
    first.publish(&[candlelight()]);
    second.publish(&[tallow()]);
    first.publish_feed(&feed());
    // The second site mirrors the generator exactly and publishes words 1.2.0 with other bytes.
    let mut other = feed();
    other["publisher"]["name"] = "Somebody else".into();
    other["packages"][1]["versions"][1]["content_digest"] = "e".repeat(64).into();
    second.publish_feed(&other);
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
    assert_eq!(site.shared_versions, 1);
    assert!(site.same_bytes, "a mirror names the same digests");
    let words = network
        .package_conflicts
        .iter()
        .find(|conflict| conflict.id == PackageId("org.example.words".to_owned()))
        .unwrap();
    assert_eq!(words.shared_versions, 2);
    assert!(!words.same_bytes, "a different content digest for 1.2.0");

    let document = serde_json::to_value(catalog::jess_packages(&network, &index())).unwrap();
    catalog::check_jess_packages(&document).unwrap();
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
        pretty(&serde_json::to_value(catalog::jess_packages(&network, &index())).unwrap()),
        pretty(&serde_json::to_value(catalog::jess_packages(&again, &index())).unwrap()),
        "the same state builds the same bytes"
    );
}

#[tokio::test]
async fn a_package_may_point_at_a_project_on_its_own_site_only() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight()]);
    let mut linked = feed();
    linked["packages"][0]["project"] = json!(support::sites::CANDLELIGHT_ID);
    site.publish_feed(&linked);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let network = Network::build(&state, &Curation::default()).unwrap();
    let document = serde_json::to_value(catalog::jess_packages(&network, &index())).unwrap();
    catalog::check_jess_packages(&document).unwrap();
    assert_eq!(
        document["packages"][0]["project"],
        support::sites::CANDLELIGHT_ID
    );
}
