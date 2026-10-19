# Vendored DreamWeave protocol schemas

| File | Published at |
|---|---|
| `dreamweave-index-2.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/dreamweave-index-2.schema.json> |
| `modManifest-2.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/modManifest-2.schema.json> |
| `dreamweave-index-3.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/dreamweave-index-3.schema.json> |
| `modManifest-3.schema.json` | <https://dreamweave-mp.github.io/DreamWeave-Mod-Template/schemas/modManifest-3.schema.json> |

Copied byte for byte from <https://github.com/DreamWeave-MP/DreamWeave-Mod-Template>. The version 2
schemas are from `b62bfb1ecc33c44380e5cf51a9b78c4a2f6757ff` (branch `V5`); the manifest schema last
changed in `f01287f8`, which added `crate` artifacts for Rust libraries, `android` as a program
platform, and a platform's optional `variant` (`portmaster`, `muos`) for handheld builds. The
version 3 schemas are from `9394f735` (branch `mod-template-on-jess`), which added the `jpk`
artifact format with its `jess` block and an index entry's `jess_package`, and changed nothing
else. The crawler compiles all four in with `include_str!`, so validation never depends on the Mod
Template's site being up. The published copies are byte-identical to these.

Updating: copy the files from the Mod Template's current branch, update the revisions above, and
run the tests. An additive change (a new enum value, a new optional field) also needs the typed
model in `src/protocol/` to learn the field, because it refuses unknown ones. A new
`schema_version` is a code change, not a file swap: `protocol::SCHEMA_VERSIONS` and the parser's
schema selection name every version read.
