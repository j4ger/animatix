# Release notes

Each versioned release uses a committed `vX.Y.Z.md` file here as its canonical
publication text. Write for users: changes, compatibility, limitations and any
migration steps. Review and finalize the content before creating the tag.

`python3 tools/release/release.py draft <version> --output <draft-path>` can
produce an optional `git-cliff` draft. It does not choose a version, commit, tag
or publish. Do not treat generated commit summaries as finished release notes.
`CHANGELOG.md`, if maintained, is an optional overview rather than the release
workflow's source of truth.

See [the release workflow](../contributing.md#prepare-a-versioned-release) for
explicit version selection, validation, tagging and publication. Historical
engineering notes remain in [history](../history/README.md).

Publication creates a release only after verifying its remote tag. Existing
releases are not edited or overwritten; a duplicate publication attempt fails.
