+++
title = "Updates"
description = "How a new manifest becomes a typed list of changes, why an event can never be recorded twice, and what the tags do and do not mean."
weight = 50

[extra]
kind = "reference"
+++

When a site publishes a new manifest, this index holds two versions of one claim: the one it had
and the one it just accepted. The difference between them is the update.

Comparing them as text would be useless. Key order, whitespace and array position would all look
like changes, and the changes that matter would be three lines somewhere in a wall of them. The
protocol is structured, so the comparison is structural.

## What is compared

**Project metadata.** Name, summary, type, status, versioning, game, license, tags, maintainers,
credits, links, the Nexus Mods integration, media.

**Channel heads.** When a channel's head moves from one release to another, the two releases are
compared: runtimes, the OpenMW Lua API, platforms, required content files, critical extensions,
capabilities provided, every relationship (added, removed, constraint changed, kind changed),
components and their selection rules, artifacts. This is the part that reads like a package
registry diff: `stable 1.1.0 → 1.2.0`, `OpenMW runtime >=0.49 → >=0.50`, `+ requires Tallow >=1.0`.

**Releases.** Added, gone, yanked, deprecated or restored, moved between channels, re-dated, notes
edited.

**Amendments.** A release that exists in both manifests but whose contents changed. The protocol
says a release's contents do not change after publication except by a deliberate amendment, so
these are reported per release, including sources and signatures added or removed.

## Nothing is interpreted

A dependency constraint tightening is reported as a dependency constraint tightening. It is never
called breaking. The only things called breaking are the entries in a publisher's own
`notes.breaking`, shown verbatim with its `notes.migration` and `notes.known_issues`. This index
does not guess what a change means for you, and no language model is anywhere near it.

## Unusual

Three facts are surfaced first, under **Unusual**, because they deserve a second look:

- **Published artifact digest changed without a version change.** New bytes under a release
  version that already existed. Outside the rolling `development` channel, which is rebuilt on
  every push by design, that is rare.
- **Previously indexed release is no longer present in the current manifest.** The protocol keeps
  yanked and deprecated releases listed, so a release vanishing outright is unusual.
- **Versioning scheme changed.** Versions before and after may not order the way they used to.

Unusual is not malicious. These are facts about a publication, stated as facts.

## Event identity

An event's id is the first 20 hex digits of the SHA-256 of its kind, the project id, the site, the
manifest digest before and the digest after. The same transition always has the same id, so
re-running a crawl over the same observations cannot record it twice, and restoring an old copy of
the claims cannot duplicate history. Events are files and are never rewritten.

## Tags

Tags say what kind of change an event contains and are computed when the site is built, so a
better classification applies to all history on the next build.

| Tag | Means |
|---|---|
| release | A release outside the `development` channel appeared |
| breaking | That release's publisher lists breaking changes |
| migration | That release's publisher wrote migration instructions |
| dependencies | A current relationship changed |
| runtime | Runtimes, Lua API, platforms, required content or critical extensions changed |
| status | A release was yanked, deprecated or restored |
| artifacts | Artifacts, sources, signatures or source revisions changed |
| anomaly | One of the unusual facts above |
| capabilities, components, metadata | What they say |
| development | The rolling development build moved: expected on every push |
| observed | The first manifest this index held for a claim |
| listing | A site stopped listing a project, or listed it again |
| moved | A claim arrived from a site its source used to lead to |

## Two different dates

A release has the date its publisher gives it, and the time this index first saw it. They are
different facts and every page keeps them apart. An index that noticed a release on Tuesday has
no idea whether it was published on Tuesday, and does not pretend to.
