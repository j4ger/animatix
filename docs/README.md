# Animatix Documentation

| Document | Audience | Description |
|----------|----------|-------------|
| [`spec.md`](spec.md) | Users | Language specification: syntax, primitives, actions, modifiers, imports, reactive system, multi-scene composition |
| [`primitives.md`](primitives.md) | Users | Quick reference for every scene primitive, graph primitive, and container |
| [`properties.md`](properties.md) | Users | Property registry reference: all actor properties, types, and applicability |
| [`architecture.md`](architecture.md) | Contributors | System architecture: pipeline, data structures, runtime, layout, rendering, property system, composition |
| [`primitive_abstraction.md`](primitive_abstraction.md) | Contributors | Primitive trait model: what is unified, accepted boundaries, remaining non-uniformity, deferred decisions, and a recommended order |
| [`contributing.md`](contributing.md) | Contributors | Build/test workflows, project structure, LSP setup, error model, commit messages |
| [`roadmap.md`](roadmap.md) | Both | Canonical source of truth for *remaining* work |
| [`history.md`](history.md) | Contributors | Archive of completed, resolved, and closed work (evidence, not tasks) |
| [`extension_abstraction_plan.md`](extension_abstraction_plan.md) | Contributors | Large refactor plan for dynamic primitives/properties |
| [`unified_extension_design.md`](unified_extension_design.md) | Contributors | Single descriptor/registry target and phased migration |
| [`extension_authoring.md`](extension_authoring.md) | Contributors | How to register primitives, actions, functions, services, and plugins |
| [`ai_agent_animation_quality.md`](ai_agent_animation_quality.md) | Contributors | AI agent review/evaluation architecture and bring-up plan |
| [`scene_stats.md`](scene_stats.md) | Contributors | Design note for scene statistics (roadmap STAT-1): build-time baked derived curves read from `always`, and why smoothing is a pure function |
| [`../web/README.md`](../web/README.md) | Users | Browser player: `<amx-player>` embed component, transformer demo, slim playback build |
| [`../dogfood/README.md`](../dogfood/README.md) | Both | In-progress real-content projects and grammar probes |
| [`gui_design_language.md`](gui_design_language.md) | Contributors | GUI visual design language, token system, component taxonomy, interaction model |

## Quick Reference

- **Language spec matrix**: `spec.md` §Language Status Matrix
- **All primitives**: `primitives.md`
- **All properties**: `properties.md`
- **System architecture**: `architecture.md`
- **Primitive abstraction status/gaps**: `primitive_abstraction.md`
- **How to build/test**: `contributing.md` §Development Workflows
- **What's next**: `roadmap.md`
- **Dogfooding**: `../dogfood/README.md`
