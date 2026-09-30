# AsHyAmS state

This branch is written by `dreamweave-network refresh`, normally from the scheduled workflow on
`main`. It is what the index observed, not what anybody published: every manifest here is a
cached copy of a claim its own site makes, and the site stays the authority.

Do not edit it by hand. To change what the index reads, edit `network/sources.toml` or
`network/curation.toml` on `main`. To recover from a broken state, delete the branch: the next
refresh rebuilds every current claim from the enrolled sites. Only the event history and the
first-observed dates are lost.

The layout and every record are described on the AsHyAmS site under About → State.
