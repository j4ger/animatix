# Contributing

Start with the [development guide](docs/contributing.md) for setup, commands,
validation and release workflows. The [architecture](docs/architecture.md) and
[ECS contract](docs/ecs.md) describe the boundaries changes must preserve.

## A useful change

- Check existing designs before adding a new mechanism. Discuss changes that
  affect language semantics or architecture before implementing them.
- Keep the implementation focused and add tests that demonstrate the behavior.
- Ship user-facing language changes with documentation and a runnable example.
- Review actual frames for layout or rendering changes, not just test output.
- Run the relevant [validation gates](docs/contributing.md#validate-a-change),
  including formatting, before committing. Note anything you could not verify.

Use ordinary `git commit` after staging the intended changes. Conventional
Commits are recommended, not required. Keep canonical notes in `docs/releases/` files named `vX.Y.Z.md` human-reviewed and editable;
optional `git-cliff` output is a draft, not the final release notes.
`CHANGELOG.md` is an optional overview.
Move completed work out of the roadmap and put design rationale or measurements
in [history](docs/history/README.md).

Run unfinished experiments and human review through the harness using temporary
artifacts. Keep actionable gaps in the roadmap and regression fixtures beside
their tests. Supported scenes belong in [examples](examples/README.md).
