+++
title = "How the network works"
description = "What this index is, what it is not, and every moving part, for readers, publishers and whoever maintains it next."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"

[extra]
kind = "field manual"
docs_root = true
docs_project_name = "AsHyAmS"
docs_sidebar_label = "How it works"
docs_repository_url = "https://github.com/DreamWeave-MP/AsHyAmS"
+++

There is no DreamWeave server in the middle. Every project on this site is published by its own
site, in a format any program can read, and this index is one such program: it reads the sites it
has been told about, remembers what they said, notices when they say something new, and writes all
of that down as static files.

If this index disappears, nothing about any mod changes. Its page still describes it, its manifest
still lists its releases, its archives still download and verify. A client that never heard of
this index installs it exactly as before. Somebody who wants the index back clones this repository
and runs three commands.

That is the design. The rest of this manual explains the machinery.

## The three layers

{{ schematic(data_path="data/schematics/layers.json") }}

**Publishers** own facts: a project's identity, its releases, what each release contains, where to
download it. Only the project's own site is authoritative for any of it.

**This index** owns observations: which sites it read and when, what each said, what changed
between one read and the next, which reads failed and why. It keeps them in Git.

**The static site** is a rendering of those observations, rebuilt from scratch every time, served
by anything that can serve files.

None of the three may quietly become another. The index never rewrites a publisher's claim. The
site never computes anything at request time, because nothing runs at request time.

## What a release can be

A release is one of three things, and the index shows each for what it is.

- **Game data.** An archive laid out for installation, `flat`, `bain` or `fomod`, with its
  components, content files and runtime constraints. Most of the St4sh's 31 projects are this.
- **A program.** One archive per platform: Windows, macOS and Linux, and on their own artifacts,
  Android and handheld builds for PortMaster and muOS. It is unpacked where the user asks and never
  installed into a game. Greenmote, Morrobroom, S3LightFixes and dream-ini publish these.
- **A crate.** A Rust library's release as crates.io serves it, with the checksum Cargo verifies.
  Nothing installs it; the index records it so the release has a history. dream-net, openmw-config,
  l3i and the other DreamWeave libraries publish these.

Every one of them carries its publisher's SHA-256, labelled as the publisher's.

## What this is not

- **Not a registry.** Nothing is allocated here. Project ids are chosen by their authors; this
  index can only report who publishes which.
- **Not the only index.** Anyone can run another one over the same sites, or different ones, with
  different curation. See [Independent indexes](@/about/catalog.md).
- **Not an installer.** It shows dependencies, capabilities and install data. Choosing what to
  install and doing it is a client's job: CHIMERA or anything else that reads the protocol.
- **Not a download host.** Every download link goes to the publisher's own sources or its listed
  mirrors. Nothing is proxied, and nothing is counted.
- **Not a trust authority.** Being listed means this index reads a site. It does not mean anybody
  vouches for it. See [Trust and curation](@/about/trust.md).

## Five sentences to keep in mind

> The network is not a place mods live. It is a map of places mods live.
>
> Publisher sites own facts. The index owns observations.
>
> A failed crawl produces stale data, not a dead ecosystem.
>
> Git is the operations database. Static files are the deployment.
>
> If this index disappears, DreamWeave distribution continues.
