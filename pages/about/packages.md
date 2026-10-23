+++
title = "Jess packages"
description = "How a site publishes Jess packages as DreamWeave projects, what this index copies from it, and what a Jess client does with the copy."
weight = 45

[extra]
kind = "reference"
+++

[Jess](https://github.com/DreamWeave-MP/jess) packages are JPK archives: Luau applications and
libraries, among them the generator and the template every DreamWeave site is built with. Under
protocol `schema_version` `"3"` and later a Jess package is a DreamWeave project like any mod or program:
one repository, one site, one `mod.toml`, releases tagged and recorded by the Mod Template. What
makes it a package is its releases: each carries one artifact of `format` `jpk`, and that
artifact's `jess` block says which package it is.

This index reads it the way it reads everything else: copy the manifest, say where it came from,
never serve or vouch for the bytes.

## The release

A `jpk` artifact is the archive `jess pack` built from the release's commit, with `media_type`
`application/vnd.jess.jpk`, the archive's size and SHA-256, its sources and signatures, and:

```json
"jess": {
  "package": "org.dreamweave.site",
  "jess_api": "1",
  "content_digest": "778b9e0e53f146dac8b5669ee93edcc692140851bed6cf7e018032c784e9bbc8",
  "dependencies": { "org.dreamweave.vfs": "0.1" }
}
```

The two digests are different facts. `content_digest` is Jess's own: BLAKE3 over the package's
canonical uncompressed content, the digest `jess package inspect` prints and `jess.lock` pins,
the same however the archive is packed. `digests.sha256` is the digest of the archive file as
downloaded. A client checks both; neither stands in for the other.

The rules the crawler holds a manifest to, beyond its schema: a release has at most one `jpk`
artifact; only a `jpk` artifact has a `jess` block, and every `jpk` artifact has one; every
release of one project names the same package, since one project is one package; the package id
is lowercase words joined by dots; a package does not depend on itself. A site's index entry for
the project may repeat the package id as `jess_package`, so a client asked for an id reads one
manifest and not all of them.

Everything else about the release is what it is for any project: its date, channel, notes,
`yanked` and `deprecated` state, sources, mirrors and Sigstore bundles. A yanked version stays
listed and says why, so a lock that pins one still finds its facts.

## The application

Under `schema_version` `"4"` the same package may also be released as a Jess application: one
artifact of `format` `jsw`, a packed Jess workspace holding the application's targets and every
package they run, each pinned to its content digest. Its `media_type` is
`application/vnd.jess.jsw`, and its `jess_workspace` block says what is inside:

```json
"jess_workspace": {
  "package": "org.example.greeter",
  "builtin": "0.1.0",
  "default": "org.example.greeter:greet",
  "targets": ["org.example.greeter:greet"],
  "commands": { "greet": "org.example.greeter:greet" },
  "packages": {
    "org.example.greeter": { "version": "0.2.0", "content_digest": "…" },
    "org.example.words": { "version": "1.2.0", "content_digest": "…" }
  }
}
```

`package` names the same package the project's `jpk` releases are, so the application belongs to
the same claim: a project that publishes only the application is still that package's claim, with
no JPK versions. A release has at most one `jsw` artifact, and only a `jsw` artifact has the block.
The block has to add up: its default and its commands run targets it exports, and it holds the
package of every target it exports and the package it is named for. Its executables, one per
platform, are ordinary `binary` artifacts, shown with the release like any program's.

`packages` is a frozen set the publisher pinned, not requirements. It is shown on the package's
page, and it never enters the dependency graph. `jess install <id>` reads the manifest, takes the
newest application release its Jess can run (by `builtin`), and installs it.

## What the index does with it

Nothing new. The manifest is a claim's last good manifest, kept with the same rule as every other:
a manifest that fails a check leaves the held one in place and says why; a site that is down
leaves it in place and says so; a project the site stops listing is withdrawn, which keeps the
record and takes it out of the catalog.

From each held manifest that publishes a package, the build derives a second claim, `(package
id, site)`, shown under [Packages](@/packages/_index.md) with every version the manifest lists and
a download link to the publisher's URL. The index does not download the archive, check either
digest, or run anything. The same id from two sites is two claims and an entry on
[Identity conflicts](@/conflicts/_index.md), which says one more thing: whether every version both
sites list names the same digests. Same digests is what mirroring one publication looks like;
different digests is two publications under one name. The index shows the difference and picks
neither.

Dependency requirements are copied as the publisher wrote them. They are Jess's to resolve, with
Jess's rules, against the archive's own manifest; they never enter this index's dependency graph,
whose edges are DreamWeave relationships between projects.

## What a Jess client does with the index

Everything the index holds about packages is also one file,
[`network-data/jess-packages-v1.json`](../../network-data/jess-packages-v1.json), described by
[`dreamweave-network-jess-packages-1.schema.json`](../../schemas/dreamweave-network-jess-packages-1.schema.json).
It is a discovery file, not a registry. For every package claim it carries the site, the project,
the manifest URL, the digest of the manifest the index copied from, the index's freshness marks,
and the versions copied unchanged, with its application releases as `applications` when it has any.

A client uses it to answer one question: which site publishes this id. Then it reads that site's
manifest, which is the authority, picks a version under its own rules, downloads the archive from a
source the manifest names, verifies the archive's SHA-256, opens it with its own reader, verifies
the content digest, and only then keeps anything. If the index is down the client's lock still
names the publisher's manifest and the digests, so a locked restore never needs the index at all.
If two sites claim one id, the client asks which one is meant, or follows the one its lock already
pins; the index's listing order means nothing.

An index listing is not provenance. It says a site served a manifest that said these things, on
the dates shown, and nothing more.
