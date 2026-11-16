# Web tooling

- `build/`: SDK/Wasm build and font preparation.
- `site/`: Astro build, assembled artifact and site validation.
- `test/`: browser smoke, transactions, cross-platform text and performance probes.
- `dev/`: local server and inspection helpers.
- `deploy/`: Cloudflare Pages signed SDK consumption and serialized release protocol.
- `animatix-web-build/`, `animatix-text-parity/`: Rust workspace tools and their narrowly scoped fixtures.
- `fixtures/`: reusable Web performance inputs.

The JavaScript SDK assembly and embed bundler remain beside this directory's `package.json`; nested tools share this dependency installation. This directory is included in the SDK source fingerprint. Build outputs and dependency caches are not source inputs.

Use `tools/ci/ci.sh gates` from the repository root for the supported gate names and required Nix shells. Nothing here is a second CI definition.
