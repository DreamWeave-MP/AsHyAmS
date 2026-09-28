<!--
Enrolling a site? Paste `cargo network inspect <url>` output below; CI runs the same inspection
on every source this pull request adds. Changing code? Say what changed and how you checked it.
-->

## What changes

## How it was checked

- [ ] `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`
- [ ] For a new source: `cargo network inspect <url>` is readable, and the note says what the site is
- [ ] For site changes: the fixture site renders and `check-site` reports no problems
