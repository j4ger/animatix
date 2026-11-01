# Animatix Documentation

| Document | Audience | Description |
|----------|----------|-------------|
| [`spec.md`](spec.md) | Users | Language specification: syntax, primitives, actions, modifiers, imports, reactive system, multi-scene composition |
| [`primitives.md`](primitives.md) | Users | Quick reference for every scene primitive, graph primitive, and container |
| [`properties.md`](properties.md) | Users | Property reference table: all actor properties, types, and applicability |
| [`effects.md`](effects.md) | Users | Effect chain model: built-in effects, parameters, support regions, and the second-input ABI |
| [`review.md`](review.md) | Users | `animatix review` guide: command roles, flags, JSON schema, rules, exit codes, and limitations |
| [`extension_authoring.md`](extension_authoring.md) | Contributors | Registering primitives, actions, functions, and native plugins |
| [`architecture.md`](architecture.md) | Contributors | System architecture: pipeline, data structures, runtime, layout, rendering, property system, composition |
| [`ecs.md`](ecs.md) | Contributors | Timeline entity/component design: `World` stores, label→id boundary, component checklist, snapshots, structure gate |
| [`primitive_abstraction.md`](primitive_abstraction.md) | Contributors | Primitive trait model: what is unified, accepted boundaries, remaining non-uniformity, deferred decisions, and a recommended order |
| [`series_construction.md`](series_construction.md) | Contributors | LG-1 design note: building multi-part series from scenes, composition, and generated wrappers |
| [`delightful_animation_plan.md`](delightful_animation_plan.md) | Contributors | Kinetic typography, physics handoff, camera, and transition plan (Waves 1–4) |
| [`ai_agent_animation_quality.md`](ai_agent_animation_quality.md) | Contributors | AI agent review/evaluation architecture and bring-up plan |
| [`scene_stats.md`](scene_stats.md) | Contributors | Design note for scene statistics (roadmap STAT-1): build-time baked derived curves read from `always`, and why smoothing is a pure function |
| [`handoff_silent_drops.md`](handoff_silent_drops.md) | Contributors | Handoff for WP6/WP7/WP8: silent-drop gates, verification discipline, and remaining work |
| [`performance_evaluation.md`](performance_evaluation.md) | Contributors | Performance evaluation framework: layers, harnesses, noise floors, and recorded measurements |
| [`contributing.md`](contributing.md) | Contributors | Build/test workflows, project structure, LSP setup, error model, commit messages |
| [`../web/README.md`](../web/README.md) | Users | Browser player: `<amx-player>` embed component, transformer demo, slim playback build |
| [`../dogfood/README.md`](../dogfood/README.md) | Both | In-progress real-content projects and grammar probes |
| [`gui_design_language.md`](gui_design_language.md) | Contributors | GUI visual design language, token system, component taxonomy, interaction model |
| [`roadmap.md`](roadmap.md) | Both | Canonical source of truth for *remaining* work |
| [`history/README.md`](history/README.md) | Contributors | Chronological archive of completed, resolved, and closed work (evidence, not tasks) |

## Quick Reference

- **Language spec matrix**: `spec.md` §Language Status Matrix
- **All primitives**: `primitives.md`
- **All properties**: `properties.md`
- **Effect chain and parameters**: `effects.md`
- **Review, storyboard, and verification**: `review.md`
- **System architecture**: `architecture.md`
- **Timeline ECS / component storage**: `ecs.md`
- **Primitive abstraction status/gaps**: `primitive_abstraction.md`
- **Performance method and measurements**: `performance_evaluation.md`
- **How to build/test**: `contributing.md` §Development Workflows
- **What's next**: `roadmap.md`
- **What landed and why**: `history/README.md`
- **Dogfooding**: `../dogfood/README.md`
