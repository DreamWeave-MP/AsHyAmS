+++
title = "Join the network"
description = "Put a DreamWeave site on this index: publish it, check it, propose it. One pull request, reviewed like any other."
template = "page.html"

[extra]
kicker = ["Join", "one pull request"]
+++

Joining costs you one line in a file and a review. It does not cost you an account, an upload, an
API key or a copy of your mod on anybody else's server, because none of those exist. This index
reads your site on a schedule. Your site stays where it is and stays the authority on everything it
says.

## 1. Publish a DreamWeave site

Use the [DreamWeave Mod Template](https://github.com/DreamWeave-MP/DreamWeave-Mod-Template), V5 or
later. It publishes `dreamweave.json` and a manifest per project on every push, and you never touch
either by hand.

You do not have to use the template. Anything that implements the
[protocol](https://dreamweave-mp.github.io/DreamWeave-Mod-Template/guide/protocol/) correctly is
read exactly the same way: a site index at `dreamweave.json`, a manifest per project, both valid
against the published schemas with `schema_version` `"2"`. This index checks the documents, not
who generated them.

## 2. Check that it is healthy

Sites built from the template have a **Network** page listing, per project, discovery, releases,
channel heads, digests and every other check. Fix anything it flags first. A project with only a
development build is on the network; a project with a recorded stable release is on it properly.

## 3. Inspect it the way the crawler will

Any page of your site will do. You do not need to know where your `dreamweave.json` lives; finding
it is the crawler's job.

```sh
git clone https://github.com/DreamWeave-MP/AsHyAmS
cd AsHyAmS
cargo network inspect https://you.github.io/your-mod/
```

`inspect` writes nothing. It prints what discovery tried, the site index it found, and every
project it lists, each either valid with its channels and current release, or with the exact
problem. The last line says whether every project would be indexed as current.

## 4. Propose it

```sh
cargo network add https://you.github.io/your-mod/ --note "Candlelight and Tallow, lighting mods for OpenMW."
git checkout -b join/your-mod
git commit -am "Enroll your-mod"
```

`add` inspects again, refuses a site it cannot read, and appends one entry to
`network/sources.toml`. Open a pull request. CI inspects every enrolled site on the pull request, so
the reviewer sees the same report you did.

No Rust toolchain? [Open an issue](https://github.com/DreamWeave-MP/AsHyAmS/issues/new?template=join.yml)
with the URL and a maintainer will run the same two commands.

## What a reviewer checks

That the site is readable, that it is what the note says it is, and that it is not obviously
hostile. That is all. Review is not an endorsement, a quality bar or a security audit, and the site
never says it is: every project record names the site that published it.

## What happens next

Nothing, from your side. The index is refreshed every six hours. When your site publishes a new
manifest its `manifest_sha256` changes, the crawler fetches the new one, and the change shows up in
[Updates](@/updates/_index.md) with a typed diff. Unchanged manifests are not even downloaded.

If your site is down for a crawl, your projects stay listed from the last good read and are marked
stale. When it comes back, they are current again. You do not need to tell anybody.

## Moving, leaving

**Moving hosts.** Keep your project ids. If your old address redirects to the new one, the crawler
follows the redirect and records the move itself. If it cannot redirect, send a pull request adding
a `[[migration]]` to `network/curation.toml`, or just change the source URL.

**Leaving.** Send a pull request removing your entry. The next refresh drops your claims from the
site. The history of what was observed stays in Git, because that is what history is.

{% callout(kind="note", title="Why a pull request and not a form") %}
A submission server would be the one piece of this index that has to stay up, be patched and be
trusted with write access. Git already does review, history and multiple maintainers. It stays.
{% end %}
