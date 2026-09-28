# Vendored DreamWeave protocol schemas

| File | Published at |
|---|---|
| `dreamweave-index-2.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/dreamweave-index-2.schema.json> |
| `modManifest-2.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/modManifest-2.schema.json> |

Copied byte for byte from <https://github.com/DreamWeave-MP/DreamWeave-Mod-Template> at
`75829833143e26a65bb50da284f2e3e554c2f6ce` (branch `V5`; the schemas last changed in
`36474629f96bc8ec73f1b551ed4b1ccb2d91e410`). The crawler compiles them in with `include_str!`,
so validation never depends on the Mod Template's site being up.

`schema_version` "2" is frozen: its core fields do not change. A new copy is only needed when
the protocol publishes version "3", and supporting that is a code change, not a file swap.
