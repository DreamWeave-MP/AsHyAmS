+++
title = "Independent indexes"
description = "The catalog format other readers can consume, how to mirror this site, and how to run your own index instead."
weight = 70

[extra]
kind = "reference"
+++

This is one index of DreamWeave sites. It is not the registry, because there is no registry.
Another community can read the same sites with a different source list, curate differently, mirror
this one, or build something specialized, and none of that needs permission from anybody.

## DreamWeave Network Catalog v1

Everything this index knows is also published as JSON beside the site:

| File | What |
|---|---|
| [`network-data/catalog.json`](../../network-data/catalog.json) | Sites, claims, conflicts, current relationships, capabilities |
| [`network-data/events.json`](../../network-data/events.json) | Every observed change, with typed differences and tags |
| [`network-data/updates.xml`](../../network-data/updates.xml) | The latest hundred events as an Atom feed |
| [`schemas/dreamweave-network-catalog-1.schema.json`](../../schemas/dreamweave-network-catalog-1.schema.json) | JSON Schema for both JSON files |

The catalog is a cache format, not a protocol. Every claim in it carries the site that published
it and the manifest URL that stays the authority. Use it to find things; read the publisher's own
manifest to believe them. A claim's `manifest_sha256` is the digest of the manifest this index
holds, and `advertised_sha256` is what the site last advertised; they differ exactly when the data
is stale, and `cached` says so outright.

The format is versioned by `format_version`. Fields are only ever added within version 1; a
removal or a change of meaning is version 2. The build checks every generated file against the
schema and fails if one does not match: an index that breaks its own documented format has a bug,
not a feature.

The DreamWeave protocol itself does not change to make this index easier to write. If the index
ever needs something the protocol lacks, the gap gets documented first.

## Mirroring this site

The built site is plain files. Every page, every search and every filter works from files inside
it; nothing is fetched from this index's host or from any publisher when somebody reads a page,
except the links they click. To mirror it, copy it:

```sh
zola build --base-url https://mirror.example.org/
python3 -m http.server --directory public 8080
```

## Running your own index

Fork the repository, replace `network/sources.toml` with the sites you care about, empty
`network/curation.toml`, change `base_url` in `zola.toml`, and run the workflow. Your index starts
from nothing on its first refresh and builds its own history from then on.

Or write a different one. The protocol is public and complete enough to implement from its pages:
[discovery](https://dreamweave-mp.github.io/DreamWeave-Mod-Template/guide/protocol/discovery/),
the [manifest](https://dreamweave-mp.github.io/DreamWeave-Mod-Template/guide/protocol/manifest/),
[versions](https://dreamweave-mp.github.io/DreamWeave-Mod-Template/guide/protocol/versions/) and
[artifacts](https://dreamweave-mp.github.io/DreamWeave-Mod-Template/guide/protocol/artifacts/).
This implementation reads nothing a new one could not.

## Installed setups

A useful future reader of the catalog is a comparison of an installed setup against it: which of
the projects you have installed have moved on, been yanked, or gained a migration note. That needs
a stable, documented record of what a client installed. No DreamWeave client publishes one yet, so
this index does not invent one. When CHIMERA or another client documents its lock format, the
comparison can run entirely in the browser against `catalog.json`, with nothing uploaded.
