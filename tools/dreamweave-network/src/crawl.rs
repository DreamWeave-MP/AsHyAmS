//! One refresh: every enrolled source read once and compared with what this index already holds.
//!
//! A refresh has two halves. The fetch half talks to the network, concurrently, and only reads
//! the state. The apply half never touches the network and writes the state in source order, so
//! the same observations always produce the same files and the same events.
//!
//! Per source:
//!
//! 1. Discover the site index from the enrolled URL.
//! 2. Validate it. For each project it lists, compare `manifest_sha256` with the digest of the
//!    manifest this index holds. Equal means unchanged: nothing is fetched.
//! 3. Fetch what changed and check the served bytes against the advertised digest. A mismatch is
//!    usually a deployment caught halfway, so wait and read both documents again before calling
//!    it inconsistent.
//! 4. Validate, diff against the held manifest, record events.
//!
//! Nothing a publisher does can fail the refresh. An unreachable site, a broken manifest, a
//! half-finished deployment: each becomes state on the claims it affects, and the last good
//! manifest stays exactly where it was.

use std::{collections::BTreeMap, time::Duration};

use futures_util::{StreamExt, stream};
use url::Url;

use crate::{
    config::Config,
    diff,
    discovery::{self, FailureKind, Known},
    fetch::{FetchError, Fetcher, Response},
    policy,
    protocol::{
        IndexEntry, Manifest, MediaKind, ProjectId,
        parse::{self, IndexReport, Problem},
    },
    state::{
        self, AttemptRecord, ClaimHealth, ClaimKey, ClaimRecord, Event, EventKind, MediaRecord,
        Move, OriginHealth, OriginRecord, Rejection, SourceHealth, SourceRecord, State,
    },
};

pub const CRAWLER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Crawler {
    pub fetcher: Fetcher,
    /// Waits between rereads of a manifest whose bytes do not match the advertised digest.
    pub retry_delays: Vec<Duration>,
    pub source_deadline: Duration,
}

impl Crawler {
    pub fn new(fetcher: Fetcher) -> Self {
        Self {
            fetcher,
            retry_delays: policy::DEPLOYMENT_RETRY_DELAYS.to_vec(),
            source_deadline: policy::SOURCE_DEADLINE,
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub sources: usize,
    pub unreachable_sources: Vec<String>,
    pub manifests_fetched: usize,
    pub events: Vec<Event>,
    pub removed_origins: Vec<String>,
}

impl Report {
    /// One line per fact, for the terminal and for the state branch's commit message.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!(
            "{} sources read, {} unavailable, {} manifests fetched, {} new events",
            self.sources,
            self.unreachable_sources.len(),
            self.manifests_fetched,
            self.events.len()
        )];
        for source in &self.unreachable_sources {
            lines.push(format!("unavailable: {source}"));
        }
        for event in &self.events {
            let kind = serde_json::to_value(event.kind).expect("an event kind serializes");
            lines.push(format!(
                "{}: {} ({})",
                kind.as_str().unwrap_or_default(),
                event.name,
                event.origin
            ));
        }
        for origin in &self.removed_origins {
            lines.push(format!("no longer enrolled: {origin}"));
        }
        lines.join("\n")
    }
}

// The fetch half ------------------------------------------------------------------------------

enum SourceOutcome {
    Failed(discovery::Failure),
    Resolved(Box<Resolved>),
}

struct Resolved {
    method: discovery::Method,
    trail: Vec<discovery::Attempt>,
    redirects: Vec<Url>,
    response: Response,
    origin: String,
    /// The index bytes this crawl read, or the cached ones when the server said 304.
    index_bytes: Vec<u8>,
    index: Result<IndexReport, Problem>,
    manifests: BTreeMap<ProjectId, ManifestOutcome>,
    media: BTreeMap<ProjectId, MediaOutcome>,
}

enum ManifestOutcome {
    Unchanged,
    StillRejected,
    Fetched {
        bytes: Vec<u8>,
        sha256: String,
        manifest: Box<Manifest>,
    },
    Invalid {
        sha256: String,
        problem: String,
    },
    Refused(String),
    Inconsistent {
        advertised: String,
        served: String,
    },
    Unreachable(String),
}

enum MediaOutcome {
    Keep,
    None,
    Cached {
        record: MediaRecord,
        bytes: Option<Vec<u8>>,
    },
}

/// The image a card may show: the featured item's thumbnail, else the featured image itself,
/// else any declared thumbnail. Never anything the manifest did not declare.
pub fn card_image(manifest: &Manifest) -> Option<(String, String)> {
    let media = &manifest.project.media;
    let featured = media.iter().find(|item| item.featured == Some(true));
    if let Some(item) = featured {
        if let Some(thumbnail) = &item.thumbnail {
            return Some((thumbnail.clone(), item.alt.clone()));
        }
        if item.kind == MediaKind::Image {
            return Some((item.url.clone(), item.alt.clone()));
        }
    }
    media
        .iter()
        .find_map(|item| Some((item.thumbnail.clone()?, item.alt.clone())))
}

fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

impl Crawler {
    pub async fn refresh(&self, config: &Config, state: &mut State, now: &str) -> Report {
        let mut sources: Vec<_> = config.sources.iter().collect();
        sources.sort_by(|left, right| left.url.cmp(&right.url));
        let snapshot: &State = state;
        let outcomes: Vec<(Url, SourceOutcome)> = stream::iter(sources.iter().map(|source| {
            let url = Url::parse(&source.url)
                .expect("sources were validated when the configuration loaded");
            async move {
                let outcome = match tokio::time::timeout(
                    self.source_deadline,
                    self.crawl_source(&url, snapshot),
                )
                .await
                {
                    Ok(outcome) => outcome,
                    Err(_) => SourceOutcome::Failed(discovery::Failure {
                        kind: FailureKind::Unreachable,
                        detail: format!(
                            "gave up after {} seconds, the crawler's limit for one source",
                            self.source_deadline.as_secs()
                        ),
                        trail: Vec::new(),
                    }),
                };
                (url, outcome)
            }
        }))
        .buffered(policy::CONCURRENT_SOURCES)
        .collect()
        .await;

        let mut report = Report {
            sources: outcomes.len(),
            ..Report::default()
        };
        apply(state, &outcomes, now, &mut report);
        state.network = Some(state::NetworkRecord {
            format: state::STATE_FORMAT,
            crawler: policy::USER_AGENT.to_owned(),
            observed_at: now.to_owned(),
        });
        report
    }

    async fn crawl_source(&self, source: &Url, state: &State) -> SourceOutcome {
        let previous_origin = state
            .sources
            .get(&state::source_id(source))
            .and_then(|record| record.origin.as_ref())
            .and_then(|origin| state.origins.get(origin));
        let known_url = previous_origin.and_then(|origin| Url::parse(&origin.index_url).ok());
        let known = match (&known_url, previous_origin) {
            (Some(index_url), Some(origin)) => Some(Known {
                index_url,
                validators: &origin.validators,
            }),
            _ => None,
        };
        let found = match discovery::discover(&self.fetcher, source, known).await {
            Ok(found) => found,
            Err(failure) => return SourceOutcome::Failed(failure),
        };
        let origin = state::origin_id(found.index_url());
        let index_bytes = if found.index.not_modified {
            if let Some(bytes) = state.indexes.get(&origin) {
                bytes.clone()
            } else {
                match self
                    .fetcher
                    .get(found.index_url(), policy::MAXIMUM_INDEX_BYTES, None)
                    .await
                {
                    Ok(response) => response.body,
                    Err(error) => {
                        return SourceOutcome::Failed(discovery::Failure {
                            kind: FailureKind::Unreachable,
                            detail: error.to_string(),
                            trail: found.trail,
                        });
                    }
                }
            }
        } else {
            found.index.body.clone()
        };
        let index = if index_bytes.len() as u64 > policy::MAXIMUM_INDEX_BYTES {
            Err(Problem::Policy(format!(
                "the site index is larger than {} bytes",
                policy::MAXIMUM_INDEX_BYTES
            )))
        } else {
            parse::parse_index(&index_bytes)
        };

        let mut manifests = BTreeMap::new();
        let mut media = BTreeMap::new();
        let index_url = found.index_url();
        if let Ok(report) = &index {
            let results: Vec<_> = stream::iter(report.entries().map(|entry| {
                let key = ClaimKey {
                    project: entry.id.clone(),
                    origin: origin.clone(),
                };
                async move {
                    let manifest = self
                        .manifest_outcome(entry, index_url, state.claims.get(&key))
                        .await;
                    let media = self.media_outcome(&manifest, &key, state).await;
                    (entry.id.clone(), manifest, media)
                }
            }))
            .buffered(policy::CONCURRENT_REQUESTS_PER_SITE)
            .collect()
            .await;
            for (project, manifest, image) in results {
                manifests.insert(project.clone(), manifest);
                media.insert(project, image);
            }
        }
        SourceOutcome::Resolved(Box::new(Resolved {
            method: found.method,
            trail: found.trail,
            redirects: found.redirects,
            response: found.index,
            origin,
            index_bytes,
            index,
            manifests,
            media,
        }))
    }

    async fn manifest_outcome(
        &self,
        entry: &IndexEntry,
        index_url: &Url,
        previous: Option<&ClaimRecord>,
    ) -> ManifestOutcome {
        if let Some(previous) = previous {
            if previous.ingested_sha256.as_deref() == Some(entry.manifest_sha256.as_str()) {
                return ManifestOutcome::Unchanged;
            }
            if previous.rejected.as_ref().is_some_and(|rejected| {
                rejected.sha256 == entry.manifest_sha256 && rejected.crawler == CRAWLER_VERSION
            }) {
                return ManifestOutcome::StillRejected;
            }
        }
        let Ok(url) = Url::parse(&entry.manifest) else {
            return ManifestOutcome::Invalid {
                sha256: entry.manifest_sha256.clone(),
                problem: format!("manifest URL {:?} does not parse", entry.manifest),
            };
        };
        let mut advertised = entry.manifest_sha256.clone();
        let mut delays = self.retry_delays.iter();
        loop {
            let response = match self
                .fetcher
                .get(&url, policy::MAXIMUM_MANIFEST_BYTES, None)
                .await
            {
                Ok(response) => response,
                Err(error) if error.is_policy_refusal() => {
                    return ManifestOutcome::Refused(format!("{url}: {error}"));
                }
                Err(error) => return ManifestOutcome::Unreachable(format!("{url}: {error}")),
            };
            let served = state::sha256_hex(&response.body);
            if served == advertised {
                return read_manifest(entry, response.body, served);
            }
            let Some(delay) = delays.next() else {
                return ManifestOutcome::Inconsistent { advertised, served };
            };
            tokio::time::sleep(*delay).await;
            // The site may have finished deploying in the meantime: believe its newest index.
            if let Some(newer) = self.readvertised(index_url, &entry.id).await {
                advertised = newer;
            }
        }
    }

    async fn readvertised(&self, index_url: &Url, project: &ProjectId) -> Option<String> {
        let response: Result<Response, FetchError> = self
            .fetcher
            .get(index_url, policy::MAXIMUM_INDEX_BYTES, None)
            .await;
        let report = parse::parse_index(&response.ok()?.body).ok()?;
        report
            .entries()
            .find(|entry| &entry.id == project)
            .map(|entry| entry.manifest_sha256.clone())
    }

    async fn media_outcome(
        &self,
        manifest: &ManifestOutcome,
        key: &ClaimKey,
        state: &State,
    ) -> MediaOutcome {
        let parsed;
        let current = if let ManifestOutcome::Fetched { manifest, .. } = manifest {
            manifest.as_ref()
        } else {
            let Some(bytes) = state.manifest_bytes(key) else {
                return MediaOutcome::Keep;
            };
            let Ok(manifest) = serde_json::from_slice::<Manifest>(bytes) else {
                return MediaOutcome::Keep;
            };
            parsed = manifest;
            &parsed
        };
        let Some((url, alt)) = card_image(current) else {
            return MediaOutcome::None;
        };
        let previous = state.claims.get(key).and_then(|claim| claim.media.as_ref());
        if previous.is_some_and(|media| media.url == url && media.alt == alt) {
            return MediaOutcome::Keep;
        }
        let mut record = MediaRecord {
            url: url.clone(),
            alt,
            file: None,
            problem: None,
        };
        let fetched = match Url::parse(&url) {
            Ok(parsed) => self
                .fetcher
                .get(&parsed, policy::MAXIMUM_MEDIA_BYTES, None)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let bytes = match fetched {
            Ok(response) => {
                let declared = response.media_type.as_deref().unwrap_or_default();
                let sniffed = sniff_image(&response.body);
                let accepted = policy::MEDIA_TYPES.iter().find(|(media_type, _)| {
                    Some(*media_type) == sniffed && *media_type == declared
                });
                if let Some((_, extension)) = accepted {
                    record.file = Some(format!(
                        "media/{}.{extension}",
                        state::sha256_hex(&response.body)
                    ));
                    Some(response.body)
                } else {
                    record.problem = Some(format!(
                        "not cached: served as {declared:?}, which is not a WebP, PNG, JPEG or GIF image this index caches"
                    ));
                    None
                }
            }
            Err(error) => {
                record.problem = Some(format!("not cached: {error}"));
                None
            }
        };
        MediaOutcome::Cached { record, bytes }
    }
}

fn read_manifest(entry: &IndexEntry, bytes: Vec<u8>, sha256: String) -> ManifestOutcome {
    match parse::parse_manifest(&bytes) {
        Ok(manifest) if manifest.project.id != entry.id => ManifestOutcome::Invalid {
            sha256,
            problem: format!(
                "the manifest describes project {}, but the site index lists it as {}",
                manifest.project.id, entry.id
            ),
        },
        Ok(manifest) => ManifestOutcome::Fetched {
            bytes,
            sha256,
            manifest: Box::new(manifest),
        },
        Err(problem) if problem.is_policy_refusal() => {
            ManifestOutcome::Refused(problem.to_string())
        }
        Err(problem) => ManifestOutcome::Invalid {
            sha256,
            problem: problem.to_string(),
        },
    }
}

// The apply half ------------------------------------------------------------------------------

fn source_health(kind: FailureKind) -> SourceHealth {
    match kind {
        FailureKind::Unreachable => SourceHealth::Unreachable,
        FailureKind::Refused => SourceHealth::Refused,
        FailureKind::UnsupportedVersion => SourceHealth::UnsupportedVersion,
        FailureKind::BrokenAdvertisement => SourceHealth::BrokenLink,
        FailureKind::NotFound => SourceHealth::NoIndex,
    }
}

fn origin_health(kind: FailureKind) -> OriginHealth {
    match kind {
        FailureKind::Unreachable => OriginHealth::Unreachable,
        FailureKind::Refused => OriginHealth::Refused,
        FailureKind::UnsupportedVersion => OriginHealth::UnsupportedVersion,
        FailureKind::BrokenAdvertisement | FailureKind::NotFound => OriginHealth::NoIndex,
    }
}

fn trail_records(trail: &[discovery::Attempt]) -> Vec<AttemptRecord> {
    trail
        .iter()
        .map(|attempt| AttemptRecord {
            url: attempt.url.to_string(),
            outcome: attempt.outcome.clone(),
        })
        .collect()
}

fn new_claim(project: &ProjectId, origin: &str, entry: &IndexEntry, now: &str) -> ClaimRecord {
    ClaimRecord {
        project: project.clone(),
        origin: origin.to_owned(),
        entry: entry.clone(),
        advertised_sha256: entry.manifest_sha256.clone(),
        ingested_sha256: None,
        rejected: None,
        health: ClaimHealth::Current,
        problem: None,
        media: None,
        first_observed: now.to_owned(),
        last_changed: None,
        last_attempt: now.to_owned(),
        last_success: None,
    }
}

/// One event's worth of facts, before it gets an id.
struct Transition<'a> {
    kind: EventKind,
    name: &'a str,
    before: Option<&'a str>,
    after: Option<&'a str>,
    changes: Vec<diff::Change>,
    moved_from: Option<&'a str>,
}

/// Writes one refresh's observations into the state. Never touches the network.
struct Apply<'a> {
    state: &'a mut State,
    report: &'a mut Report,
    now: &'a str,
    /// Origins whose claims this refresh already applied, in case two sources lead to one site.
    applied: Vec<String>,
}

fn apply(state: &mut State, outcomes: &[(Url, SourceOutcome)], now: &str, report: &mut Report) {
    let enrolled: Vec<String> = outcomes
        .iter()
        .map(|(url, _)| state::source_id(url))
        .collect();
    state.sources.retain(|id, _| enrolled.contains(id));
    for origin in state.origins.values_mut() {
        origin.sources.clear();
    }

    let mut apply = Apply {
        state,
        report,
        now,
        applied: Vec::new(),
    };
    for (url, outcome) in outcomes {
        let id = state::source_id(url);
        let previous_origin = apply
            .state
            .sources
            .get(&id)
            .and_then(|record| record.origin.clone());
        match outcome {
            SourceOutcome::Failed(failure) => {
                let line = if failure.detail.contains(url.as_str()) {
                    failure.detail.clone()
                } else {
                    format!("{url}: {failure}")
                };
                apply.report.unreachable_sources.push(line);
                apply.failure(url, previous_origin.as_deref(), failure);
            }
            SourceOutcome::Resolved(resolved) => {
                apply.resolved(url, previous_origin.as_deref(), resolved);
            }
        }
    }
    apply.forget_unenrolled();
}

impl Apply<'_> {
    fn stamp(&self) -> String {
        self.now.to_owned()
    }

    /// A site no source leads to any more is no longer observed. Its files leave the state; Git
    /// keeps them in history.
    fn forget_unenrolled(&mut self) {
        let live: Vec<String> = self
            .state
            .sources
            .values()
            .filter_map(|record| record.origin.clone())
            .collect();
        let gone: Vec<String> = self
            .state
            .origins
            .keys()
            .filter(|origin| !live.contains(origin))
            .cloned()
            .collect();
        for origin in gone {
            self.state.origins.remove(&origin);
            self.state.indexes.remove(&origin);
            self.state.claims.retain(|key, _| key.origin != origin);
            self.state.manifests.retain(|key, _| key.origin != origin);
            self.report.removed_origins.push(origin);
        }
    }

    fn record(&mut self, key: &ClaimKey, transition: Transition<'_>) {
        let id = state::event_id(
            transition.kind,
            &key.project,
            &key.origin,
            transition.before,
            transition.after,
        );
        if self.state.events.contains_key(&id) {
            return;
        }
        let event = Event {
            id: id.clone(),
            kind: transition.kind,
            project: key.project.clone(),
            origin: key.origin.clone(),
            name: transition.name.to_owned(),
            observed_at: self.stamp(),
            before: transition.before.map(str::to_owned),
            after: transition.after.map(str::to_owned),
            changes: transition.changes,
            moved_from: transition.moved_from.map(str::to_owned),
        };
        self.state.events.insert(id, event.clone());
        self.report.events.push(event);
    }

    fn failure(&mut self, url: &Url, previous_origin: Option<&str>, failure: &discovery::Failure) {
        let id = state::source_id(url);
        let last_success = self
            .state
            .sources
            .get(&id)
            .and_then(|record| record.last_success.clone());
        let record = SourceRecord {
            url: url.to_string(),
            origin: previous_origin.map(str::to_owned),
            health: source_health(failure.kind),
            problem: Some(failure.detail.clone()),
            method: None,
            trail: trail_records(&failure.trail),
            last_attempt: self.stamp(),
            last_success,
        };
        self.state.sources.insert(id, record);
        let Some(origin_id) = previous_origin else {
            return;
        };
        let now = self.stamp();
        if let Some(origin) = self.state.origins.get_mut(origin_id) {
            if !origin.sources.contains(&url.to_string()) {
                origin.sources.push(url.to_string());
            }
            origin.health = origin_health(failure.kind);
            origin.problem = Some(failure.detail.clone());
            origin.last_attempt = now;
        }
        self.origin_unavailable(origin_id, &failure.detail);
    }

    fn origin_unavailable(&mut self, origin: &str, problem: &str) {
        let now = self.stamp();
        for (key, claim) in &mut self.state.claims {
            if key.origin == origin && claim.health != ClaimHealth::Withdrawn {
                claim.health = ClaimHealth::OriginUnavailable;
                claim.problem = Some(problem.to_owned());
                claim.last_attempt.clone_from(&now);
            }
        }
    }

    fn resolved(&mut self, url: &Url, previous_origin: Option<&str>, resolved: &Resolved) {
        let id = state::source_id(url);
        let (health, problem, last_success) = match &resolved.index {
            Ok(_) => (SourceHealth::Resolved, None, Some(self.stamp())),
            Err(problem) => (
                if matches!(problem, Problem::UnsupportedVersion(_)) {
                    SourceHealth::UnsupportedVersion
                } else {
                    SourceHealth::Resolved
                },
                Some(problem.to_string()),
                self.state
                    .sources
                    .get(&id)
                    .and_then(|record| record.last_success.clone()),
            ),
        };
        let record = SourceRecord {
            url: url.to_string(),
            origin: Some(resolved.origin.clone()),
            health,
            problem,
            method: Some(resolved.method.describe().to_owned()),
            trail: trail_records(&resolved.trail),
            last_attempt: self.stamp(),
            last_success,
        };
        self.state.sources.insert(id, record);

        self.update_origin(url, previous_origin, resolved);
        if self.applied.contains(&resolved.origin) {
            return;
        }
        self.applied.push(resolved.origin.clone());

        match &resolved.index {
            Ok(index) => {
                self.state
                    .indexes
                    .insert(resolved.origin.clone(), resolved.index_bytes.clone());
                let moved_from = previous_origin.filter(|previous| *previous != resolved.origin);
                self.claims(resolved, index, moved_from);
            }
            Err(problem) => {
                let problem = format!("the site index cannot be used: {problem}");
                self.origin_unavailable(&resolved.origin, &problem);
            }
        }
    }

    fn update_origin(&mut self, url: &Url, previous_origin: Option<&str>, resolved: &Resolved) {
        let now = self.stamp();
        let moved = previous_origin
            .filter(|previous| *previous != resolved.origin)
            .and_then(|previous| self.state.origins.get(previous))
            .map(|previous| Move {
                origin: previous.id.clone(),
                index_url: previous.index_url.clone(),
                observed_at: now.clone(),
                evidence: if resolved.redirects.is_empty() {
                    format!("the enrolled source {url} now leads to this site")
                } else {
                    format!("the enrolled source {url} now redirects to this site")
                },
            });
        let origin = self
            .state
            .origins
            .entry(resolved.origin.clone())
            .or_insert_with(|| OriginRecord {
                id: resolved.origin.clone(),
                index_url: resolved.response.url.to_string(),
                site_name: None,
                site_url: None,
                generator: None,
                sources: Vec::new(),
                redirects: Vec::new(),
                validators: crate::fetch::Validators::default(),
                index_sha256: None,
                health: OriginHealth::Healthy,
                problem: None,
                issues: Vec::new(),
                previous: Vec::new(),
                first_observed: now.clone(),
                last_attempt: now.clone(),
                last_success: None,
            });
        if !origin.sources.contains(&url.to_string()) {
            origin.sources.push(url.to_string());
            origin.sources.sort();
        }
        origin.previous.extend(moved);
        origin.index_url = resolved.response.url.to_string();
        origin.redirects = resolved.redirects.iter().map(ToString::to_string).collect();
        if !resolved.response.not_modified {
            origin.validators = resolved.response.validators.clone();
        }
        origin.last_attempt.clone_from(&now);
        match &resolved.index {
            Ok(report) => {
                origin.site_name = Some(report.index.site.name.clone());
                origin.site_url = Some(report.index.site.url.clone());
                origin.generator.clone_from(&report.index.generator);
                origin.index_sha256 = Some(state::sha256_hex(&resolved.index_bytes));
                origin.health = OriginHealth::Healthy;
                origin.problem = None;
                origin.issues = report
                    .duplicate_ids
                    .iter()
                    .map(|id| {
                        format!("project {id} is listed more than once, so no entry for it is used")
                    })
                    .collect();
                origin.last_success = Some(now);
            }
            Err(problem) => {
                origin.health = match problem {
                    Problem::UnsupportedVersion(_) => OriginHealth::UnsupportedVersion,
                    Problem::Policy(_) => OriginHealth::Refused,
                    _ => OriginHealth::InvalidIndex,
                };
                origin.problem = Some(problem.to_string());
            }
        }
    }

    fn claims(&mut self, resolved: &Resolved, index: &IndexReport, moved_from: Option<&str>) {
        for entry in index.entries() {
            let key = ClaimKey {
                project: entry.id.clone(),
                origin: resolved.origin.clone(),
            };
            self.claim(&key, entry, &resolved.manifests[&entry.id], moved_from);
            self.media(&key, &resolved.media[&entry.id]);
        }
        for id in &index.duplicate_ids {
            let key = ClaimKey {
                project: id.clone(),
                origin: resolved.origin.clone(),
            };
            let entry = index
                .index
                .projects
                .iter()
                .find(|entry| &entry.id == id)
                .expect("a duplicate id comes from an entry");
            let now = self.stamp();
            let claim = self
                .state
                .claims
                .entry(key)
                .or_insert_with(|| new_claim(id, &resolved.origin, entry, &now));
            claim.health = ClaimHealth::Invalid;
            claim.problem = Some(
                "the site index lists this project more than once, so neither entry is used"
                    .to_owned(),
            );
            claim.last_attempt = now;
        }
        self.withdrawals(resolved, index);
    }

    fn withdrawals(&mut self, resolved: &Resolved, index: &IndexReport) {
        let listed: Vec<&ProjectId> = index.index.projects.iter().map(|entry| &entry.id).collect();
        let withdrawn: Vec<ClaimKey> = self
            .state
            .claims
            .iter()
            .filter(|(key, claim)| {
                key.origin == resolved.origin
                    && !listed.contains(&&key.project)
                    && claim.health != ClaimHealth::Withdrawn
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in withdrawn {
            let now = self.stamp();
            let claim = self
                .state
                .claims
                .get_mut(&key)
                .expect("the key came from the map");
            claim.health = ClaimHealth::Withdrawn;
            claim.problem = None;
            claim.last_attempt = now;
            let (name, ingested) = (claim.entry.name.clone(), claim.ingested_sha256.clone());
            if ingested.is_some() {
                self.record(
                    &key,
                    Transition {
                        kind: EventKind::Withdrawn,
                        name: &name,
                        before: ingested.as_deref(),
                        after: None,
                        changes: Vec::new(),
                        moved_from: None,
                    },
                );
            }
        }
    }

    fn media(&mut self, key: &ClaimKey, outcome: &MediaOutcome) {
        match outcome {
            MediaOutcome::Keep => {}
            MediaOutcome::None => {
                if let Some(claim) = self.state.claims.get_mut(key) {
                    claim.media = None;
                }
            }
            MediaOutcome::Cached { record, bytes } => {
                if let (Some(file), Some(bytes)) = (&record.file, bytes) {
                    self.state.new_media.insert(file.clone(), bytes.clone());
                }
                if let Some(claim) = self.state.claims.get_mut(key) {
                    claim.media = Some(record.clone());
                }
            }
        }
    }

    fn claim(
        &mut self,
        key: &ClaimKey,
        entry: &IndexEntry,
        outcome: &ManifestOutcome,
        moved_from: Option<&str>,
    ) {
        let now = self.stamp();
        let mut claim = self
            .state
            .claims
            .remove(key)
            .unwrap_or_else(|| new_claim(&key.project, &key.origin, entry, &now));
        let was_withdrawn = claim.health == ClaimHealth::Withdrawn;
        claim.entry = entry.clone();
        claim.advertised_sha256.clone_from(&entry.manifest_sha256);
        claim.last_attempt.clone_from(&now);
        claim.problem = None;
        match outcome {
            ManifestOutcome::Unchanged => {
                claim.health = ClaimHealth::Current;
                claim.last_success = Some(now);
                if was_withdrawn {
                    let ingested = claim.ingested_sha256.clone();
                    self.record(
                        key,
                        Transition {
                            kind: EventKind::Restored,
                            name: &entry.name,
                            before: None,
                            after: ingested.as_deref(),
                            changes: Vec::new(),
                            moved_from: None,
                        },
                    );
                }
            }
            ManifestOutcome::Fetched {
                bytes,
                sha256,
                manifest,
            } => {
                self.report.manifests_fetched += 1;
                self.ingest(key, &claim, was_withdrawn, manifest, sha256, moved_from);
                self.state.manifests.insert(key.clone(), bytes.clone());
                claim.advertised_sha256.clone_from(sha256);
                claim.ingested_sha256 = Some(sha256.clone());
                claim.rejected = None;
                claim.health = ClaimHealth::Current;
                claim.last_changed = Some(now.clone());
                claim.last_success = Some(now);
            }
            ManifestOutcome::StillRejected => {
                claim.health = ClaimHealth::Invalid;
                claim.problem = claim
                    .rejected
                    .as_ref()
                    .map(|rejected| rejected.problem.clone());
            }
            ManifestOutcome::Invalid { sha256, problem } => {
                claim.health = ClaimHealth::Invalid;
                claim.problem = Some(problem.clone());
                claim.rejected = Some(Rejection {
                    sha256: sha256.clone(),
                    crawler: CRAWLER_VERSION.to_owned(),
                    problem: problem.clone(),
                });
            }
            ManifestOutcome::Refused(problem) => {
                claim.health = ClaimHealth::Refused;
                claim.problem = Some(problem.clone());
            }
            ManifestOutcome::Inconsistent { advertised, served } => {
                claim.health = ClaimHealth::Inconsistent;
                claim.problem = Some(format!(
                    "the site index advertises sha256 {advertised}, but the manifest URL serves bytes with sha256 {served}; usually a deployment still in progress"
                ));
            }
            ManifestOutcome::Unreachable(problem) => {
                claim.health = ClaimHealth::ManifestUnreachable;
                claim.problem = Some(problem.clone());
            }
        }
        self.state.claims.insert(key.clone(), claim);
    }

    /// Records the event for a newly accepted manifest. A claim that is new here but was held at
    /// the site its source used to lead to continues from that site's manifest, so the event
    /// says what changed rather than listing everything.
    fn ingest(
        &mut self,
        key: &ClaimKey,
        claim: &ClaimRecord,
        was_withdrawn: bool,
        manifest: &Manifest,
        sha256: &str,
        moved_from: Option<&str>,
    ) {
        let held = self
            .state
            .manifests
            .get(key)
            .map(|bytes| (bytes.clone(), claim.ingested_sha256.clone(), None));
        let moved = moved_from.and_then(|origin| {
            let moved_key = ClaimKey {
                project: key.project.clone(),
                origin: origin.to_owned(),
            };
            Some((
                self.state.manifests.get(&moved_key)?.clone(),
                self.state.claims.get(&moved_key)?.ingested_sha256.clone(),
                Some(origin),
            ))
        });
        let (before_bytes, before_sha, continued_from) = match held.or(moved) {
            Some((bytes, sha, from)) => (Some(bytes), sha, from),
            None => (None, None, None),
        };
        let changes = before_bytes
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Manifest>(bytes).ok())
            .map(|before| diff::diff(&before, manifest))
            .unwrap_or_default();
        let kind = if was_withdrawn {
            EventKind::Restored
        } else if before_sha.is_some() && continued_from.is_none() {
            EventKind::Changed
        } else {
            EventKind::Observed
        };
        self.record(
            key,
            Transition {
                kind,
                name: &manifest.project.name,
                before: before_sha.as_deref(),
                after: Some(sha256),
                changes,
                moved_from: continued_from,
            },
        );
    }
}
