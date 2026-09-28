# DreamWeave Network: implementation plan

The index that reads sites published with the DreamWeave Mod Template (V5 and later, protocol
`schema_version` "2") and renders what it observed as a static site. This file is the plan the
first implementation followed; the site's architecture pages are the maintained description.

## Shape

This is a standalone repository, not an area of another site. Three layers, kept apart:

| Layer | Lives in | Owns |
|---|---|---|
| Publisher | each project's own site | identity, releases, artifacts, sources |
| Index | `network/` (reviewed input), `network-state` branch (observations), `tools/dreamweave-network/` (Rust) | crawling, last-good cache, events, derived graphs |
| Static site | `pages/`, `templates/`, `sass/`, `static/`; generated `content/` and `data/network/` | presentation |

## Rust tool: `dreamweave-network`

One crate in a one-member Cargo workspace, the StroggForge war-room arrangement.

| Command | Does |
|---|---|
| `inspect <url>` | Discovery, schema validation and a claim report. Touches nothing on disk. |
| `add <url>` | `inspect`, then appends the reviewed entry to `network/sources.toml`. |
| `refresh` | Crawls every source, updates `state/`, writes events. Source failures are state, never exit codes. |
| `build` | Pure function of `state/` + `network/` + `pages/`: writes Zola content, view data and the public catalog. |
| `diff` | Prints what the last refresh changed. |
| `check` | Validates `network/*.toml` and the state directory without touching the network. |
| `check-site` | Local links and anchors in the built `public/`. |

Modules: `version` (numeric/decimal precedence and constraints), `address` + `fetch` (HTTP with
SSRF-safe resolution, manual redirects, byte caps), `protocol` (typed documents, vendored JSON
Schemas, semantic checks), `discovery`, `sources`, `state`, `crawl`, `diff`, `events`, `network`
(current release policy, dependency and capability graphs, gaps, conflicts, health), `catalog`
(public aggregate format v1), `markdown` (sanitized CommonMark), `graph` (layered SVG), `site`.

## State

`state/` is a checkout of the `network-state` branch. Per origin: its record and the last good
`dreamweave.json` bytes. Per claim `(project id, origin)`: its record and the last good manifest
bytes exactly as served, so the recorded digest stays checkable. Events are one JSON file each,
named by `sha256(project, origin, old digest, new digest)`, so a rerun cannot duplicate one.
Git history is the observation history. Losing the branch loses events, not the network: a
refresh from empty state recovers every current claim.

## Crawl

Per source: discover the index, validate it, compare each entry's `manifest_sha256` with the
last good digest, fetch only what changed, verify served bytes against the advertised digest
(bounded retries, then "inconsistent deployment"), validate, diff, commit to state. Failures
keep the last good claim and mark it. Same UUID from two origins is a conflict, never a merge.

## Site

Zola, on the Mod Template's presentation layer (imported, recorded in `UPSTREAM.md`) plus a
network skin. Every page renders without JavaScript; JavaScript adds search and filters over
`/network-data/*.json`. No analytics, no external requests from the browser except outbound
links and publisher downloads. Publisher text is escaped by Tera; publisher CommonMark is
rendered and sanitized in Rust before it reaches a template.

## CI

- `check.yml`: fmt, clippy pedantic, tests, actionlint, fixture site build, link check, live
  `inspect` of every enrolled source on pull requests that touch `network/`.
- `network.yml`: every six hours, on pushes to `main` and on demand: load state, refresh,
  build, deploy Pages, push state to `network-state`.

## Out of scope for V1

Artifact downloading and independent digest verification, Sigstore verification, install-state
comparison (no stable CHIMERA lock format exists), automatic host-migration inference beyond
redirect evidence and reviewed curation.
