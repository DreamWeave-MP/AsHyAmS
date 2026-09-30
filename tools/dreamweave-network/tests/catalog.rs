//! The published aggregate matches its own schema and is byte-identical across builds.

mod support;

use dreamweave_network::{
    catalog::{self, IndexInfo},
    config::Curation,
    network::Network,
    state::State,
};
use support::{
    Server,
    sites::{Site, candlelight, config, refresh, tallow},
};

fn index() -> IndexInfo {
    IndexInfo {
        name: "AsHyAmS".to_owned(),
        url: "https://dreamweave-mp.github.io/AsHyAmS".to_owned(),
        repository: "https://github.com/DreamWeave-MP/AsHyAmS".to_owned(),
    }
}

#[tokio::test]
async fn the_catalog_and_events_match_the_published_schema() {
    let server = Server::start();
    let first = Site::new(&server, "/alpha/");
    let second = Site::new(&server, "/beta/");
    first.publish(&[candlelight(), tallow()]);
    second.publish(&[tallow()]);
    let mut state = State::default();
    refresh(
        &config(&[first.url(""), second.url("")]),
        &mut state,
        "2026-09-28T00:00:00Z",
    )
    .await;
    let network = Network::build(&state, &Curation::default()).unwrap();

    let document = serde_json::to_value(catalog::catalog(&network, &index())).unwrap();
    catalog::check(&document).unwrap();
    assert_eq!(document["claims"].as_array().unwrap().len(), 3);
    assert_eq!(document["conflicts"].as_array().unwrap().len(), 1);

    let events = serde_json::to_value(catalog::events(&network, &index())).unwrap();
    catalog::check(&events).unwrap();

    let again = Network::build(&state, &Curation::default()).unwrap();
    assert_eq!(
        serde_json::to_vec_pretty(&catalog::catalog(&network, &index())).unwrap(),
        serde_json::to_vec_pretty(&catalog::catalog(&again, &index())).unwrap(),
        "the same state builds the same bytes"
    );
}
