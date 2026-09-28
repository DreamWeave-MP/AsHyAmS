//! `dreamweave-network`: the DreamWeave Network index, as a batch job.
//!
//! There is no daemon. Each command runs, writes files, and exits. CI runs the same commands a
//! maintainer runs locally; nothing here knows or cares which one it is.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use dreamweave_network::{
    address::AddressPolicy,
    config::{self, Config},
    crawl::Crawler,
    events,
    fetch::Fetcher,
    inspect,
    network::Network,
    site, sitecheck,
    state::{DEFAULT_STATE_DIRECTORY, State},
};
use url::Url;

#[derive(Parser)]
#[command(
    name = "dreamweave-network",
    version,
    about = "Crawl DreamWeave sites, keep what they said, and build the network's static site."
)]
struct Arguments {
    /// The repository root: where `network/`, `pages/` and `zola.toml` live.
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    /// The state directory, normally a checkout of the `network-state` branch.
    #[arg(long, global = true, default_value = DEFAULT_STATE_DIRECTORY)]
    state: PathBuf,

    /// Allow loopback addresses. Only for failure drills against a local fixture server; private,
    /// link-local and metadata addresses stay refused.
    #[arg(long, global = true)]
    allow_loopback: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read a site the way the crawler does and report what it publishes. Writes nothing.
    Inspect {
        /// Any URL on the site: a project page, the front page, dreamweave.json, a manifest.
        url: String,
    },
    /// Inspect a site, then enroll it in network/sources.toml.
    Add {
        url: String,
        /// A line for reviewers: what the site is.
        #[arg(long)]
        note: Option<String>,
    },
    /// Crawl every enrolled source and update the state. Unreachable sites are state, not errors.
    Refresh {
        /// Record this time instead of the clock, as `2026-09-28T18:00:00Z`.
        #[arg(long)]
        now: Option<String>,
        /// Also write the summary here, for a commit message.
        #[arg(long)]
        summary: Option<PathBuf>,
    },
    /// Print what the last refresh observed changing.
    Diff,
    /// Write the Zola content, view data and public JSON from the state.
    Build,
    /// Validate the configuration and the state without touching the network.
    Check,
    /// Check the local links and anchors of the built site.
    CheckSite {
        #[arg(long, default_value = "public")]
        public: PathBuf,
        /// The base URL the site was built for; defaults to zola.toml's.
        #[arg(long)]
        base_url: Option<String>,
    },
}

fn addresses(arguments: &Arguments) -> AddressPolicy {
    if arguments.allow_loopback {
        AddressPolicy::AllowLoopback
    } else {
        AddressPolicy::PublicOnly
    }
}

fn now() -> Result<String> {
    Ok(
        time::OffsetDateTime::now_utc().format(time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
        ))?,
    )
}

fn state_directory(arguments: &Arguments) -> PathBuf {
    if arguments.state.is_absolute() {
        arguments.state.clone()
    } else {
        arguments.root.join(&arguments.state)
    }
}

async fn inspect_command(arguments: &Arguments, url: &str) -> Result<bool> {
    let url = config::source_url(url, addresses(arguments))?;
    let fetcher = Fetcher::new(addresses(arguments))?;
    let inspection = inspect::inspect(&fetcher, &url).await;
    print!("{}", inspection.report());
    Ok(inspection.is_clean())
}

async fn add(arguments: &Arguments, url: &str, note: Option<&str>) -> Result<()> {
    let config = Config::load(&arguments.root, addresses(arguments))?;
    let url = config::source_url(url, addresses(arguments))?;
    if config
        .sources
        .iter()
        .any(|source| Url::parse(&source.url).is_ok_and(|listed| listed == url))
    {
        bail!("{url} is already enrolled");
    }
    let fetcher = Fetcher::new(addresses(arguments))?;
    let inspection = inspect::inspect(&fetcher, &url).await;
    print!("{}", inspection.report());
    if !inspection.is_readable() {
        bail!(
            "not enrolled: this index cannot read the site, so there would be nothing to observe"
        );
    }
    let added = now()?[..10].to_owned();
    let path = arguments.root.join(config::SOURCES_PATH);
    let mut text = fs::read_to_string(&path)?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&config::source_entry(&url, &added, note));
    fs::write(&path, text)?;
    Config::load(&arguments.root, addresses(arguments)).context("the updated source list")?;
    println!(
        "\nEnrolled in {}. Commit it and open a pull request.",
        config::SOURCES_PATH
    );
    Ok(())
}

async fn refresh(arguments: &Arguments, time: Option<&str>, summary: Option<&Path>) -> Result<()> {
    let config = Config::load(&arguments.root, addresses(arguments))?;
    let directory = state_directory(arguments);
    let mut state = State::load(&directory)?;
    let observed_at = match time {
        Some(time) => time.to_owned(),
        None => now()?,
    };
    let crawler = Crawler::new(Fetcher::new(addresses(arguments))?);
    let report = crawler.refresh(&config, &mut state, &observed_at).await;
    state.save(&directory)?;
    let text = report.summary();
    println!("{text}");
    if let Some(summary) = summary {
        fs::write(summary, format!("{text}\n"))?;
    }
    Ok(())
}

fn diff(arguments: &Arguments) -> Result<()> {
    let state = State::load(&state_directory(arguments))?;
    let Some(network) = &state.network else {
        println!("No refresh has run against this state.");
        return Ok(());
    };
    let mut latest: Vec<_> = state
        .events
        .values()
        .filter(|event| event.observed_at == network.observed_at)
        .collect();
    latest.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    if latest.is_empty() {
        println!(
            "The refresh at {} observed no changes.",
            network.observed_at
        );
    }
    for event in latest {
        println!(
            "{} ({}): {}",
            event.name,
            event.origin,
            events::headline(event)
        );
        for section in events::sections(event) {
            println!("  {}", section.title);
            for line in section.lines {
                let values = match (&line.before, &line.after) {
                    (Some(before), Some(after)) => format!(" {before} → {after}"),
                    (None, Some(after)) => format!(" {after}"),
                    (Some(before), None) => format!(" {before}"),
                    (None, None) => String::new(),
                };
                let detail = line
                    .detail
                    .map(|detail| format!(" ({detail})"))
                    .unwrap_or_default();
                println!("    {:?} {}{values}{detail}", line.class, line.subject);
            }
        }
    }
    Ok(())
}

fn build(arguments: &Arguments) -> Result<()> {
    let config = Config::load(&arguments.root, addresses(arguments))?;
    let directory = state_directory(arguments);
    let state = State::load(&directory)?;
    let network = Network::build(&state, &config.curation)?;
    let built = site::build(&arguments.root, &directory, &network)?;
    println!(
        "Wrote {} pages for {} project claims and {} events. Next: zola build",
        built.pages, built.claims, built.events
    );
    Ok(())
}

fn check(arguments: &Arguments) -> Result<bool> {
    let config = Config::load(&arguments.root, addresses(arguments))?;
    println!("{}: {} sources", config::SOURCES_PATH, config.sources.len());
    let directory = state_directory(arguments);
    let state = State::load(&directory)?;
    let problems = state.verify(&directory);
    for problem in &problems {
        println!("PROBLEM: {problem}");
    }
    Network::build(&state, &config.curation)?;
    println!(
        "{}: {} origins, {} claims, {} events, {}",
        directory.display(),
        state.origins.len(),
        state.claims.len(),
        state.events.len(),
        if problems.is_empty() {
            "consistent"
        } else {
            "INCONSISTENT"
        }
    );
    Ok(problems.is_empty())
}

fn check_site(arguments: &Arguments, public: &Path, base_url: Option<&str>) -> Result<bool> {
    let base_url = match base_url {
        Some(base_url) => base_url.to_owned(),
        None => site::index_info(&arguments.root)?.url,
    };
    let public = if public.is_absolute() {
        public.to_path_buf()
    } else {
        arguments.root.join(public)
    };
    let report = sitecheck::check(&public, &base_url)?;
    for problem in &report.problems {
        println!("PROBLEM: {problem}");
    }
    println!(
        "{} pages, {} local links checked, {} problems",
        report.pages,
        report.links,
        report.problems.len()
    );
    Ok(report.problems.is_empty())
}

#[tokio::main]
async fn main() -> ExitCode {
    let arguments = Arguments::parse();
    let result = match &arguments.command {
        Command::Inspect { url } => inspect_command(&arguments, url).await,
        Command::Add { url, note } => add(&arguments, url, note.as_deref()).await.map(|()| true),
        Command::Refresh { now, summary } => {
            refresh(&arguments, now.as_deref(), summary.as_deref())
                .await
                .map(|()| true)
        }
        Command::Diff => diff(&arguments).map(|()| true),
        Command::Build => build(&arguments).map(|()| true),
        Command::Check => check(&arguments),
        Command::CheckSite { public, base_url } => {
            check_site(&arguments, public, base_url.as_deref())
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
