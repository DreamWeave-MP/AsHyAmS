# Imported DreamWeave site components

The network's presentation is built on the DreamWeave Mod Template's, so a project page and its
network record look like the same ecosystem. The files below are copied in, not fetched at build
time, so a clean checkout always has everything.

## From DreamWeave-Mod-Template

Imported at `ef5d5bf9c5c5cb99a799e130bc9773ca546c10a4` (branch `V5`) from
<https://github.com/DreamWeave-MP/DreamWeave-Mod-Template>, licensed AGPL-3.0 like this
repository. Attribution stays with the Mod Template's contributors.

| Files | What |
|---|---|
| `sass/_tokens.sass`, `_base.sass`, `_layout.sass`, `_components.sass`, `_project.sass`, `_catalog.sass`, `_schematic.sass` | Design tokens, elements, page chrome, components, project and catalog styles |
| `sass/docs.sass`, `templates/docs/{base,breadcrumbs,page,section,sidebar,toc}.html`, `static/docs/docs.js` | The shared documentation shell (also imported by StroggForge) |
| `templates/shortcodes/schematic.html`, `templates/shortcodes/callout.html`, `templates/anchor-link.html` | Shortcodes and heading anchors |

Local changes:

- `templates/docs/base.html`: the giscus comments include is removed. The network has no
  comments, and a template naming a file that does not exist is a build error waiting to happen.

Everything network-specific lives in `sass/network.sass`, which loads last, and in the templates
that are not listed above. Do not edit the imported files to restyle them; override in
`network.sass`.

## Protocol schemas

`tools/dreamweave-network/schemas/` holds the protocol's JSON Schemas. Their provenance is in
`tools/dreamweave-network/schemas/UPSTREAM.md`.

## Updating

Diff the listed files against a newer Mod Template revision you have actually read, apply what
matters, reapply the local changes above, update the revision here, and build the site.
