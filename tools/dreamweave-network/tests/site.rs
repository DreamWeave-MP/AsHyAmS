//! A fixture network with every state the site has to show, crawled twice through real HTTP and
//! generated as a site: a new release with breaking and migration notes, a yank, an artifact
//! changing under an unchanged version, a rolling development build, a program built per
//! platform, an identity conflict, an unreachable site and an invalid publication.
//!
//! Set `DREAMWEAVE_NETWORK_FIXTURE_SITE` to a directory to keep the result. CI renders it with
//! `zola --root <directory> build` and checks its links, so the templates are exercised on every
//! pull request without the network.

mod support;

use std::{fs, path::Path};

use dreamweave_network::{
    config::Curation,
    network::Network,
    site,
    state::{ClaimHealth, State},
};
use serde_json::{Value, json};
use support::{
    Server,
    sites::{Site, candlelight, config, program, refresh, tallow},
};

const FIRST: &str = "2026-09-27T12:00:00Z";
const SECOND: &str = "2026-09-28T18:00:00Z";

fn fictional(id: &str, name: &str, project_type: &str, summary: &str) -> Value {
    let mut manifest = tallow();
    manifest["project"]["id"] = id.into();
    manifest["project"]["name"] = name.into();
    manifest["project"]["type"] = project_type.into();
    manifest["project"]["summary"] =
        format!("{summary} A fixture: this project does not exist.").into();
    manifest
}

fn ashfall() -> Value {
    let mut manifest = fictional(
        "3e7c2d10-8a4b-4f6e-9c1d-5b2a7e9f0c48",
        "Ashfall Weather",
        "mod",
        "Ash storms that dim every flame they pass.",
    );
    manifest["project"]["tags"] = json!(["Weather", "OpenMW"]);
    for release in manifest["releases"].as_array_mut().unwrap() {
        release["relationships"] = json!([
            { "kind": "requires", "capability": "dreamweave:dynamic-lights", "reason": "Any scheduled-lights mod will do." },
            { "kind": "recommends", "name": "Patch for Purists", "url": "https://www.nexusmods.com/morrowind/mods/45096" }
        ]);
    }
    manifest
}

fn candlelight_moved_on() -> Value {
    let mut manifest = candlelight();
    let releases = manifest["releases"].as_array_mut().unwrap();
    let mut stable = releases[1].clone();
    stable["version"] = "1.2.0".into();
    stable["date"] = "2026-09-28".into();
    stable["runtimes"]["openmw"] = ">=0.50".into();
    stable["relationships"][0]["version"] = ">=1.1".into();
    stable["notes"] = json!({
        "summary": "Storm lanterns, and Tallow 1.1.",
        "breaking": ["The `candlelight.cfg` schedule format changed; old files are ignored."],
        "migration": "Delete `candlelight.cfg`, start the game once, and set your schedule again.",
        "added": ["Lanterns gutter in ash storms."]
    });
    stable["artifacts"][0]["digests"]["sha256"] = "1".repeat(64).into();
    stable["artifacts"][0]["size"] = 260_001.into();
    let mut development = releases[0].clone();
    development["version"] = "1.2.1-dev.3".into();
    // Candlelight 1.1.0 gets new bytes under its old version: the anomaly the site surfaces.
    releases[1]["artifacts"][0]["digests"]["sha256"] = "2".repeat(64).into();
    releases[2]["status"] = "yanked".into();
    releases[2]["yanked"] =
        json!({ "reason": "Corrupts saves made during an ash storm.", "replacement": "1.1.0" });
    releases.remove(0);
    releases.insert(0, stable);
    releases.insert(0, development);
    manifest["channels"] =
        json!({ "stable": { "version": "1.2.0" }, "development": { "version": "1.2.1-dev.3" } });
    manifest
}

fn copy_into(from: &Path, to: &Path) {
    if from.is_dir() {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap().path();
            copy_into(&entry, &to.join(entry.file_name().unwrap()));
        }
    } else {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(from, to).unwrap();
    }
}

#[tokio::test]
async fn a_fixture_network_with_every_state_builds_a_site() {
    let server = Server::start();
    let template = Site::new(&server, "/mod-template/");
    let elsewhere = Site::new(&server, "/elsewhere/");
    let flaky = Site::new(&server, "/flaky/");
    let broken = Site::new(&server, "/broken/");
    template.publish(&[candlelight(), tallow(), program()]);
    let mut impostor = tallow();
    impostor["project"]["name"] = "Tallow (another site's claim)".into();
    elsewhere.publish(&[impostor]);
    flaky.publish(&[ashfall()]);
    let glass = fictional(
        "c5a1e8f2-4d3b-4a7c-8e9f-1b2c3d4e5f60",
        "Lantern Glass",
        "assets",
        "Stained glass textures for every lantern in Vvardenfell.",
    );
    broken.publish(std::slice::from_ref(&glass));

    let config = config(&[
        template.url(""),
        elsewhere.url("dreamweave.json"),
        flaky.url(""),
        broken.url(""),
    ]);
    let mut state = State::default();
    refresh(&config, &mut state, FIRST).await;

    template.publish(&[candlelight_moved_on(), tallow(), program()]);
    let mut invalid = glass;
    invalid["channels"]["stable"]["version"] = "9.9.9".into();
    broken.publish(&[invalid]);
    let _ = flaky.server.take_down_prefix("/flaky/");
    let report = refresh(&config, &mut state, SECOND).await;
    assert_eq!(report.unreachable_sources.len(), 1);

    let network = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(network.conflicts.len(), 1);
    let healths: Vec<ClaimHealth> = network
        .claims
        .values()
        .map(|claim| claim.record.health)
        .collect();
    for expected in [
        ClaimHealth::Current,
        ClaimHealth::OriginUnavailable,
        ClaimHealth::Invalid,
    ] {
        assert!(healths.contains(&expected), "{healths:?}");
    }

    let keep = std::env::var_os("DREAMWEAVE_NETWORK_FIXTURE_SITE");
    let root = keep.as_ref().map_or_else(
        || std::env::temp_dir().join(format!("dreamweave-network-site-{}", std::process::id())),
        std::path::PathBuf::from,
    );
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for entry in [
        "zola.toml",
        "pages",
        "templates",
        "sass",
        "static",
        "data/schematics",
    ] {
        copy_into(&repository.join(entry), &root.join(entry));
    }
    let state_directory = root.join("state");
    state.save(&state_directory).unwrap();
    let built = site::build(&root, &state_directory, &network, None).unwrap();
    assert!(built.pages > 50, "{}", built.pages);
    for generated in [
        "content/_index.md",
        "content/conflicts/_index.md",
        "content/updates/anomaly.md",
        "static/network-data/catalog.json",
        "static/network-data/search.json",
        "static/network-data/updates.xml",
    ] {
        assert!(
            root.join(generated).is_file(),
            "{generated} was not written"
        );
    }
    let catalog: Value =
        serde_json::from_slice(&fs::read(root.join("static/network-data/catalog.json")).unwrap())
            .unwrap();
    assert!(
        catalog["claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|claim| claim["cached"] == true)
    );
    if keep.is_none() {
        fs::remove_dir_all(&root).unwrap();
    }
}
