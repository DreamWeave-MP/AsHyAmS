//! Refreshes against real HTTP, covering the failure model: outages, broken publications,
//! half-finished deployments, withdrawals, moves and identity collisions.

mod support;

use dreamweave_network::{
    protocol::ProjectId,
    state::{ClaimHealth, ClaimKey, EventKind, OriginHealth, SourceHealth, State, sha256_hex},
};
use serde_json::json;
use support::{
    Server,
    sites::{CANDLELIGHT_ID, Site, TALLOW_ID, candlelight, config, pretty, refresh, tallow},
};

const FIRST: &str = "2026-09-28T00:00:00Z";
const SECOND: &str = "2026-09-28T06:00:00Z";
const THIRD: &str = "2026-09-28T12:00:00Z";

fn only_origin(state: &State) -> String {
    assert_eq!(state.origins.len(), 1, "{:?}", state.origins.keys());
    state.origins.keys().next().unwrap().clone()
}

fn key(state: &State, project: &str) -> ClaimKey {
    ClaimKey {
        project: ProjectId(project.to_owned()),
        origin: only_origin(state),
    }
}

fn manifest_requests(server: &Server) -> usize {
    server
        .requested_paths()
        .iter()
        .filter(|path| path.contains("/projects/"))
        .count()
}

#[tokio::test]
async fn the_first_crawl_observes_every_claim() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();

    let report = refresh(&config, &mut state, FIRST).await;
    assert_eq!(report.manifests_fetched, 2);
    assert_eq!(report.events.len(), 2);
    assert!(
        report
            .events
            .iter()
            .all(|event| event.kind == EventKind::Observed)
    );

    let origin = &state.origins[&only_origin(&state)];
    assert_eq!(origin.health, OriginHealth::Healthy);
    assert_eq!(origin.site_name.as_deref(), Some("Fixture site"));
    let claim = &state.claims[&key(&state, CANDLELIGHT_ID)];
    assert_eq!(claim.health, ClaimHealth::Current);
    assert_eq!(claim.first_observed, FIRST);
    let held = state.manifest_bytes(&key(&state, CANDLELIGHT_ID)).unwrap();
    assert_eq!(
        held,
        pretty(&candlelight()).as_slice(),
        "manifests are kept as served"
    );
    assert_eq!(
        claim.ingested_sha256.as_deref(),
        Some(sha256_hex(held).as_str())
    );
}

#[tokio::test]
async fn an_unchanged_site_costs_no_manifest_downloads_and_no_events() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    server.forget_requests();
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(
        manifest_requests(&server),
        0,
        "{:?}",
        server.requested_paths()
    );
    assert!(report.events.is_empty());
    let claim = &state.claims[&key(&state, CANDLELIGHT_ID)];
    assert_eq!(claim.last_success.as_deref(), Some(SECOND));
    assert_eq!(claim.first_observed, FIRST);
}

#[tokio::test]
async fn a_new_release_is_one_changed_event_with_typed_changes() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    let mut updated = candlelight();
    let mut release = updated["releases"][1].clone();
    release["version"] = "1.2.0".into();
    release["runtimes"]["openmw"] = ">=0.50".into();
    updated["releases"]
        .as_array_mut()
        .unwrap()
        .insert(0, release);
    updated["channels"]["stable"]["version"] = "1.2.0".into();
    site.publish(&[updated, tallow()]);

    server.forget_requests();
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(
        manifest_requests(&server),
        1,
        "only the changed manifest is fetched"
    );
    assert_eq!(report.events.len(), 1);
    let event = &report.events[0];
    assert_eq!(event.kind, EventKind::Changed);
    assert_eq!(event.project.0, CANDLELIGHT_ID);
    assert!(event.before.is_some() && event.after.is_some());
    let text = serde_json::to_string(&event.changes).unwrap();
    assert!(text.contains("\"channel-head\""), "{text}");
    assert!(text.contains(">=0.50"), "{text}");

    // The same observations again: nothing new.
    let report = refresh(&config, &mut state, THIRD).await;
    assert!(report.events.is_empty());
}

#[tokio::test]
async fn an_outage_keeps_the_last_good_claims_and_recovers_on_its_own() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let held = state.manifests.clone();

    let routes = server.take_down();
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(report.unreachable_sources.len(), 1);
    assert!(
        report.events.is_empty(),
        "an outage is not a publication change"
    );
    let origin = &state.origins[&only_origin(&state)];
    assert_eq!(origin.health, OriginHealth::Unreachable);
    assert_eq!(origin.last_success.as_deref(), Some(FIRST));
    assert_eq!(origin.last_attempt, SECOND);
    for claim in state.claims.values() {
        assert_eq!(claim.health, ClaimHealth::OriginUnavailable);
        assert_eq!(claim.last_success.as_deref(), Some(FIRST));
    }
    assert_eq!(state.manifests, held, "the cached manifests survive");
    let source = state.sources.values().next().unwrap();
    assert_eq!(source.health, SourceHealth::Unreachable);

    server.restore(routes);
    let report = refresh(&config, &mut state, THIRD).await;
    assert!(report.unreachable_sources.is_empty());
    assert!(report.events.is_empty());
    assert_eq!(
        state.origins[&only_origin(&state)].health,
        OriginHealth::Healthy
    );
    assert!(
        state
            .claims
            .values()
            .all(|claim| claim.health == ClaimHealth::Current)
    );
}

#[tokio::test]
async fn an_invalid_manifest_never_replaces_the_valid_one() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let held = state
        .manifest_bytes(&key(&state, CANDLELIGHT_ID))
        .unwrap()
        .to_vec();

    let mut broken = candlelight();
    broken["channels"]["stable"]["version"] = "1.0.0".into();
    site.publish(&[broken, tallow()]);
    let report = refresh(&config, &mut state, SECOND).await;
    assert!(report.events.is_empty());
    let claim = &state.claims[&key(&state, CANDLELIGHT_ID)];
    assert_eq!(claim.health, ClaimHealth::Invalid);
    assert!(
        claim.problem.as_ref().unwrap().contains("channels.stable"),
        "{claim:?}"
    );
    assert_eq!(
        state.manifest_bytes(&key(&state, CANDLELIGHT_ID)).unwrap(),
        held.as_slice()
    );

    // The same broken bytes are not downloaded again.
    server.forget_requests();
    refresh(&config, &mut state, THIRD).await;
    assert_eq!(manifest_requests(&server), 0);
    assert_eq!(
        state.claims[&key(&state, CANDLELIGHT_ID)].health,
        ClaimHealth::Invalid
    );
}

#[tokio::test]
async fn a_half_finished_deployment_is_inconsistent_not_malicious() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let held = state
        .manifest_bytes(&key(&state, CANDLELIGHT_ID))
        .unwrap()
        .to_vec();

    // The new index is live, the CDN still serves the old manifest.
    let mut updated = candlelight();
    updated["project"]["summary"] = "Lights that know what time it is, now in 4K.".into();
    let new_bytes = pretty(&updated);
    site.serve_index(&site.index(&[
        site.entry(&updated, &new_bytes),
        site.entry(&tallow(), &pretty(&tallow())),
    ]));

    server.forget_requests();
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(
        manifest_requests(&server),
        3,
        "the first read and two retries: {:?}",
        server.requested_paths()
    );
    assert!(report.events.is_empty());
    let claim = &state.claims[&key(&state, CANDLELIGHT_ID)];
    assert_eq!(claim.health, ClaimHealth::Inconsistent);
    assert_eq!(claim.advertised_sha256, sha256_hex(&new_bytes));
    assert_eq!(
        claim.ingested_sha256.as_deref(),
        Some(sha256_hex(&held).as_str())
    );
    assert!(claim.problem.as_ref().unwrap().contains("deployment"));
    assert_eq!(
        state.manifest_bytes(&key(&state, CANDLELIGHT_ID)).unwrap(),
        held.as_slice()
    );

    // The deployment finishes.
    server.serve_json(&site.manifest_path(CANDLELIGHT_ID), new_bytes);
    let report = refresh(&config, &mut state, THIRD).await;
    assert_eq!(report.events.len(), 1);
    assert_eq!(
        state.claims[&key(&state, CANDLELIGHT_ID)].health,
        ClaimHealth::Current
    );
}

#[tokio::test]
async fn a_manifest_for_another_project_is_refused() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    let mut impostor = tallow();
    impostor["project"]["id"] = CANDLELIGHT_ID.into();
    let bytes = pretty(&impostor);
    server.serve_json(&site.manifest_path(TALLOW_ID), bytes.clone());
    let mut entry = site.entry(&tallow(), &bytes);
    entry["manifest"] = site
        .url(&format!("dreamweave/projects/{TALLOW_ID}.json"))
        .into();
    site.serve_index(&site.index(&[entry]));
    let config = config(&[site.url("dreamweave.json")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let claim = &state.claims[&key(&state, TALLOW_ID)];
    assert_eq!(claim.health, ClaimHealth::Invalid);
    assert!(
        claim
            .problem
            .as_ref()
            .unwrap()
            .contains("describes project")
    );
    assert!(state.manifests.is_empty());
}

#[tokio::test]
async fn withdrawal_and_return_are_events_and_history_stays() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    site.publish(&[candlelight()]);
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.events[0].kind, EventKind::Withdrawn);
    assert_eq!(
        state.claims[&key(&state, TALLOW_ID)].health,
        ClaimHealth::Withdrawn
    );
    assert!(state.manifest_bytes(&key(&state, TALLOW_ID)).is_some());

    site.publish(&[candlelight(), tallow()]);
    let report = refresh(&config, &mut state, THIRD).await;
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.events[0].kind, EventKind::Restored);
    assert_eq!(
        state.claims[&key(&state, TALLOW_ID)].health,
        ClaimHealth::Current
    );
}

#[tokio::test]
async fn one_uuid_at_two_origins_is_two_claims() {
    let server = Server::start();
    let first = Site::new(&server, "/alpha/");
    let second = Site::new(&server, "/beta/");
    first.publish(&[candlelight()]);
    let mut copy = candlelight();
    copy["project"]["name"] = "Candlelight (a different site's claim)".into();
    second.publish(&[copy]);
    let config = config(&[first.url(""), second.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    assert_eq!(state.origins.len(), 2);
    let claims: Vec<&ClaimKey> = state
        .claims
        .keys()
        .filter(|key| key.project.0 == CANDLELIGHT_ID)
        .collect();
    assert_eq!(claims.len(), 2, "neither claim overwrites the other");
    let names: Vec<&str> = state
        .claims
        .values()
        .map(|claim| claim.entry.name.as_str())
        .collect();
    assert!(names.contains(&"Candlelight"));
    assert!(names.contains(&"Candlelight (a different site's claim)"));
}

#[tokio::test]
async fn one_name_under_two_uuids_is_two_projects() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    let mut twin = candlelight();
    twin["project"]["id"] = "0e8a52b1-5d7f-4c1e-9a2b-6f3c8d1e4a70".into();
    site.publish(&[candlelight(), twin]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    assert_eq!(state.claims.len(), 2);
}

#[tokio::test]
async fn removing_a_source_removes_its_claims_and_nothing_else() {
    let server = Server::start();
    let kept = Site::new(&server, "/kept/");
    let dropped = Site::new(&server, "/dropped/");
    kept.publish(&[tallow()]);
    dropped.publish(&[candlelight()]);
    let mut state = State::default();
    refresh(&config(&[kept.url(""), dropped.url("")]), &mut state, FIRST).await;
    assert_eq!(state.claims.len(), 2);

    let report = refresh(&config(&[kept.url("")]), &mut state, SECOND).await;
    assert_eq!(report.removed_origins.len(), 1);
    assert_eq!(state.claims.len(), 1);
    assert_eq!(state.sources.len(), 1);
    assert_eq!(state.claims.keys().next().unwrap().project.0, TALLOW_ID);
}

#[tokio::test]
async fn a_moved_site_continues_its_claims_without_a_conflict() {
    let server = Server::start();
    let old = Site::new(&server, "/old/");
    old.publish(&[candlelight()]);
    let config = config(&[old.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    let old_origin = only_origin(&state);

    let new = Site::new(&server, "/new/");
    new.publish(&[candlelight()]);
    server.redirect("/old/", "/new/");
    let report = refresh(&config, &mut state, SECOND).await;

    let new_origin = only_origin(&state);
    assert_ne!(new_origin, old_origin);
    assert_eq!(report.removed_origins, vec![old_origin.clone()]);
    let origin = &state.origins[&new_origin];
    assert_eq!(origin.previous.len(), 1);
    assert!(origin.previous[0].evidence.contains("redirects"));
    let event = &report.events[0];
    assert_eq!(event.kind, EventKind::Observed);
    assert_eq!(event.moved_from.as_deref(), Some(old_origin.as_str()));
    assert!(
        event.changes.is_empty(),
        "the same manifest at a new address: {:?}",
        event.changes
    );
}

#[tokio::test]
async fn events_are_never_duplicated_even_when_claims_are_lost() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;
    assert_eq!(state.events.len(), 1);

    // Lose the claims but keep the history, as after restoring events from an old backup.
    state.claims.clear();
    state.manifests.clear();
    let report = refresh(&config, &mut state, SECOND).await;
    assert!(report.events.is_empty());
    assert_eq!(state.events.len(), 1);
}

#[tokio::test]
async fn state_survives_a_round_trip_through_disk_byte_for_byte() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let config = config(&[site.url("")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    let directory = tempdir();
    state.save(&directory).unwrap();
    let loaded = State::load(&directory).unwrap();
    assert_eq!(loaded.claims, state.claims);
    assert_eq!(loaded.manifests, state.manifests);
    assert_eq!(loaded.events, state.events);
    assert_eq!(loaded.origins, state.origins);

    let before = snapshot(&directory);
    loaded.save(&directory).unwrap();
    assert_eq!(snapshot(&directory), before);
}

#[tokio::test]
async fn an_unreachable_site_that_was_never_read_is_a_source_problem_only() {
    let server = Server::start();
    server.route("/down/", support::Route::Hangup);
    let config = config(&[server.url("/down/").to_string()]);
    let mut state = State::default();
    let report = refresh(&config, &mut state, FIRST).await;
    assert_eq!(report.unreachable_sources.len(), 1);
    assert!(state.origins.is_empty());
    assert_eq!(
        state.sources.values().next().unwrap().health,
        SourceHealth::Unreachable
    );
}

#[tokio::test]
async fn a_newer_protocol_is_reported_and_nothing_is_guessed() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[tallow()]);
    let config = config(&[site.url("dreamweave.json")]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    let mut future = site.index(&[]);
    future["schema_version"] = "3".into();
    future["projects"] = json!([{ "id": TALLOW_ID, "whatever": "comes next" }]);
    site.serve_index(&future);
    refresh(&config, &mut state, SECOND).await;
    let source = state.sources.values().next().unwrap();
    assert_eq!(source.health, SourceHealth::UnsupportedVersion);
    assert_eq!(
        state.claims.values().next().unwrap().health,
        ClaimHealth::OriginUnavailable
    );
    assert!(state.manifest_bytes(&key(&state, TALLOW_ID)).is_some());
}

fn tempdir() -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "dreamweave-network-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn snapshot(directory: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}
