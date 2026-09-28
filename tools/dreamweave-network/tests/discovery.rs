mod support;

use dreamweave_network::{
    address::AddressPolicy,
    discovery::{self, FailureKind, Method},
    fetch::Fetcher,
};
use support::Server;

const INDEX: &str = include_str!("fixtures/mod-template/dreamweave.json");
const CANDLELIGHT: &str = include_str!("fixtures/mod-template/candlelight.json");

fn fetcher() -> Fetcher {
    Fetcher::new(AddressPolicy::AllowLoopback).unwrap()
}

fn page(head: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><title>Candlelight</title>{head}</head><body><h1>Candlelight</h1></body></html>"
    )
}

async fn discover(server: &Server, path: &str) -> Result<discovery::Discovery, discovery::Failure> {
    discovery::discover(&fetcher(), &server.url(path), None).await
}

#[tokio::test]
async fn a_direct_index_url() {
    let server = Server::start();
    server.serve_json("/mods/dreamweave.json", INDEX);
    let found = discover(&server, "/mods/dreamweave.json").await.unwrap();
    assert_eq!(found.method, Method::Direct);
    assert_eq!(found.index_url(), &server.url("/mods/dreamweave.json"));
}

#[tokio::test]
async fn a_direct_manifest_url_walks_up_to_its_index() {
    let server = Server::start();
    server.serve_json("/mods/dreamweave.json", INDEX);
    server.serve_json("/mods/dreamweave/projects/candlelight.json", CANDLELIGHT);
    let found = discover(&server, "/mods/dreamweave/projects/candlelight.json")
        .await
        .unwrap();
    assert_eq!(found.method, Method::Manifest);
    assert_eq!(found.index_url(), &server.url("/mods/dreamweave.json"));
}

#[tokio::test]
async fn an_html_page_linking_the_index() {
    let server = Server::start();
    server.serve(
        "/mods/candlelight/",
        "text/html; charset=utf-8",
        page(r#"<link rel="stylesheet" href="/site.css"><link rel="alternate" type="application/vnd.dreamweave.index+json" href="../dreamweave.json">"#),
    );
    server.serve_json("/mods/dreamweave.json", INDEX);
    let found = discover(&server, "/mods/candlelight/").await.unwrap();
    assert_eq!(found.method, Method::IndexLink);
    assert_eq!(found.index_url(), &server.url("/mods/dreamweave.json"));
}

#[tokio::test]
async fn rel_and_type_are_matched_case_insensitively() {
    let server = Server::start();
    server.serve(
        "/mods/",
        "text/html",
        page(r#"<LINK REL="Alternate Feed" TYPE="Application/Vnd.DreamWeave.Index+JSON" HREF="/mods/dreamweave.json">"#),
    );
    server.serve_json("/mods/dreamweave.json", INDEX);
    assert_eq!(
        discover(&server, "/mods/").await.unwrap().method,
        Method::IndexLink
    );
}

#[tokio::test]
async fn an_html_page_linking_only_its_manifest() {
    let server = Server::start();
    server.serve(
        "/mods/candlelight/",
        "text/html",
        page(r#"<link rel="alternate" type="application/vnd.dreamweave.project+json" href="/mods/dreamweave/projects/candlelight.json">"#),
    );
    server.serve_json("/mods/dreamweave.json", INDEX);
    let found = discover(&server, "/mods/candlelight/").await.unwrap();
    assert_eq!(found.method, Method::ProjectLink);
    assert_eq!(found.index_url(), &server.url("/mods/dreamweave.json"));
}

#[tokio::test]
async fn a_page_without_links_falls_back_to_the_parent_path() {
    let server = Server::start();
    server.serve("/mods/candlelight/docs/events.html", "text/html", page(""));
    server.serve_json("/mods/dreamweave.json", INDEX);
    let found = discover(&server, "/mods/candlelight/docs/events.html")
        .await
        .unwrap();
    assert_eq!(found.method, Method::PathWalk);
    let tried: Vec<String> = found
        .trail
        .iter()
        .map(|attempt| attempt.url.path().to_owned())
        .collect();
    assert_eq!(
        tried,
        [
            "/mods/candlelight/docs/events.html",
            "/mods/candlelight/docs/dreamweave.json",
            "/mods/candlelight/dreamweave.json",
            "/mods/dreamweave.json",
        ]
    );
}

#[tokio::test]
async fn a_project_site_below_a_user_site_finds_its_own_index_first() {
    // GitHub Pages: you.github.io/ is the account's user site and you.github.io/cool-mods/ is a
    // project site. Both may publish an index; the nearer one belongs to the page.
    let server = Server::start();
    let mut user_site = serde_json::from_str::<serde_json::Value>(INDEX).unwrap();
    user_site["site"]["name"] = "Someone else's user site".into();
    server.serve_json("/dreamweave.json", user_site.to_string());
    server.serve_json("/cool-mods/dreamweave.json", INDEX);
    server.serve("/cool-mods/candlelight/", "text/html", page(""));
    let found = discover(&server, "/cool-mods/candlelight/").await.unwrap();
    assert_eq!(found.index_url(), &server.url("/cool-mods/dreamweave.json"));
}

#[tokio::test]
async fn a_moved_site_is_followed_through_its_redirect() {
    let server = Server::start();
    server.redirect("/old-home/", "/new-home/");
    server.serve(
        "/new-home/",
        "text/html",
        page(r#"<link rel="alternate" type="application/vnd.dreamweave.index+json" href="dreamweave.json">"#),
    );
    server.serve_json("/new-home/dreamweave.json", INDEX);
    let found = discover(&server, "/old-home/").await.unwrap();
    assert_eq!(found.index_url(), &server.url("/new-home/dreamweave.json"));
}

#[tokio::test]
async fn a_malformed_alternate_link_is_ignored_and_the_path_walked() {
    let server = Server::start();
    server.serve(
        "/mods/",
        "text/html",
        page(r#"<link rel="alternate" type="application/vnd.dreamweave.index+json" href="http://[::1">"#),
    );
    server.serve_json("/mods/dreamweave.json", INDEX);
    let found = discover(&server, "/mods/").await.unwrap();
    assert_eq!(found.method, Method::PathWalk);
    assert!(
        found
            .trail
            .iter()
            .any(|attempt| attempt.outcome.contains("ignored <link>"))
    );
}

#[tokio::test]
async fn an_advertised_index_that_is_missing_is_reported_not_guessed_around() {
    let server = Server::start();
    server.serve(
        "/mods/",
        "text/html",
        page(r#"<link rel="alternate" type="application/vnd.dreamweave.index+json" href="/elsewhere/dreamweave.json">"#),
    );
    server.serve_json("/mods/dreamweave.json", INDEX);
    let failure = discover(&server, "/mods/").await.unwrap_err();
    assert_eq!(failure.kind, FailureKind::BrokenAdvertisement);
}

#[tokio::test]
async fn nothing_anywhere_is_not_found() {
    let server = Server::start();
    server.serve("/plain/page.html", "text/html", page(""));
    let failure = discover(&server, "/plain/page.html").await.unwrap_err();
    assert_eq!(failure.kind, FailureKind::NotFound);
    assert_eq!(
        failure.trail.last().unwrap().url.path(),
        "/dreamweave.json",
        "the walk goes all the way to the origin's root"
    );
}

#[tokio::test]
async fn a_newer_protocol_stops_discovery() {
    let server = Server::start();
    let mut future = serde_json::from_str::<serde_json::Value>(INDEX).unwrap();
    future["schema_version"] = "3".into();
    server.serve_json("/mods/dreamweave.json", future.to_string());
    let failure = discover(&server, "/mods/").await.unwrap_err();
    assert_eq!(failure.kind, FailureKind::UnsupportedVersion);
}

#[tokio::test]
async fn an_unreachable_host_is_not_walked() {
    let server = Server::start();
    server.route("/mods/", support::Route::Hangup);
    let failure = discover(&server, "/mods/").await.unwrap_err();
    assert_eq!(failure.kind, FailureKind::Unreachable);
    assert_eq!(server.requested_paths(), ["/mods/"]);
}
