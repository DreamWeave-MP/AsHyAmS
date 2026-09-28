//! HTTP GET for untrusted URLs.
//!
//! Three rules make this safe to point at a list of strangers' websites:
//!
//! 1. Names are resolved by [`GuardedResolver`] and nothing else. A name with any address the
//!    [`AddressPolicy`] refuses is refused outright, and the connection goes to the addresses that
//!    were checked, so there is no second lookup for DNS rebinding to win.
//! 2. Redirects are followed here, one hop at a time, and every hop gets the same scheme, address
//!    and credential checks as the first request. `reqwest` never follows one on its own.
//! 3. Bodies are read in chunks and abandoned the moment they pass the caller's limit, whatever
//!    `Content-Length` claimed.
//!
//! No proxy is ever used: a proxy would do its own resolution, and rule 1 would be decoration.

use std::{error::Error as _, fmt, net::IpAddr};

use reqwest::{
    StatusCode,
    dns::{Addrs, Name, Resolve, Resolving},
    header, redirect,
};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::{address::AddressPolicy, policy};

/// HTTP cache validators. A transport optimization only: `manifest_sha256` stays the protocol's
/// change signal, and a server that ignores these costs one full download.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validators {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

impl Validators {
    pub fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    /// Where the body came from, after redirects.
    pub url: Url,
    /// Every URL that answered with a redirect, in the order they were visited.
    pub redirects: Vec<Url>,
    /// The `Content-Type` without parameters, lowercased.
    pub media_type: Option<String>,
    pub body: Vec<u8>,
    pub validators: Validators,
    /// The server answered 304 to our validators; `body` is empty.
    pub not_modified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    InvalidUrl(String),
    UnsupportedScheme(String),
    CredentialsInUrl,
    ForbiddenAddress { host: String, address: IpAddr },
    NameNotResolved { host: String, reason: String },
    TooManyRedirects { limit: usize },
    RedirectWithoutLocation { status: u16 },
    Status(u16),
    TooLarge { limit: u64 },
    Timeout,
    Connection(String),
}

impl FetchError {
    /// Refused by this crawler's own rules rather than failed by the network or the server.
    pub fn is_policy_refusal(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedScheme(_)
                | Self::CredentialsInUrl
                | Self::ForbiddenAddress { .. }
                | Self::TooManyRedirects { .. }
                | Self::TooLarge { .. }
        )
    }
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(reason) => write!(formatter, "not a usable URL: {reason}"),
            Self::UnsupportedScheme(scheme) => write!(
                formatter,
                "refused by crawler policy: {scheme}: URLs are not fetched, only http and https"
            ),
            Self::CredentialsInUrl => formatter
                .write_str("refused by crawler policy: the URL carries a user name or password"),
            Self::ForbiddenAddress { host, address } if *host == address.to_string() => write!(
                formatter,
                "refused by crawler policy: {address} is not a public address"
            ),
            Self::ForbiddenAddress { host, address } => write!(
                formatter,
                "refused by crawler policy: {host} resolves to {address}, which is not a public address"
            ),
            Self::NameNotResolved { host, reason } => {
                write!(formatter, "could not resolve {host}: {reason}")
            }
            Self::TooManyRedirects { limit } => write!(
                formatter,
                "refused by crawler policy: more than {limit} redirects"
            ),
            Self::RedirectWithoutLocation { status } => {
                write!(
                    formatter,
                    "HTTP {status} redirect without a Location header"
                )
            }
            Self::Status(status) => write!(formatter, "HTTP {status}"),
            Self::TooLarge { limit } => write!(
                formatter,
                "refused by crawler policy: the response is larger than {limit} bytes"
            ),
            Self::Timeout => formatter.write_str("timed out"),
            Self::Connection(reason) => write!(formatter, "connection failed: {reason}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// The error [`GuardedResolver`] hands back through `reqwest`, recognised again on the way out.
#[derive(Debug)]
struct RefusedResolution {
    host: String,
    address: IpAddr,
}

impl fmt::Display for RefusedResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} resolves to {}", self.host, self.address)
    }
}

impl std::error::Error for RefusedResolution {}

struct GuardedResolver {
    addresses: AddressPolicy,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let addresses = self.addresses;
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let resolved: Vec<_> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            // A name with one private address among public ones is either misconfigured or
            // an attempt to get lucky with address selection. Neither gets a connection.
            if let Some(refused) = resolved
                .iter()
                .find(|address| !addresses.permits(address.ip()))
            {
                return Err(Box::new(RefusedResolution {
                    host,
                    address: refused.ip(),
                })
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(resolved.into_iter()) as Addrs)
        })
    }
}

pub struct Fetcher {
    client: reqwest::Client,
    addresses: AddressPolicy,
}

impl Fetcher {
    pub fn new(addresses: AddressPolicy) -> Result<Self, FetchError> {
        let client = reqwest::Client::builder()
            .user_agent(policy::USER_AGENT)
            .connect_timeout(policy::CONNECT_TIMEOUT)
            .timeout(policy::REQUEST_TIMEOUT)
            .redirect(redirect::Policy::none())
            .no_proxy()
            .dns_resolver(GuardedResolver { addresses })
            .build()
            .map_err(|error| FetchError::Connection(error.to_string()))?;
        Ok(Self { client, addresses })
    }

    /// The checks every URL gets before a connection is attempted, redirect targets included.
    pub fn check_url(&self, url: &Url) -> Result<(), FetchError> {
        check_url(url, self.addresses)
    }

    /// GET `url`, following at most [`policy::MAXIMUM_REDIRECTS`] redirects, reading at most
    /// `limit` bytes. Anything but a 2xx (or a 304 answering `validators`) is an error.
    pub async fn get(
        &self,
        url: &Url,
        limit: u64,
        validators: Option<&Validators>,
    ) -> Result<Response, FetchError> {
        let mut current = url.clone();
        let mut redirects = Vec::new();
        loop {
            self.check_url(&current)?;
            let mut request = self.client.get(current.clone());
            if let Some(validators) = validators {
                if let Some(etag) = &validators.etag {
                    request = request.header(header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = &validators.last_modified {
                    request = request.header(header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            let mut response = request
                .send()
                .await
                .map_err(|error| classify(&current, &error))?;
            let status = response.status();

            if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
                if redirects.len() == policy::MAXIMUM_REDIRECTS {
                    return Err(FetchError::TooManyRedirects {
                        limit: policy::MAXIMUM_REDIRECTS,
                    });
                }
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or(FetchError::RedirectWithoutLocation {
                        status: status.as_u16(),
                    })?;
                let next = current
                    .join(location)
                    .map_err(|error| FetchError::InvalidUrl(format!("{location:?}: {error}")))?;
                redirects.push(current);
                current = next;
                continue;
            }

            let received = read_validators(response.headers());
            if status == StatusCode::NOT_MODIFIED && validators.is_some() {
                return Ok(Response {
                    url: current,
                    redirects,
                    media_type: None,
                    body: Vec::new(),
                    validators: received,
                    not_modified: true,
                });
            }
            if !status.is_success() {
                return Err(FetchError::Status(status.as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|length| length > limit)
            {
                return Err(FetchError::TooLarge { limit });
            }
            let media_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty());

            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| classify(&current, &error))?
            {
                if (body.len() + chunk.len()) as u64 > limit {
                    return Err(FetchError::TooLarge { limit });
                }
                body.extend_from_slice(&chunk);
            }
            return Ok(Response {
                url: current,
                redirects,
                media_type,
                body,
                validators: received,
                not_modified: false,
            });
        }
    }
}

pub fn check_url(url: &Url, addresses: AddressPolicy) -> Result<(), FetchError> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(FetchError::UnsupportedScheme(other.to_owned())),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::CredentialsInUrl);
    }
    // Literal addresses never reach the resolver, so they are checked here.
    let literal = match url.host() {
        Some(Host::Ipv4(address)) => Some(IpAddr::V4(address)),
        Some(Host::Ipv6(address)) => Some(IpAddr::V6(address)),
        Some(Host::Domain(_)) => None,
        None => return Err(FetchError::InvalidUrl(format!("{url} has no host"))),
    };
    if let Some(address) = literal
        && !addresses.permits(address)
    {
        return Err(FetchError::ForbiddenAddress {
            host: address.to_string(),
            address,
        });
    }
    Ok(())
}

fn read_validators(headers: &header::HeaderMap) -> Validators {
    let text = |name| {
        headers
            .get(name)
            .and_then(|value: &header::HeaderValue| value.to_str().ok())
            .map(str::to_owned)
    };
    Validators {
        etag: text(header::ETAG),
        last_modified: text(header::LAST_MODIFIED),
    }
}

fn classify(url: &Url, error: &reqwest::Error) -> FetchError {
    let mut source = error.source();
    while let Some(cause) = source {
        if let Some(refused) = cause.downcast_ref::<RefusedResolution>() {
            return FetchError::ForbiddenAddress {
                host: refused.host.clone(),
                address: refused.address,
            };
        }
        source = cause.source();
    }
    if error.is_timeout() {
        return FetchError::Timeout;
    }
    let host = url.host_str().unwrap_or_default().to_owned();
    let mut reason = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        reason = cause.to_string();
        source = cause.source();
    }
    if error.is_connect() && reason.contains("failed to lookup address") {
        return FetchError::NameNotResolved { host, reason };
    }
    FetchError::Connection(reason)
}
