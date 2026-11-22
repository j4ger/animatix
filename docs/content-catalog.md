# Example sources

Public scenes live in [`web/content/scenes/`](../web/content/scenes/). The
homepage, showcase, lessons, techniques and Playground use this source tree.
The [repository index](../examples/README.md) opens the same files with native
tools. Compose each entry around one teaching goal or a complete explanation.

The [Playground catalog](../web/app/src/lib/scene-catalog.ts) derives choices
from the page metadata and includes the two rhythm studies. The site build
checks that both languages list every public entry exactly once and that the
URLs resolve at the root and at the project subpath. Public resources must be
inside `web/content/` so their relative paths work in the assembled site.

Put regression inputs near their assertions and keep benchmark inputs under
the owning workload. Preserve a benchmark's scene, assets, and IDs across
moves. The narrated starter's media lives with the starter and ships inside
the native binaries. The former public and native collections are available
at [the pre-migration revision](https://github.com/j4ger/animatix/tree/fb1aee420f42e86e08f95f3e8e223394d9955056).
The [former Epicycles walkthrough](https://github.com/j4ger/animatix/tree/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/epicycles)
has no public entry. Its imported harmonic function, Graph stroke reveal and
chapter transition are exercised by the
[SDK composition fixture](../crates/animatix-web/tests/fixtures/amx/composition/curve-handoff.amx).

Use `python3 tools/content/content-catalog.py --check` to inspect the tracked
source inventory. The report includes source hashes, import edges and literal
references. It does not replace the SDK import and asset preparation tests.
