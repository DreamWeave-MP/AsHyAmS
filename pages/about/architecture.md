+++
title = "Architecture"
description = "Where every piece lives, what it owns, and why there is no database and no server."
weight = 10

[extra]
kind = "reference"
+++

The whole index is a batch job and a directory of files. That is not a simplification of some
grander design. It is the design.

## The data flow

{{ schematic(data_path="data/schematics/pipeline.json") }}

Each arrow is a command that runs, writes files, and exits.

| Step | Command | Reads | Writes |
|---|---|---|---|
| Crawl | `cargo network refresh` | `network/sources.toml`, publisher sites, `state/` | `state/` |
| Derive | `cargo network build` | `state/`, `network/curation.toml`, `pages/`, `zola.toml` | `content/`, `data/network/`, `static/network-data/`, `static/network-media/` |
| Render | `zola build` | all of the above, `templates/`, `sass/`, `static/` | `public/` |

`refresh` is the only step that touches the network. `build` is a pure function of its inputs: the
same state builds byte-identical pages, and the only times on the site are the observation times
the crawler recorded. `zola build` is Zola.

## What lives where

| Path | What | Who writes it |
|---|---|---|
| `network/sources.toml` | The sites this index reads | maintainers, by pull request |
| `network/curation.toml` | This index's own decisions | maintainers, by pull request |
| `pages/` | This manual, the join page | maintainers |
| `templates/`, `sass/`, `static/` | Presentation; part of it imported from the Mod Template (`UPSTREAM.md`) | maintainers |
| `tools/dreamweave-network/` | The crawler, diff engine, network model and site generator, in Rust | maintainers |
| `state/` | A checkout of the `network-state` branch | `refresh`, and nobody else |
| `content/`, `data/network/`, `static/network-data/`, `static/network-media/` | Generated for Zola; gitignored | `build` |
| `public/` | The built site; gitignored | `zola build` |

## The Rust tool

One crate, `dreamweave-network`, with one module per concern. Nothing in it is generic over
anything it does not have two of.

| Module | Job |
|---|---|
| `version` | The protocol's numeric and decimal precedence, constraints |
| `address`, `fetch` | Which addresses may be contacted, and HTTP GET that respects it |
| `protocol` | The documents as typed Rust; parsing through the vendored JSON Schemas and the rules schemas cannot express |
| `discovery` | From any URL to a site's `dreamweave.json` |
| `crawl` | A refresh: fetch what changed, keep the last good, record events |
| `state` | The state directory, its records and its invariants |
| `diff`, `events` | Typed differences between two manifests, and how to read them |
| `network` | Current releases, conflicts, dependency edges, capabilities, gaps |
| `catalog` | The public aggregate format |
| `graph`, `markdown` | Dependency maps as SVG, publisher notes through a sanitizer |
| `site` | Everything Zola renders |

## Why there is no database

The data is small, append-mostly, and has exactly one writer on a schedule. Its history matters,
and it has to be inspectable by anybody who clones the repository. Git does all of that already,
and it comes with review, blame, bisect and a hosting story. A database would add a service that
has to be running, backed up, migrated and administered by somebody who knows how, which is the
failure this whole design exists to avoid.

## Why there is no server

Nothing on this site depends on who is asking or when. Every page is the same for every reader
until the next crawl, so every page is rendered once, ahead of time. Search runs in the reader's
browser against a file the build wrote. The result can be served by GitHub Pages, nginx,
`python3 -m http.server`, or a USB stick, and every one of those is a complete copy of the index.
