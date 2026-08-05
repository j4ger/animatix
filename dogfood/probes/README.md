# Dogfood Probes

A probe is a minimal, reproducible language or grammar gap found while dogfooding.

Use one folder per probe:

- `NNN-<slug>/repro.amx` - the smallest source that reproduces the issue.
- `NNN-<slug>/probe.md` - the structured writeup.

Use the template at `../templates/probe.md`. Keep repros independent of project
scenes so they are useful to the parser, analyzer, and runtime teams.

Probes are allowed to fail. The failure itself is the deliverable.
