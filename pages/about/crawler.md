+++
title = "The crawler"
description = "Discovery, the digest that saves every unnecessary download, half-finished deployments, and the limits this index puts on strangers' servers."
weight = 20

[extra]
kind = "reference"
+++

A refresh reads every enrolled source once. Most of the time that means one small JSON file per
site, a digest comparison per project, and nothing else, because nothing changed.

## Per source

1. **Discover the site index.** The enrolled URL can be any page on the site. The crawler follows
   the protocol's own algorithm: a DreamWeave JSON document is used as is; an HTML page is read for
   exactly one thing, a `<link rel="alternate">` with a DreamWeave media type; otherwise it tries
   `dreamweave.json` beside the URL and in every parent directory up to the origin's root. That
   last step is what finds a GitHub Pages project site below `you.github.io`. Nothing else in the
   HTML is read. Scraping pages would make this index depend on how a site looks this week.
2. **Validate the index** against the published schema, `dreamweave-index-2.schema.json`.
3. **Compare digests.** Each entry advertises `manifest_sha256`, the SHA-256 of the manifest's
   bytes as served. If it equals the digest of the manifest this index already holds, the manifest
   has not changed and is not downloaded.
4. **Fetch what changed**, check the served bytes against the advertised digest, validate against
   `modManifest-2.schema.json` and the protocol's rules, and check that the manifest describes the
   project the index entry says it does.
5. **Diff and record** against the manifest held before. See [Updates](@/about/updates.md).

A site with forty projects costs one index request plus one request per project that changed.
Artifacts are never downloaded: a 4 GiB texture pack does not get fetched every six hours to prove
its hash still matches. The digest is the publisher's claim, shown as the publisher's claim.

## Half-finished deployments

Static hosts do not publish atomically. A crawl can see the new `dreamweave.json` while a CDN still
serves the old manifest. The crawler notices, because the bytes do not hash to the advertised
digest, and then:

1. waits 5 seconds, rereads the site index, and fetches the manifest again;
2. waits 20 seconds and does it once more;
3. if the bytes still do not match anything the site advertises, keeps the last good manifest and
   marks the claim **inconsistent deployment**.

The next crawl usually finds it settled. A mismatch means the publication changed or is mid-flight.
It is reported as exactly that and never as anything more sinister.

## What never fails a refresh

Unreachable hosts, DNS failures, 404s, malformed JSON, invalid manifests, sites speaking a newer
protocol, digest mismatches, responses over the size limit. Every one of those is a fact about one
source, recorded on that source's site and claims, and the refresh carries on with the next.

The things that do fail a refresh are this index's own bugs: a source list that does not parse, a
state directory that contradicts itself, a generated catalog that breaks its own schema.

## Who is knocking

Every request identifies itself:

```text
AsHyAmS/0.1.0 (+https://github.com/DreamWeave-MP/AsHyAmS)
```

At most eight sources are read at once, and at most two requests are in flight to any one site.
If a site publishes twenty projects it gets its index read once and two manifest downloads at a
time, and only for projects that changed. ETag and Last-Modified are sent back when a site provided
them; a 304 costs nothing. They are a transport courtesy. `manifest_sha256` stays the change signal.

## Resource limits

This is crawler policy, not the DreamWeave protocol. The protocol allows a manifest of any size;
this crawler reads at most 16 MiB of one. A document refused here is reported as "refused by
crawler policy", never as invalid, and another index is free to choose other numbers.

| Limit | Value | Why that number |
|---|---|---|
| HTML page read during discovery | 2 MiB | A page is only read for its `<link>` elements |
| Site index | 4 MiB | About 1 KiB per project, so thousands of projects |
| Manifest | 16 MiB | A Mod Template release is about 6 KiB: over two thousand releases |
| Cached card image | 2 MiB | Thumbnails, not wallpapers |
| Projects per site | 500 | |
| Releases per project | 2,000 | |
| Artifacts per release | 64 | Programs build one per platform |
| Sources per artifact | 32 | |
| Relationships per release | 256 | |
| Components per release | 256 | |
| Media items per project | 128 | |
| Any single string | 64 KiB | Names, notes, URLs |
| Redirects | 5 | Per request, each hop checked |
| Connect / request timeout | 10 s / 30 s | |
| One source, everything included | 240 s | One slow site cannot hold up the rest |

## Hostile input

Every URL the crawler fetches came from somebody else. Unchecked, one malicious entry would turn a
scheduled CI job into a way to make requests from GitHub's network. So:

- Only `http` and `https`, never with a user name or password in the URL.
- Names are resolved by the crawler's own resolver, which refuses a name if **any** of its
  addresses is loopback, private (RFC 1918), carrier-grade NAT, link-local (which is where cloud
  metadata services live), multicast, documentation, benchmarking or otherwise reserved, in IPv4
  or IPv6, including IPv4 addresses wearing IPv6 clothes. The connection goes to the addresses that
  were checked, so there is no second lookup for DNS rebinding to win.
- Literal IP addresses in URLs get the same check before any connection.
- Redirects are followed by the crawler, one hop at a time, and every hop is checked again.
- No proxy is used. A proxy would resolve names itself and make the checks decoration.
- Bodies are read in chunks and abandoned at the limit, whatever `Content-Length` claimed.
- Nothing downloaded is executed, extracted, or passed to a shell. Publisher text is escaped when
  rendered; publisher CommonMark goes through an allowlist sanitizer that keeps prose and drops
  scripts, frames, styles and images.
- Relationship URLs are never followed. A manifest cannot enroll anybody.

Local failure drills use `--allow-loopback`, which admits 127.0.0.0/8 and `::1` and nothing else.
Private and metadata ranges stay refused even then.
