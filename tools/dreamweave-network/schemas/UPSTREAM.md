# Vendored DreamWeave protocol schemas

| File | Published at |
|---|---|
| `dreamweave-index-2.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/dreamweave-index-2.schema.json> |
| `modManifest-2.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/modManifest-2.schema.json> |

Copied byte for byte from <https://github.com/DreamWeave-MP/DreamWeave-Mod-Template> at
`ef5d5bf9c5c5cb99a799e130bc9773ca546c10a4` (branch `V5`). The manifest schema last changed in
`fd935b146dda065306a26f6445aad7ada5937f58`, which added `binary` artifacts for programs and their
`platform`; the index schema is unchanged since `36474629f96bc8ec73f1b551ed4b1ccb2d91e410`. The
crawler compiles both in with `include_str!`, so validation never depends on the Mod Template's
site being up. The published copies are byte-identical to these.

Updating: copy both files from the Mod Template's current branch, update the revisions above,
and run the tests. An additive change (a new enum value, a new optional field) also needs the
typed model in `src/protocol/` to learn the field, because it refuses unknown ones. A new
`schema_version` is a code change, not a file swap.
