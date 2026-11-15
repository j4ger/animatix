# Cloudflare native-build experiment

This is a separate, non-deploying experiment. It does not change GitHub Pages,
the prepared Pages Direct Upload workflow, DNS, or the production domain.

## First run: inspect the host

The Cloudflare form with **Build command**, **Deploy command** and **Preview
command** is Workers Builds, not Pages. Use a new disposable Worker project,
not the production application. Connecting GitHub grants Cloudflare access;
select only `j4ger/animatix`. Project creation may also create a Cloudflare
deployment token even though this probe itself needs no secret. Review that
authorization in Cloudflare rather than assuming the whole setup is read-only.

The experimental branch must first exist on GitHub. It is deliberately **not
pushed automatically**. After explicitly publishing it, use:

| Field | Value |
| --- | --- |
| Project | `animatix-build-probe` (or an unused experiment name) |
| Branch | `experiment/cloudflare-native-build` |
| Root directory | repository root |
| Build command | `bash scripts/cloudflare-build-experiment.sh probe` |
| Deploy command | `printf 'Probe only: deployment intentionally disabled\n'` |
| Preview command | `printf 'Probe only: deployment intentionally disabled\n'` |
| Build variable | `SKIP_DEPENDENCY_INSTALL=1` |

Do not bind the production domain or add user secrets. Do not leave the default
`npx wrangler deploy` command: that could invoke automatic configuration or a
real deployment. A no-op deploy may cause the platform to report no deployed
version; the probe log, not a deployment badge, is the result of this stage.
If the onboarding UI refuses a no-op deploy, stop rather than deploying dummy
content under a production project.

`probe` performs no installation or file changes. It reports numeric UID/GID,
architecture, CPU count, memory/disk capacity, writable-directory flags and tool
availability. It never prints the environment, credentials, account data,
home-directory names or configuration files. Finding `sudo` does not prove
that privilege escalation is allowed; the script does not invoke it.

## Later stages: do not enable blindly

Keep the no-op Deploy/Preview commands. Replace only the Build command's final
argument after reviewing each preceding result:

| Stage | Evidence | Bound |
| --- | --- | --- |
| `probe` | Basic host and tool availability | No network or install |
| `nix` | Existing Nix can enter pinned `.#web-build`, including wasm std | 5 min |
| `sdk` | Build SDK and verify its current commit/version | 17.5 min |
| `site` | Build SDK and assemble/preflight `_site` in this same job | 17.5 min |

The `site` stage is self-contained: a new Cloudflare build need not retain the
previous stage's SDK. All build stages require a clean checkout, preserve
`flake.lock`, and use the existing Nix shell. No unpinned installer, system
configuration change, `sudo`, secret, Wrangler invocation or automatic deploy
is performed. Non-probe stages fetch dependencies normally through Nix/npm/Cargo.

Nix is **not documented as preinstalled**. If absent, later stages fail with a
clear blocker. A bootstrap must be chosen after seeing the actual host:
confirm a permitted writable store, then pin the official installer/archive
version and checksum before executing it. Do not use a mutable `curl | sh`,
change host isolation, or pretend setting `IN_NIX_SHELL` installs dependencies.
The probe intentionally does not test namespace mounts or request privileges.

## Acceptance and limits

Record a genuinely empty-cache run and a repeated run. The local 130-second SDK
build was warm and is not a Cloudflare cold-build prediction. Logs include stage
wall time; monitor Cloudflare for memory/disk failures. Set a target comfortably
below the platform's 20-minute limit (preferably 12–15 minutes cold).

Workers Builds Free currently provides 2 vCPU, 8 GB RAM, 20 GB disk, one concurrent
build and 3,000 build minutes/month. Pages is a different product with 500 free
builds/month. Do not transfer resource assumptions between them. Neither Nix
store nor Cargo target persistence is guaranteed by the Pages cache contract.

These stages do **not** replace full CI, browser/WebGPU verification, scene
acceptance, same-SHA CI gating, custom-domain verification, or production rollout.
There is no Workers hosting config in this experiment. A successful static build
is only a decision point for implementing one later.

## References

- [Workers build configuration](https://developers.cloudflare.com/workers/ci-cd/builds/configuration/)
- [Workers build image](https://developers.cloudflare.com/workers/ci-cd/builds/build-image/)
- [Workers limits](https://developers.cloudflare.com/workers/ci-cd/builds/limits-and-pricing/)
- [Pages build image](https://developers.cloudflare.com/pages/configuration/build-image/)
- [Pages caching](https://developers.cloudflare.com/pages/configuration/build-caching/)

## Verified rootless bootstrap experiment

The observed Workers host has no writable `/nix` and no sudo, but permits
unprivileged user/mount namespaces. `scripts/cloudflare-nix-bootstrap.py` tests
that route without modifying the host store or security settings. Build command:

```sh
python3 scripts/cloudflare-nix-bootstrap.py; exit 1
```

It downloads only official Nix **2.34.9**, verifies the hard-coded SHA256 before
extracting/executing, and assembles a temporary root. Source checksum:
<https://releases.nixos.org/nix/nix-2.34.9/nix-2.34.9-x86_64-linux.tar.xz.sha256>.
No daemon, installer, profile changes, system users, or credentials are created.
The current unprivileged host UID maps to UID 0 **inside the child namespace
only**; this is not host root. The stage refuses execution in its parent
namespaces or with a host-root mapping. System program directories are read-only
mounts; procfs is a fresh read-only mount inside a separate PID namespace, not
a remount of host proc. Only null/zero/random host device handles are exposed;
Nix receives a separate devpts instance limited to 32 PTYs with mode 0600,
not the host's terminals. The host's
`/etc` is not mounted: the temporary root receives only its resolver and public
CA certificate bundle, plus generated minimal account/name-service files.

The experiment enters the pinned `.#web-build` shell and checks Rust, wasm std,
Node, wasm-bindgen, wasm-opt and zopfli. It does not compile the full SDK or run
browser acceptance. The supervisor starts only with 6 GiB free, stops after
15 minutes or below 3 GiB free, and terminates the entire child process group.
There is no disabled sandbox or relaxed signature policy. Host resources remain
restricted by Cloudflare even inside the child namespace. It always exits 1,
including on success, to prevent the deployment stage. Read the explicit
`BOOTSTRAP_AND_DEVSHELL_PASS` or `BOOTSTRAP_BLOCKED` evidence in logs.
