+++
title = "Rejected designs"
description = "The obvious ways to build a mod index, and why this one is none of them."
weight = 90

[extra]
kind = "reference"
+++

Every design below would have been easier to build. Most would have been easier to run for about a
year.

**A central database.** One place to put every mod, every release, every update. It makes the
index's infrastructure authoritative and operationally special: somebody has to keep it up, back it
up, migrate it and fix it at 3 AM, and when that somebody leaves, the knowledge goes with them.
Publisher sites already hold the data, and Git already holds the history.

**A persistent web server.** Nothing on this site depends on who is asking. Rendering the same
pages on every request adds a process that must be running for the network to be visible, and a
security surface for no benefit.

**A manually maintained project database.** Somebody copies metadata off mod pages into a form.
It rots the moment the mod updates, because the copy has no reason to follow. The protocol already
has every project describe itself.

**Scraping HTML.** Pages are for people and change whenever the design does. DreamWeave has a
documented machine protocol with schemas. Reading anything else would be choosing to break.

**The GitHub API as the source of truth.** Hosting is not identity. A project can leave GitHub, and
a GitHub repository name says nothing about which mod it is.

**The Nexus Mods API as the source of truth.** Nexus Mods is an integration and a source, listed in
manifests as such. It is not where a DreamWeave project's identity lives.

**URLs as project identity.** Projects move. A mod that went from GitHub Pages to its own domain
would become two mods.

**UUIDs as proof of publisher.** Ids are chosen, not allocated. Treating a matching id as the same
publisher would let anyone take over a project's page on this index by copying one string.

**Recursive crawling.** Following every URL a manifest mentions would let one manifest enroll
arbitrary third parties, and would crawl the internet by accident. The reviewed source list decides
what this index reads.

**Proxying downloads.** Routing archives through this index would make it required for
distribution, which is the one thing it must never be.

**Download counts and popularity.** There is no telemetry, so any number would be built from
proxies, and a ranking built from proxies is a ranking of the proxies. Counting downloads would
also need a server in the path of every download. The network does without both.
