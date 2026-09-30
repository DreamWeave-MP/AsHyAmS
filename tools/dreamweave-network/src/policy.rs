//! This index's crawler policy: how it identifies itself, how long it waits, and how much it is
//! willing to read from somebody else's server.
//!
//! None of this is DreamWeave protocol. The protocol permits a manifest of any size; this crawler
//! refuses to read one larger than [`MAXIMUM_MANIFEST_BYTES`]. A document refused here is reported
//! as "refused by crawler policy", never as invalid. Another index is free to pick other numbers.
//!
//! The limits are sized from real publishers: a Mod Template release with four components, an
//! OpenMW extension block and one artifact is about 6 KiB of manifest, so the manifest limit
//! leaves room for more than two thousand releases. A project needing more has bigger problems.

use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "AsHyAmS/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/DreamWeave-MP/AsHyAmS)"
);

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// One request, from sending it to the last byte of the body.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Everything done for one source: discovery, index, changed manifests, retries, media.
pub const SOURCE_DEADLINE: Duration = Duration::from_secs(240);
pub const MAXIMUM_REDIRECTS: usize = 5;

pub const MAXIMUM_HTML_BYTES: u64 = 2 * 1024 * 1024;
pub const MAXIMUM_INDEX_BYTES: u64 = 4 * 1024 * 1024;
pub const MAXIMUM_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
pub const MAXIMUM_MEDIA_BYTES: u64 = 2 * 1024 * 1024;

pub const MAXIMUM_PROJECTS_PER_SITE: usize = 500;
pub const MAXIMUM_RELEASES_PER_PROJECT: usize = 2_000;
pub const MAXIMUM_ARTIFACTS_PER_RELEASE: usize = 64;
pub const MAXIMUM_SOURCES_PER_ARTIFACT: usize = 32;
pub const MAXIMUM_RELATIONSHIPS_PER_RELEASE: usize = 256;
pub const MAXIMUM_COMPONENTS_PER_RELEASE: usize = 256;
pub const MAXIMUM_MEDIA_PER_PROJECT: usize = 128;
/// Any single string in a document: a name, a note, a URL.
pub const MAXIMUM_STRING_BYTES: usize = 64 * 1024;

/// Sources crawled at the same time. Different sources are usually different hosts.
pub const CONCURRENT_SOURCES: usize = 8;
/// Requests in flight to one site. A site publishing forty projects gets two at a time, not forty.
pub const CONCURRENT_REQUESTS_PER_SITE: usize = 2;

/// A manifest whose bytes do not match the digest its site index advertises is refetched after
/// each of these delays, index first, before the claim is reported as inconsistent. CDNs
/// catch up in seconds, not minutes.
pub const DEPLOYMENT_RETRY_DELAYS: [Duration; 2] =
    [Duration::from_secs(5), Duration::from_secs(20)];

/// Image types a cached thumbnail may have. SVG is not on the list: it is a document that can
/// carry script, not a picture.
pub const MEDIA_TYPES: [(&str, &str); 4] = [
    ("image/webp", "webp"),
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/gif", "gif"),
];
