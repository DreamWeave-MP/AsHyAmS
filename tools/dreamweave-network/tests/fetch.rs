mod support;

use dreamweave_network::{
    address::AddressPolicy,
    fetch::{FetchError, Fetcher, Validators},
    policy,
};
use support::{Route, Server};
use url::Url;

fn drill_fetcher() -> Fetcher {
    Fetcher::new(AddressPolicy::LoopbackOnly).unwrap()
}

fn real_fetcher() -> Fetcher {
    Fetcher::new(AddressPolicy::PublicOnly).unwrap()
}

#[tokio::test]
async fn follows_redirects_and_reports_the_chain() {
    let server = Server::start();
    server.redirect("/old/", "/new/");
    server.redirect("/new/", &format!("{}/final.json", server.origin()));
    server.serve_json("/final.json", "{}");
    let response = drill_fetcher()
        .get(&server.url("/old/"), 1024, None)
        .await
        .unwrap();
    assert_eq!(response.url, server.url("/final.json"));
    assert_eq!(
        response.redirects,
        vec![server.url("/old/"), server.url("/new/")]
    );
    assert_eq!(response.media_type.as_deref(), Some("application/json"));
    assert_eq!(response.body, b"{}");
}

#[tokio::test]
async fn refuses_loopback_under_the_real_policy() {
    let server = Server::start();
    server.serve_json("/dreamweave.json", "{}");
    let error = real_fetcher()
        .get(&server.url("/dreamweave.json"), 1024, None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, FetchError::ForbiddenAddress { .. }),
        "{error}"
    );
    assert!(error.is_policy_refusal());
    assert!(
        server.requests().is_empty(),
        "a refused address must never be connected to"
    );
}

#[tokio::test]
async fn refuses_localhost_by_name() {
    let error = real_fetcher()
        .get(&Url::parse("http://localhost:9/").unwrap(), 1024, None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, FetchError::ForbiddenAddress { .. }),
        "{error}"
    );
}

#[tokio::test]
async fn refuses_private_link_local_and_metadata_literals() {
    for url in [
        "http://10.0.0.1/",
        "http://192.168.1.1/dreamweave.json",
        "http://172.16.5.4/",
        "http://169.254.169.254/latest/meta-data/",
        "http://[fe80::1]/",
        "http://[::1]/",
        "http://0.0.0.0/",
    ] {
        let error = real_fetcher()
            .get(&Url::parse(url).unwrap(), 1024, None)
            .await
            .unwrap_err();
        assert!(
            matches!(error, FetchError::ForbiddenAddress { .. }),
            "{url}: {error}"
        );
    }
}

#[tokio::test]
async fn refuses_a_redirect_into_a_private_network() {
    let server = Server::start();
    server.redirect(
        "/dreamweave.json",
        "http://169.254.169.254/latest/meta-data/",
    );
    let error = drill_fetcher()
        .get(&server.url("/dreamweave.json"), 1024, None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, FetchError::ForbiddenAddress { .. }),
        "{error}"
    );
}

#[tokio::test]
async fn refuses_a_redirect_to_another_scheme() {
    let server = Server::start();
    server.redirect("/dreamweave.json", "file:///etc/passwd");
    let error = drill_fetcher()
        .get(&server.url("/dreamweave.json"), 1024, None)
        .await
        .unwrap_err();
    assert_eq!(error, FetchError::UnsupportedScheme("file".to_owned()));
}

#[tokio::test]
async fn refuses_non_http_schemes_and_credentials() {
    for (url, expected) in [
        (
            "file:///etc/passwd",
            FetchError::UnsupportedScheme("file".to_owned()),
        ),
        (
            "ftp://example.org/dreamweave.json",
            FetchError::UnsupportedScheme("ftp".to_owned()),
        ),
        (
            "https://user:secret@example.org/dreamweave.json",
            FetchError::CredentialsInUrl,
        ),
    ] {
        let error = real_fetcher()
            .get(&Url::parse(url).unwrap(), 1024, None)
            .await
            .unwrap_err();
        assert_eq!(error, expected, "{url}");
    }
}

#[tokio::test]
async fn caps_redirect_chains() {
    let server = Server::start();
    for hop in 0..=policy::MAXIMUM_REDIRECTS {
        server.redirect(&format!("/hop/{hop}"), &format!("/hop/{}", hop + 1));
    }
    server.serve_json(&format!("/hop/{}", policy::MAXIMUM_REDIRECTS + 1), "{}");
    let error = drill_fetcher()
        .get(&server.url("/hop/0"), 1024, None)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        FetchError::TooManyRedirects {
            limit: policy::MAXIMUM_REDIRECTS
        }
    );
}

#[tokio::test]
async fn refuses_oversized_bodies() {
    let server = Server::start();
    server.serve_json("/huge.json", vec![b' '; 4096]);
    let error = drill_fetcher()
        .get(&server.url("/huge.json"), 1024, None)
        .await
        .unwrap_err();
    assert_eq!(error, FetchError::TooLarge { limit: 1024 });
    assert!(error.is_policy_refusal());
}

#[tokio::test]
async fn reports_status_codes_and_outages() {
    let server = Server::start();
    let error = drill_fetcher()
        .get(&server.url("/missing"), 1024, None)
        .await
        .unwrap_err();
    assert_eq!(error, FetchError::Status(404));

    server.route("/down", Route::Hangup);
    let error = drill_fetcher()
        .get(&server.url("/down"), 1024, None)
        .await
        .unwrap_err();
    assert!(matches!(error, FetchError::Connection(_)), "{error}");
    assert!(!error.is_policy_refusal());
}

#[tokio::test]
async fn sends_validators_and_understands_not_modified() {
    let server = Server::start();
    server.route(
        "/dreamweave.json",
        Route::Respond {
            status: 304,
            headers: vec![("ETag".to_owned(), "\"abc\"".to_owned())],
            body: Vec::new(),
        },
    );
    let validators = Validators {
        etag: Some("\"abc\"".to_owned()),
        last_modified: None,
    };
    let response = drill_fetcher()
        .get(&server.url("/dreamweave.json"), 1024, Some(&validators))
        .await
        .unwrap();
    assert!(response.not_modified);
    let request = &server.requests()[0];
    assert!(
        request
            .headers
            .contains(&("if-none-match".to_owned(), "\"abc\"".to_owned()))
    );
    let user_agent = request
        .headers
        .iter()
        .find(|(name, _)| name == "user-agent")
        .map(|(_, value)| value.as_str());
    assert_eq!(user_agent, Some(policy::USER_AGENT));
}
