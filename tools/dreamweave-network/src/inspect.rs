//! `inspect`: what a site publishes, read the way the crawler reads it, without touching the
//! state. The tool a mod author runs before asking to join, and the check CI runs on a pull
//! request that enrolls a site.

use std::fmt::Write as _;

use url::Url;

use crate::{
    discovery::{self, Discovery, Failure},
    fetch::Fetcher,
    network::{Current, current_release},
    policy,
    protocol::{
        IndexEntry, Manifest,
        parse::{self, IndexReport, Problem},
    },
    state::sha256_hex,
};

pub struct ClaimInspection {
    pub entry: IndexEntry,
    pub manifest: Result<Manifest, String>,
    pub current: Option<Current>,
}

pub struct Inspection {
    pub source: Url,
    pub discovery: Result<Discovery, Failure>,
    pub index: Option<Result<IndexReport, Problem>>,
    pub claims: Vec<ClaimInspection>,
}

impl Inspection {
    /// Discovery worked and the site index is valid: the site can be enrolled.
    pub fn is_readable(&self) -> bool {
        matches!(self.index, Some(Ok(_)))
    }

    /// Readable, and every manifest it lists is valid and current.
    pub fn is_clean(&self) -> bool {
        self.is_readable()
            && self.claims.iter().all(|claim| claim.manifest.is_ok())
            && matches!(&self.index, Some(Ok(report)) if report.duplicate_ids.is_empty())
    }

    pub fn report(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "source     {}", self.source);
        let found = match &self.discovery {
            Ok(found) => found,
            Err(failure) => {
                for attempt in &failure.trail {
                    let _ = writeln!(text, "  tried    {}  {}", attempt.url, attempt.outcome);
                }
                let _ = writeln!(text, "\nNOT READABLE: {failure}");
                return text;
            }
        };
        for attempt in &found.trail {
            let _ = writeln!(text, "  tried    {}  {}", attempt.url, attempt.outcome);
        }
        let _ = writeln!(
            text,
            "index      {} ({})",
            found.index_url(),
            found.method.describe()
        );
        for redirect in &found.redirects {
            let _ = writeln!(text, "redirect   {redirect}");
        }
        let report = match &self.index {
            Some(Ok(report)) => report,
            Some(Err(problem)) => {
                let _ = writeln!(text, "\nNOT READABLE: the site index {problem}");
                return text;
            }
            None => return text,
        };
        let _ = writeln!(
            text,
            "site       {} <{}>",
            report.index.site.name, report.index.site.url
        );
        if let Some(generator) = &report.index.generator {
            let _ = writeln!(text, "generator  {generator}");
        }
        let _ = writeln!(text, "projects   {}", report.index.projects.len());
        for id in &report.duplicate_ids {
            let _ = writeln!(
                text,
                "\nPROBLEM: project {id} is listed more than once; neither entry would be used"
            );
        }
        for claim in &self.claims {
            let _ = writeln!(text);
            let _ = writeln!(text, "  {} ({})", claim.entry.name, claim.entry.id);
            let _ = writeln!(text, "    manifest  {}", claim.entry.manifest);
            match &claim.manifest {
                Ok(manifest) => {
                    let project = &manifest.project;
                    let _ = writeln!(
                        text,
                        "    valid     {} · {} · {} versioning · {} releases",
                        project.project_type.token(),
                        project.status.token(),
                        project.versioning.name(),
                        manifest.releases.len()
                    );
                    for (channel, head) in &manifest.channels {
                        let _ = writeln!(text, "    channel   {channel} {}", head.version);
                    }
                    if let Some(current) = &claim.current {
                        let _ = writeln!(
                            text,
                            "    current   {} {} (this index's current-release policy)",
                            current.channel, current.version
                        );
                    }
                }
                Err(problem) => {
                    let _ = writeln!(text, "    PROBLEM   {problem}");
                }
            }
        }
        let _ = writeln!(
            text,
            "\n{}",
            if self.is_clean() {
                "Ready: every project this site lists would be indexed as current."
            } else {
                "Readable, with the problems above: those projects would be indexed as invalid until they are fixed."
            }
        );
        text
    }
}

async fn inspect_claim(fetcher: &Fetcher, entry: &IndexEntry) -> ClaimInspection {
    let manifest = async {
        let url = Url::parse(&entry.manifest).map_err(|error| format!("manifest URL: {error}"))?;
        let response = fetcher
            .get(&url, policy::MAXIMUM_MANIFEST_BYTES, None)
            .await
            .map_err(|error| format!("{url}: {error}"))?;
        let served = sha256_hex(&response.body);
        if served != entry.manifest_sha256 {
            return Err(format!(
                "the site index advertises sha256 {}, but the manifest serves {served}; a deployment in progress, or a stale index",
                entry.manifest_sha256
            ));
        }
        let manifest = parse::parse_manifest(&response.body).map_err(|problem| problem.to_string())?;
        if manifest.project.id != entry.id {
            return Err(format!(
                "the manifest describes project {}, but the index lists it as {}",
                manifest.project.id, entry.id
            ));
        }
        Ok(manifest)
    }
    .await;
    let current = manifest.as_ref().ok().and_then(current_release);
    ClaimInspection {
        entry: entry.clone(),
        manifest,
        current,
    }
}

pub async fn inspect(fetcher: &Fetcher, source: &Url) -> Inspection {
    let discovery = discovery::discover(fetcher, source, None).await;
    let index = discovery.as_ref().ok().map(|found| {
        if found.index.body.len() as u64 > policy::MAXIMUM_INDEX_BYTES {
            return Err(Problem::Policy(format!(
                "the site index is larger than {} bytes",
                policy::MAXIMUM_INDEX_BYTES
            )));
        }
        parse::parse_index(&found.index.body)
    });
    let mut claims = Vec::new();
    if let Some(Ok(report)) = &index {
        for entry in report.entries() {
            claims.push(inspect_claim(fetcher, entry).await);
        }
    }
    Inspection {
        source: source.clone(),
        discovery,
        index,
        claims,
    }
}
