+++
title = "Identity and conflicts"
description = "Claims, not ownership: why a project id proves nothing about who publishes it, and what this index does when two sites publish the same one."
weight = 40

[extra]
kind = "reference"
+++

A DreamWeave project id is a UUID its author picked. Nobody allocated it. That is what lets a mod
survive moving from GitHub Pages to its own domain without becoming a different mod, and it is also
why an id proves nothing about who is allowed to use it. Two unrelated sites can publish the same
id tomorrow, by accident or on purpose, and neither the protocol nor this index can stop them.

So this index never models "the project with id X". It models **claims**: the pair of a project id
and the site that publishes a manifest for it.

## One id, one site

The normal case. The claim's page is `/projects/<id>/<site>/`, and `/projects/<id>/` lists it as
the only claim. Relationships naming the id resolve to it.

## One id, two sites

An **identity conflict**, shown on the [conflicts page](@/conflicts/_index.md), on both claims, on
the id's page, and in the status line of every page. Neither claim is hidden. Neither overwrites
the other. Their releases are not merged. The newer one does not win, the more popular one does
not win, the one with the better name does not win.

A relationship that names the id resolves to both, and says so. A client meeting this should ask
its user which site to trust; the protocol says a client should treat a manifest for a known id
from anywhere new as a separate claim that needs confirmation.

## One name, two ids

Two projects. Names are display text and change whenever an author likes. They are never used to
match anything.

## Moving hosts

A project that moves keeps its id. This index recognizes a move two ways, and only two:

1. **A redirect it saw itself.** The enrolled source URL now redirects to, or leads to, a different
   site. The new site's record notes the old one and the evidence, the old site leaves the state,
   and the claim's first event at its new home shows what changed rather than everything.
2. **A reviewed curation entry.** A `[[migration]]` in `network/curation.toml` names the project,
   the old and new site index URLs, the reason and the date. The old claim is marked as moved,
   stays visible, and stops being counted as a conflict. The site labels this as this index's
   decision.

This index does not infer moves from matching names, matching Nexus Mods ids, matching repository
names or matching file names. All of those are coincidences waiting to be exploited.

When the protocol gains publisher keys, continuity of a signing key becomes a third kind of
evidence. Until then, it is not something this index pretends to know.
