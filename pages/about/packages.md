+++
title = "Jess packages"
description = "How a site distributes Jess packages beside its DreamWeave documents, what this index copies from it, and what a Jess client does with the copy."
weight = 45

[extra]
kind = "reference"
+++

[Jess](https://github.com/DreamWeave-MP/jess) packages are JPK archives: Luau applications and
libraries, among them the generator that builds Jess's own documentation. A DreamWeave release
cannot carry one honestly. `schema_version` `"2"` says every release has at least one artifact and
that an artifact is game data, a program built for one platform, or a Rust crate; a JPK is none of
those, and relabelling it as one would be a lie the schema happens to accept. The core of version
`"2"` is frozen, so the fix is not a new artifact kind either.

A site that distributes Jess packages therefore publishes one more static document, and this
index reads it the same way it reads everything else: copy, say where it came from, never serve
or vouch for the bytes.

## The feed

`jess-packages.json`, in the directory `dreamweave.json` lives in. Nothing advertises it and
nothing links it: a site has a feed exactly when that file is there. The format is
[`jess-packages-1.schema.json`](../../schemas/jess-packages-1.schema.json), format version 1.

```json
{
  "format": "jess-packages",
  "format_version": 1,
  "publisher": { "name": "DreamWeave", "url": "https://publisher.example/" },
  "packages": [
    {
      "id": "org.dreamweave.site",
      "summary": "The static site generator Jess's documentation is built with.",
      "versions": [
        {
          "version": "0.1.0",
          "jess_api": "1",
          "status": "available",
          "content_digest": "778b9e0e53f146dac8b5669ee93edcc692140851bed6cf7e018032c784e9bbc8",
          "dependencies": {},
          "artifact": {
            "filename": "org.dreamweave.site-0.1.0.jpk",
            "media_type": "application/vnd.jess.jpk",
            "size": 184320,
            "digests": { "sha256": "…64 hex digits…" },
            "sources": [{ "url": "https://…/org.dreamweave.site-0.1.0.jpk", "kind": "publisher" }],
            "signatures": []
          }
        }
      ]
    }
  ]
}
```

Every version carries two digests, and they are different facts. `content_digest` is Jess's own:
BLAKE3 over the package's canonical uncompressed content, the digest `jess package inspect`
prints and `jess.lock` pins, which is the same however the archive is packed. `digests.sha256` is
the digest of the archive file as downloaded. A client checks both; neither stands in for the
other.

The rules the schema cannot state, which the crawler checks before it accepts a feed: a package id
is listed once; its versions are distinct and newest first; `yanked` is present exactly when
`status` is `yanked`; a package does not depend on itself; URLs are http(s). Yanked versions stay
listed and say why, so a lock that pins one still finds its facts. A `project` on a package names
a DreamWeave project **on the same site**; it is never resolved to another site's claim.

The same resource limits apply as to a manifest, plus the feed's own: at most 8 MiB, at most 500
packages. Over a limit is "refused by crawler policy", not "invalid".

## What the index does with it

The feed is a record on the site's origin, `origins/<site>/jess-packages.json`, kept byte for byte
with the same last-good rule as a manifest: a feed that fails any check leaves the held one in
place and says why; a site that is down leaves it in place and says so; a 404 withdraws it, which
keeps the record and the bytes and takes its packages out of the catalog. Most sites never publish
a feed, and for them the whole of this is one request answered 404 per crawl.

Each package in a held feed is a claim, `(package id, site)`, shown under [Packages](@/packages/_index.md)
with every version the publisher lists and a download link to the publisher's URL. The index does
not download the archive, check either digest, or run anything. The same id from two sites is two
claims and an entry on [Identity conflicts](@/conflicts/_index.md), which says one more thing: whether
every version both sites list names the same digests. Same digests is what mirroring one
publication looks like; different digests is two publications under one name. The index shows the
difference and picks neither.

Dependency requirements in a feed are copied as the publisher wrote them. They are Jess's to
resolve, with Jess's rules, against the archive's own manifest; they never enter this index's
dependency graph, whose edges are DreamWeave relationships between projects.

## What a Jess client does with the index

Everything the index holds about packages is also one file,
[`network-data/jess-packages-v1.json`](../../network-data/jess-packages-v1.json), described by
[`dreamweave-network-jess-packages-1.schema.json`](../../schemas/dreamweave-network-jess-packages-1.schema.json).
It is a discovery file, not a registry. For every package claim it carries the site, the
publisher's feed URL, the digest of the feed the index copied from, the index's freshness marks,
and the versions copied unchanged.

A client uses it to answer one question: which site publishes this id. Then it reads that site's
feed, which is the authority, picks a version under its own rules, downloads the archive from a
source the feed names, verifies the archive's SHA-256, opens it with its own reader, verifies the
content digest, and only then keeps anything. If the index is down the client's lock still names
the publisher's feed and the digests, so a locked restore never needs the index at all. If two
sites claim one id, the client asks which one is meant, or follows the one its lock already pins;
the index's listing order means nothing.

An index listing is not provenance. It says a site served a feed that said these things, on the
dates shown, and nothing more.
