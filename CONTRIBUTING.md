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

Use conventional commits through `cog` in the Nix development shell. Keep
`CHANGELOG.md` generated, move completed work out of the roadmap, and put design
rationale or measurements in [history](docs/history/README.md).

For unfinished experiments, use [dogfood](dogfood/README.md). Supported scenes
belong in [examples](examples/README.md).
