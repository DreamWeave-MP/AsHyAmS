+++
title = "Trust and curation"
description = "What being listed means, what a digest and a signature prove, which words this site will not use, and why nothing is counted."
weight = 60

[extra]
kind = "reference"
+++

Four different things get confused constantly. This site keeps them apart.

| Thing | Who decides | What it tells you |
|---|---|---|
| Protocol validity | the published schemas and rules | The documents are well-formed DreamWeave |
| Index enrollment | this index's maintainers, by pull request | This index reads the site |
| Cryptographic provenance | math, given a signature | Which workflow produced some bytes |
| Trust | you, and your client | Whether to install it |

Nothing on this site crosses those lines. A project being listed means a site this index reads
publishes a valid claim for it. It does not mean endorsed, safe, maintained, compatible with your
setup, or any good.

## Integrity is not trust

Every artifact on this site shows the SHA-256 its publisher declared, labelled as the publisher's
claim. This index does not download archives during crawls and has not checked those bytes. Your
client checks them, against the manifest, on download: that is what makes mirrors safe to use.

A published Sigstore signature is shown as published, with the identity and issuer it names. A
signature, when verified, says which workflow in which repository produced the bytes. It says
nothing about whether to trust that repository. This index does not verify signatures today, so it
never says "verified". When it does, it will say "signature cryptographically valid" and nothing
grander.

Words this site does not use about projects: trusted, safe, verified author, official, recommended.
Each would claim a basis this index does not have.

## Curation

This index may make decisions of its own: acknowledging a host move, featuring a project on the
front page. Every one lives in `network/curation.toml`, is reviewed like code, and is shown on the
site as this index's decision, not as anything a publisher said. Nothing editorial is hidden in the
crawler.

Moderation is the source list. A site that turns hostile is removed from `network/sources.toml` by
pull request, and its claims leave the site on the next refresh.

## Nothing is counted

There are no analytics, no download counters, no view counts, no stars, no endorsements, no
trending, no top-ten. The network has no telemetry, so any popularity number would be invented
from proxies, and invented numbers end up ranking things. Every download link goes straight to the
publisher's own sources or its declared mirrors, never through this domain.

The cost is that nobody knows how often anything is downloaded. That is the correct cost.
