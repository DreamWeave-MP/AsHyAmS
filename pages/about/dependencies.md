+++
title = "Dependencies and capabilities"
description = "Which release counts as current, how relationships resolve, what a capability is, and where the network ends."
weight = 55

[extra]
kind = "reference"
+++

Relationships belong to releases, not projects. Candlelight 1.0 may need nothing and Candlelight
1.1 may need Tallow. Flattening every release ever published into one graph would produce a
drawing of nothing in particular, so every graph and table on this site reads one release per
claim: its **current** release.

## The current release

1. The head of `stable`, when the project has one.
2. Otherwise the highest-precedence head among its other channels, except `development`.
3. Otherwise the head of `development`.

`stable` and `development` are the two channel names the protocol itself gives meaning to: stable
is the default, development is the rolling build of the default branch. Any other channel means
whatever its publisher says, so none is preferred over another except by version, compared under
the project's own scheme. Every channel head is shown on the claim's page, and every release keeps
its own relationships there.

This is this index's policy for presenting a graph. It is not dependency resolution. A client
picks releases by its own policy and resolves them itself.

## Resolution

| A relationship names | It resolves to |
|---|---|
| a project id | every listed claim for that id; more than one is an identity conflict and is shown as one |
| a capability | every claim whose current release provides it |
| neither | nothing: it is informational, readable by people, resolvable by nobody |

A version constraint is checked against the target's current release under the **target's**
versioning scheme, and the page says whether it is satisfied. Numeric and decimal versions are
never compared with each other, and nothing here is SemVer except where the protocol says it is.

## Capabilities

A release may say it `provides` named capabilities, like `dreamweave:dynamic-lights`. A
relationship may require one instead of a project, and any project whose release provides it
satisfies it. Capability names are published by projects. This index lists who claims each one; it
does not decide who deserves it.

## Network gaps

A relationship whose target nobody in this index publishes is a **gap**, and the gaps page lists
every one with what references it. Gaps are not errors. Most of the modding world does not publish
DreamWeave manifests, and nothing says it has to.

Gaps are grouped only where identity is clear: by project id, by capability, by URL. References
that give nothing but a name are grouped by identical text and labelled as such, because two mods
both saying "Patch for Purists" is a coincidence of spelling, not proof of anything.

URLs named by gaps that lie outside every enrolled site are listed as **candidates**, with the
`inspect` command to try. Nothing follows them. A manifest cannot enroll a site; a maintainer can.

## Used by

"Used by 12 indexed projects" is a count of distinct claims whose current release requires this
one. It is useful when deciding whether a library change will hurt anybody. It is not a ranking,
and there is no page that sorts projects by it and calls the result popular.
