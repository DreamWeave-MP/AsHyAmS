//! The derived network: resolution, reverse dependencies, capabilities, gaps and conflicts, built
//! from real crawls of fixture sites.

mod support;

use dreamweave_network::{
    config::{Curation, Migration},
    network::{GapKey, Network, Resolution},
    protocol::{ProjectId, RelationshipKind},
    state::State,
};
use support::{
    Server,
    sites::{CANDLELIGHT_ID, Site, TALLOW_ID, candlelight, config, refresh, tallow},
};

const NOW: &str = "2026-09-28T00:00:00Z";

async fn crawled(sites: &[&Site<'_>]) -> State {
    let mut state = State::default();
    let urls: Vec<String> = sites.iter().map(|site| site.url("")).collect();
    refresh(&config(&urls), &mut state, NOW).await;
    state
}

#[tokio::test]
async fn candlelight_requires_tallow_and_tallow_knows_it() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let state = crawled(&[&site]).await;
    let network = Network::build(&state, &Curation::default()).unwrap();

    let candlelight_key = network
        .claims
        .keys()
        .find(|key| key.project.0 == CANDLELIGHT_ID)
        .unwrap()
        .clone();
    let tallow_key = network
        .claims
        .keys()
        .find(|key| key.project.0 == TALLOW_ID)
        .unwrap()
        .clone();

    let requires: Vec<_> = network
        .forward_edges(&candlelight_key)
        .into_iter()
        .filter(|edge| edge.kind() == RelationshipKind::Requires)
        .collect();
    assert_eq!(requires.len(), 1);
    assert_eq!(
        requires[0].resolution,
        Resolution::Project(vec![tallow_key.clone()])
    );
    assert_eq!(
        requires[0].satisfied,
        Some(true),
        "Tallow 1.0.0 satisfies >=1.0"
    );

    assert_eq!(
        network.used_by(&tallow_key).into_iter().collect::<Vec<_>>(),
        vec![candlelight_key.clone()]
    );
    assert!(network.conflicts.is_empty());

    let provided = &network.capabilities["dreamweave:dynamic-lights"];
    assert_eq!(provided.providers, vec![candlelight_key]);
}

#[tokio::test]
async fn relationships_without_an_indexed_target_are_gaps_grouped_honestly() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight(), tallow()]);
    let state = crawled(&[&site]).await;
    let network = Network::build(&state, &Curation::default()).unwrap();

    let tamriel = network
        .gaps
        .iter()
        .find(|gap| matches!(&gap.key, GapKey::Url { url } if url.contains("tamriel-rebuilt")))
        .expect("Tamriel Rebuilt is referenced by URL");
    assert!(tamriel.names.contains("Tamriel Rebuilt"));
    assert!(
        network
            .gaps
            .iter()
            .any(|gap| matches!(&gap.key, GapKey::Name { name } if name.starts_with("Other mods"))),
        "a name-only reference stays a name"
    );
    let candidates = network.candidates();
    assert!(
        candidates
            .iter()
            .any(|(url, _)| url.contains("tamriel-rebuilt.org"))
    );
}

#[tokio::test]
async fn an_unindexed_dependency_is_a_gap_not_an_error() {
    let server = Server::start();
    let site = Site::new(&server, "/mods/");
    site.publish(&[candlelight()]);
    let state = crawled(&[&site]).await;
    let network = Network::build(&state, &Curation::default()).unwrap();
    assert!(network.gaps.iter().any(|gap| gap.key
        == GapKey::Project {
            project: ProjectId(TALLOW_ID.to_owned())
        }));
}

#[tokio::test]
async fn the_same_id_at_two_sites_is_a_visible_conflict_and_both_stay() {
    let server = Server::start();
    let first = Site::new(&server, "/alpha/");
    let second = Site::new(&server, "/beta/");
    first.publish(&[candlelight(), tallow()]);
    let mut copy = tallow();
    copy["project"]["name"] = "Tallow, but somebody else's".into();
    second.publish(&[copy]);
    let state = crawled(&[&first, &second]).await;
    let network = Network::build(&state, &Curation::default()).unwrap();

    assert_eq!(network.conflicts.len(), 1);
    assert_eq!(network.conflicts[0].project.0, TALLOW_ID);
    assert_eq!(network.conflicts[0].claims.len(), 2);
    assert!(
        network
            .claims
            .values()
            .filter(|claim| claim.key.project.0 == TALLOW_ID)
            .all(|claim| claim.conflict && claim.is_listed())
    );
    // Candlelight's requirement now lands on both claims: the index does not pick one.
    let edge = network
        .edges
        .iter()
        .find(|edge| {
            edge.relationship
                .project
                .as_ref()
                .is_some_and(|id| id.0 == TALLOW_ID)
        })
        .unwrap();
    assert_eq!(edge.targets().len(), 2);
}

#[tokio::test]
async fn a_reviewed_migration_resolves_a_conflict_without_hiding_either_claim() {
    let server = Server::start();
    let old = Site::new(&server, "/old/");
    let new = Site::new(&server, "/new/");
    old.publish(&[tallow()]);
    new.publish(&[tallow()]);
    let state = crawled(&[&old, &new]).await;
    let curation = Curation {
        migration: vec![Migration {
            project: ProjectId(TALLOW_ID.to_owned()),
            from: old.url("dreamweave.json"),
            to: new.url("dreamweave.json"),
            reason: "The author moved the site.".to_owned(),
            reviewed: "2026-09-28".to_owned(),
        }],
        featured: Vec::new(),
    };
    let network = Network::build(&state, &curation).unwrap();
    assert!(network.conflicts.is_empty());
    let superseded: Vec<_> = network
        .claims
        .values()
        .filter(|claim| claim.superseded_by.is_some())
        .collect();
    assert_eq!(superseded.len(), 1);
    assert!(
        superseded[0].is_listed(),
        "a superseded claim is still shown"
    );
    assert_eq!(network.claims_of(&ProjectId(TALLOW_ID.to_owned())).len(), 1);
}
