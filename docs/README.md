# Documentation

## Start here

- [Language tour](https://amx.j4ger.dev/tour/) — learn through animated examples.
- [Showcase](https://amx.j4ger.dev/demos/) — watch finished scenes in the browser.
- [Examples](../examples/README.md) — run and adapt `.amx` files.
- [Desktop workbench](gui-workbench.md) — preview, review timing, and navigate to source.
- [Build and run](contributing.md) — native tools, Nix shells, and export setup.
- [Browser player](../web/README.md) — embed animations and build the website.

## Write animations

| Reference | Covers |
| --- | --- |
| [Language](spec.md) | Syntax, timing, expressions, imports, and composition |
| [Primitives](primitives.md) | Shapes, text, plots, audio, and containers |
| [Properties](properties.md) | Types, applicability, and animation support |
| [Effects](effects.md) | Effect chains, parameters, and support regions |
| [Visual review](audit.md) | Storyboards, audit evidence, and regression checks |
| [Figure content](figure-content-spec.md) | Figure-kit templates and text budgets |

## Contribute

Start with the [contribution guide](../CONTRIBUTING.md). For implementation details:

- [Architecture](architecture.md) and [timeline ECS contract](ecs.md)
- [Extension authoring](extension_authoring.md) and [primitive abstraction](primitive_abstraction.md)
- [Performance evaluation](performance_evaluation.md)
- [GUI design language](gui_design_language.md) and [application identity](branding.md)
- [Dogfooding](../dogfood/README.md): projects, probes, and comparisons

## Plans and history

[Roadmap](roadmap.md) tracks remaining work; [history](history/README.md) records completed work and its evidence. Design notes describe proposals or implementation rationale, not a separate feature checklist:

- [Animation direction](delightful_animation_plan.md)
- [AI-assisted animation review](ai_agent_animation_quality.md)
- [Multi-part series](series_construction.md)
- [Scene statistics](scene_stats.md)
- [Silent-drop verification handoff](handoff_silent_drops.md)
