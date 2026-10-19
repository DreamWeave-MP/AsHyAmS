+++
title = "State"
description = "What the index remembers, where, and why losing it costs history and not the network."
weight = 30

[extra]
kind = "reference"
+++

The index's memory is a directory, `state/`, which is a checkout of the `network-state` branch.
The scheduled workflow commits to that branch after every refresh. `main` holds code, source list
and pages; `network-state` holds observations. Neither pollutes the other's history.

## Layout

| Path | Holds |
|---|---|
| `network.json` | When the last refresh started, and which crawler ran it |
| `sources/<source>.json` | One per enrolled URL: which site it led to, the discovery trail, last attempt and success |
| `origins/<site>/origin.json` | One per site index: health, problem, publication issues, validators, moves |
| `origins/<site>/dreamweave.json` | That site index's last good bytes, exactly as served |
| `claims/<project>/<site>/claim.json` | One per claim: health, advertised and held digests, first observed, last changed, last success |
| `claims/<project>/<site>/manifest.json` | The claim's last good manifest, exactly as served |
| `events/<event>.json` | One per observed transition, never rewritten |
| `media/<sha256>.<ext>` | Cached card images, named by digest |

Manifests are kept byte for byte. `sha256sum claims/…/manifest.json` prints the digest the claim
record holds, and the site index advertised, when it was accepted. Nothing is normalized on the
way in, so nothing is lost on the way in.

Names are readable on purpose: a site's directory is its host and path plus eight hex digits of
its index URL's digest, `dreamweave-mp-github-io-dreamweave-mod-template-439f8bc8`. A person
reading `git log` can tell what changed without a lookup table.

## Last good

A claim's manifest is replaced only by a manifest that passed every check. Everything else leaves
it exactly where it was and says why on the claim:

| Health | Meaning | What you see |
|---|---|---|
| current | The held manifest is the one the site advertises now | The data |
| origin unavailable | The site could not be read on the last crawl | The last good data, marked stale |
| manifest unreachable | The index was read; the manifest was not | The last good data, marked stale |
| invalid publication | The site publishes a manifest that fails the protocol | The last good data, and the exact problem |
| inconsistent deployment | The bytes never matched the advertised digest | The last good data; usually gone next crawl |
| refused by crawler policy | Over a resource limit | The last good data, and which limit |
| withdrawn | The site's index no longer lists the project | Out of the catalog; the record and its history stay |

A failed read never deletes anything. An outage is not evidence a project is dead, and a broken
deploy is not evidence it was never valid. The page says "cached from the last successful
observation" with both times, and it never passes old data off as current.

A manifest that was refused is remembered by digest, so the same broken bytes are not downloaded
every six hours. A new crawler version reads it again, since the rules may have changed.

## History

Git history of the `network-state` branch is the history of what this index observed. The previous
version of any manifest is one `git log -p` away, so the state keeps only the current one. Events
are the index's own summary of each transition, kept as files so the site can list them without
Git.

## Recovery

**The branch is corrupt or gone.** Delete it. The next refresh starts from nothing and recovers
every claim its publishers still publish. Lost: event history and "first observed" dates. Not
lost: the network. The sites were always the authority.

**One claim looks wrong.** `cargo network check` verifies every invariant: each held manifest
hashes to its recorded digest and describes its claim's project, every reference resolves, every
event's id matches its facts, every cached image exists. It exits non-zero and says what is
broken.

Never edit the branch by hand. If a record is wrong, the crawler is wrong; fix the crawler.
