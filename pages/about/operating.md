+++
title = "Operating the index"
description = "The runbook: running it locally, the scheduled workflow, adding and removing sites, reading a failed crawl, recovering, and moving to another host."
weight = 80

[extra]
kind = "runbook"
+++

Everything CI does, a maintainer can do from a clone. Nothing depends on a particular machine, a
cron table, a database password or somebody's memory.

## What you need

Rust (stable), [Zola](https://www.getzola.org/) 0.22.1, and Git. `cargo network` is an alias for
`cargo run --locked --release -p dreamweave-network --`.

## Rebuild AsHyAmS locally

```sh
git clone https://github.com/DreamWeave-MP/AsHyAmS && cd AsHyAmS
git fetch origin network-state && git worktree add state origin/network-state   # the history, if you want it
cargo network refresh      # crawl every enrolled site into state/
cargo network build        # write Zola's content from state/
zola serve                 # look at it
```

Skip the worktree and `refresh` starts from nothing: every current claim comes back, the event
history starts today. `cargo network diff` prints what the last refresh saw change.

## The scheduled workflow

`.github/workflows/network.yml` runs every six hours, on every push to `main`, and on demand from
the Actions tab. Each run is a finite batch job:

1. check out `main`, then `network-state` into `state/` (or start it, the first time);
2. `cargo network refresh`, writing the summary for the commit message;
3. `cargo network check`, `cargo network build`, `zola build`, `cargo network check-site`;
4. publish `public/` to GitHub Pages;
5. commit `state/` to `network-state` and push.

One unreachable site cannot stop a deployment: it is state, and the run carries on. Runs never
overlap; a run that starts while another is going waits for it.

`.github/workflows/check.yml` runs on every pull request: formatting, clippy, tests, `actionlint`,
a site build from the test fixtures with its links checked, and, when the pull request touches
`network/`, a live `inspect` of every enrolled source.

## Adding a site

```sh
cargo network inspect https://example.org/their-mod/
cargo network add https://example.org/their-mod/ --note "What it is, in one line."
```

Review the pull request: is the site readable, is it what the note says, is it not obviously
hostile? Merge it; the push runs a refresh, and the site's projects appear.

## Removing a site

Delete its `[[source]]` entry and merge. The next refresh removes its site record and claims from
the state and the site. Its history stays in `network-state`'s Git log.

## Reading a failed crawl

Start at [Network health](@/health/_index.md). Every enrolled source shows what happened on the
last attempt, and **What discovery tried** lists every request discovery made and what came back.
Every site shows its last problem. Every claim that is not current shows why.

To reproduce one locally without touching the state, run `cargo network inspect <url>`: it reads
the site exactly the way the crawler does and prints the same trail.

## Failure drills

The crawl's failure handling is tested against a local HTTP server in
`tools/dreamweave-network/tests/crawl.rs`: outages and recovery, invalid manifests, half-finished
deployments, withdrawals, moves, identity collisions, removed sources, duplicated events. To
rehearse one by hand, serve fixture sites locally and point a scratch state at them:

```sh
cargo network --allow-loopback --state /tmp/drill-state refresh
```

`--allow-loopback` admits 127.0.0.1 and `::1` and nothing else; private and metadata addresses
stay refused.

## Recovering

| Problem | Fix |
|---|---|
| `cargo network check` reports an inconsistent state | Find the crawler bug. Until then, deleting the branch and refreshing recovers every current claim |
| The `network-state` branch is gone | Run the workflow. It starts the branch again from a fresh crawl |
| GitHub Pages is gone | Build anywhere and copy `public/` to any static host |
| GitHub is gone | Clone from any copy, crawl from anywhere with Rust and Zola |

## Moving to another host

```sh
cargo network refresh && cargo network build
zola build --base-url https://network.example.org/
rsync -a public/ host:/srv/network/
```

The scheduled refresh is one workflow file. Any CI that can run it every few hours, and any
place that can hold a Git branch, replaces GitHub completely.

## Updating the protocol schemas

When the Mod Template changes its schemas, copy them into `tools/dreamweave-network/schemas/`,
record the revision in its `UPSTREAM.md`, teach the typed model in `src/protocol/` any new field
(it refuses unknown ones, as the protocol requires), and run the tests. A new `schema_version` is a
code change, not a file swap: until one is supported, sites publishing it are reported as speaking
a newer protocol and their last good claims stay.
