#!/usr/bin/env bash
# The one definition of "is this green".
#
# Gates and job IDs are registered below. Workflows consume generated matrices;
# PR selection falls back to the full matrix on uncertainty. Each standalone gate
# or shared audit pair runs in an independent measured shell, retaining failures.
#
# The `shell=` column is the load-bearing part. Animatix needs five different
# environments and the wrong one fails in a way that looks like a code bug:
#
#   native  macOS/Windows OS verification with their native SDK/toolchain.
#   dist-driver Nix tools orchestrating the isolated Ubuntu Docker distribution.
#   nix     `nix develop` — provides ALSA + FFmpeg headers + Vulkan/Mesa + cargo-audit.
#   web     `nix develop .#web-build` — wasm32 std + the pinned wasm-bindgen CLI.
#   web-ci  `nix develop .#web-ci` — web-build plus Chromium/node/python for web-smoke.
#   none    pure planner/result orchestration, with no Rust toolchain.
#
# `ci.sh` deliberately never enters a shell itself. DevShells *prepend* to PATH
# rather than replacing it, so entering one from inside another (or from a
# direnv-loaded login shell) keeps the OUTER rustc first — `rustc --print sysroot`
# then reports the toolchain with no wasm32 std and the build fails exactly as
# before, while wasm-opt and brotli resolve correctly, which makes the shell look
# like it loaded fine. The caller decides the shell; `gate` asserts the one it got.
#
# Usage
#   tools/ci/ci.sh --list                 name<TAB>shell<TAB>os<TAB>setup<TAB>stage, one row per gate
#   tools/ci/ci.sh --matrix               the same as JSON, for the CI matrix
#   tools/ci/ci.sh gates                  human-readable listing
#   tools/ci/ci.sh gate <name> [<name>…]  run gates in this shell
#   tools/ci/ci.sh setup <token>…         provision what the tokens mean here
#   tools/ci/ci.sh shell <name>           print the shell command this gate expects
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# ── the registry ──────────────────────────────────────────────────────────────
# name | shell | os | setup tokens | stage
#
# Automatic stages and explicit-only orchestration:
#   pr        eligible PR checks; main and release callers run the full set.
#   release   build-only: the distribution jobs that produce the binaries.
#   site      composite gates: site-artifact/dist-site/dist-web-sdk need the
#             gitignored web/runtime SDK in the same job that built them, so
#             they are run by name from pages.yml and release.yml and are not
#             matrix rows.
#   control   orchestration gates, never matrix jobs.
#
# Adding a gate here is the whole job: `--list` feeds the matrix, so CI picks it
# up. Nothing in .github/ has to learn its name.
GATE_TABLE='
splitter-probe-installer|none|ubuntu-latest|none|control
splitter-probe-tests|none|ubuntu-latest|none|control
splitter-probe-input|web|ubuntu-latest|nix|control
splitter-probe-compare|web|ubuntu-latest|nix|control
fmt|nix|ubuntu-latest|nix|pr
lint|nix|ubuntu-latest|nix|pr
check|nix|ubuntu-latest|nix|pr
test|nix|ubuntu-latest|nix|pr
no-video|nix|ubuntu-latest|nix|pr
render-smoke|nix|ubuntu-latest|nix|pr
examples|nix|ubuntu-latest|nix|pr
ext-bench|nix|ubuntu-latest|nix|pr
doc|nix|ubuntu-latest|nix|pr
doc-links|nix|ubuntu-latest|nix|pr
ci-policy-tests|nix|ubuntu-latest|nix|pr
site-scenes|nix|ubuntu-latest|nix|site
site-build|nix|ubuntu-latest|nix|pr
embed-build|nix|ubuntu-latest|nix|pr
web-runtime-tests|nix|ubuntu-latest|nix|pr
web-sdk-tests|nix|ubuntu-latest|nix|pr
native-package-tests|nix|ubuntu-latest|nix|pr
perf-policy-tests|nix|ubuntu-latest|nix|pr
content-sync|nix|ubuntu-latest|nix|pr
structure|nix|ubuntu-latest|nix|pr
meta-version|nix|ubuntu-latest|nix|pr
workflows|nix|ubuntu-latest|nix|pr
eparts|nix|ubuntu-latest|nix|pr
eparts|native|macos-latest|rust-stable|pr
eparts|native|windows-latest|rust-stable|pr
audit|nix|ubuntu-latest|nix|pr
scene-audit|nix|ubuntu-latest|nix|pr
canonical-audit|nix|ubuntu-latest|nix|pr
budget-audit|nix|ubuntu-latest|nix|pr
wasm-check|web|ubuntu-latest|nix|pr
web-build|web-ci|ubuntu-latest|nix|pr
web-smoke|web-ci|ubuntu-latest|nix|pr
web-site-smoke|web-ci|ubuntu-latest|nix|pr
site-smoke|web-ci|ubuntu-latest|nix|site
web-split-proof|web-ci|ubuntu-latest|nix|pr
web-sdk-integrity|web-ci|ubuntu-latest|nix|pr
text-parity|web-ci|ubuntu-latest|nix|pr
web-raster-proof|web-ci|ubuntu-latest|nix|pr
web-transactions|web-ci|ubuntu-latest|nix|pr
# Composite site-stage gates — never matrix jobs (see the stage note above).
cloudflare-prepare|nix|ubuntu-latest|nix|site
cloudflare-deploy|nix|ubuntu-latest|nix|site
site-artifact|nix|ubuntu-latest|nix|site
dist-site|nix|ubuntu-latest|nix|site
dist-web-sdk|nix|ubuntu-latest|nix|site
web-artifact-verify|nix|ubuntu-latest|nix|site
web-artifact-stage|nix|ubuntu-latest|nix|site
web-artifact-restore|nix|ubuntu-latest|nix|site
web-artifact-absence|none|ubuntu-latest|none|control
cloudflare-pages-publish|none|ubuntu-latest|none|control
cloudflare-pages-build|none|ubuntu-latest|none|control
cloudflare-pages-release|none|ubuntu-latest|none|control
cloudflare-pages-initialize|none|ubuntu-latest|none|control
cloudflare-sdk-publish|none|ubuntu-latest|none|control
cloudflare-sdk-dispatch|none|ubuntu-latest|none|control
text-visual-parity|web-ci|ubuntu-latest|nix|site
web-performance|web-ci|ubuntu-latest|nix|site
dist-linux|dist-driver|ubuntu-latest|nix|release
dist-macos|native|macos-latest|rust-stable|release
dist-windows|native|windows-latest|rust-stable|release
ci-plan|none|ubuntu-latest|none|control
ci-result|none|ubuntu-latest|none|control
release-resolve|none|ubuntu-latest|none|control
release-verify|none|ubuntu-latest|none|control
release-notes|none|ubuntu-latest|none|control
release-publish|none|ubuntu-latest|none|control
'

# ── CI matrix jobs ────────────────────────────────────────────────────────────
# name | gates | shell | os | setup tokens | stage | stable id
#
# Consolidated jobs for CI. Rather than spawning a separate VM for each
# individual gate (which caused 21 runners to hammer GitHub API and Nix caches,
# taking minutes to spin up each for 3-second checks), related checks run in
# the same job and reuse compiler/toolchain artifacts.
#
# Every gate listed here is defined above in GATE_TABLE and can still be run
# individually via `tools/ci/ci.sh gate <name>`. The `workflows` gate verifies
# each job's shell/os/stage/setup against every gate it lists, so a job cannot
# silently run a gate in the wrong environment.
JOB_TABLE='
Fast Checks|fmt structure workflows site-build content-sync meta-version embed-build web-runtime-tests web-sdk-tests native-package-tests perf-policy-tests|nix|ubuntu-latest|nix|pr|fast-checks
Documentation Links|doc-links ci-policy-tests|nix|ubuntu-latest|nix|pr|doc-links
eparts|eparts|nix|ubuntu-latest|nix|pr|eparts-linux
eparts|eparts|native|macos-latest|rust-stable|pr|eparts-macos
eparts|eparts|native|windows-latest|rust-stable|pr|eparts-windows
Security Audit|audit|nix|ubuntu-latest|nix|pr|security-audit
Check & Lint|scene-audit budget-audit canonical-audit check lint doc no-video|nix|ubuntu-latest|nix|pr|check-lint
Test & Examples|test examples|nix|ubuntu-latest|nix|pr|test-examples
Render Smoke & Scenes|render-smoke|nix|ubuntu-latest|nix|pr|render-scenes
Benchmarks|ext-bench|nix|ubuntu-latest|nix|pr|benchmarks
Web Check|wasm-check|web|ubuntu-latest|nix|pr|web-check
Web Build|web-split-proof web-build web-sdk-integrity text-parity web-raster-proof web-smoke web-transactions web-site-smoke|web-ci|ubuntu-latest|nix|pr|web-build
dist-linux|dist-linux|dist-driver|ubuntu-latest|nix|release|dist-linux
dist-macos|dist-macos|native|macos-latest|rust-stable|release|dist-macos
dist-windows|dist-windows|native|windows-latest|rust-stable|release|dist-windows
'

# The commands each gate expects to be run by. Kept as prose so `ci.sh shell`
# can print them and a human can copy one.
shell_for() {
  case "$1" in
    native) echo '' ;;
    nix) echo 'nix develop --command' ;;
    web) echo 'nix develop .#web-build --command' ;;
    web-ci) echo 'nix develop .#web-ci --command' ;;
    dist-driver) echo 'nix develop .#dist-driver --command' ;;
    none) echo '' ;;
    *) echo "unknown shell: $1" >&2; return 1 ;;
  esac
}

# Repeated gate names (eparts) have different requirements on each OS.
# RUNNER_OS is supplied by Actions; local queries derive it from uname.
gate_field() {
  local platform="${RUNNER_OS:-$(uname -s)}" runner
  case "$platform" in
    Linux) runner=ubuntu-latest ;;
    macOS|Darwin) runner=macos-latest ;;
    Windows|MINGW*|MSYS*|CYGWIN*) runner=windows-latest ;;
    *) echo "error: unsupported platform: $platform" >&2; return 1 ;;
  esac
  awk -F'|' -v g="$1" -v col="$2" -v os="$runner" '
    $1==g { count++; value=$col; if ($3==os) { found=1; selected=$col } }
    END { if (found) print selected; else if (count==1) print value;
          else { print "error: no unambiguous gate row for " g " on " os > "/dev/stderr"; exit 1 } }
  ' <<<"$GATE_TABLE"
}
gate_shell() { gate_field "$1" 2; }
gate_os() { gate_field "$1" 3; }
gate_setup() { gate_field "$1" 4; }
gate_stage() { gate_field "$1" 5; }

# ── environment assertions ────────────────────────────────────────────────────
# Each gate checks the environment it was promised, because the interesting
# failures are the ones that look like code faults: a stable rustc formatting
# against unstable rustfmt options, a rustc whose sysroot has no wasm32 std.
want_cargo() {
  command -v cargo >/dev/null 2>&1 || { echo "error: no cargo on PATH" >&2; return 1; }
}

want_node() {
  command -v node >/dev/null 2>&1 || { echo "error: no node on PATH" >&2; return 1; }
}

want_chromium() {
  command -v chromium >/dev/null 2>&1 || {
    echo "error: chromium is not on PATH — the web-smoke gate needs it." >&2
    echo "       Run it as: $(shell_for web-ci) tools/ci/ci.sh gate web-smoke" >&2
    return 1
  }
}

want_nix_shell() {
  [ -n "${IN_NIX_SHELL:-}" ] || {
    echo "error: this gate is shell=$(gate_shell "$1") — run it as:" >&2
    echo "       $(shell_for "$(gate_shell "$1")") tools/ci/ci.sh gate $1" >&2
    return 1
  }
}

want_wasm_std() {
  if ! ls "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" >/dev/null 2>&1; then
    echo "error: this rustc has no wasm32-unknown-unknown std (sysroot: $(rustc --print sysroot))." >&2
    echo "       That is the shell, not the code: the target may be installed under" >&2
    echo "       ~/.rustup while this toolchain cannot see it. Run it as:" >&2
    echo "       $(shell_for "$(gate_shell "$1")") tools/ci/ci.sh gate $1" >&2
    return 1
  fi
}

# ── gates ─────────────────────────────────────────────────────────────────────
gate_fmt() {
  want_cargo
  cargo fmt --all -- --check
}

gate_lint() {
  want_cargo
  cargo clippy --workspace --all-targets -- -D warnings
}

gate_check() {
  want_cargo
  cargo check --workspace --all-targets
}

gate_test() {
  want_cargo
  # Build once with workspace feature unification; verify every active test ID.
  # Keep engine/renderer harnesses serial and require a GPU adapter; run at most
  # two other harnesses concurrently, with one libtest thread per process.
  python3 tools/ci/workspace-tests.py
}

measure_phase() {
  local gate="$1" phase="$2" kind="$3"
  shift 3
  python3 "$root/tools/ci/phase-timing.py" --gate "$gate" --phase "$phase" --kind "$kind" -- "$@"
}

gate_no_video() {
  want_cargo
  # The `video` feature must stay truly opt-in. This gate runs in the nix dev
  # shell, which ships FFmpeg, so "it built without FFmpeg present" cannot be
  # demonstrated here — the old comment claimed exactly that and was false.
  # Build the default user-facing binaries, run the engine with every optional
  # feature off, and inspect the resolved feature graph: rsmpeg is pulled in
  # only by animatix-render/video, so its presence means a default build turned
  # video on. The minimal engine must also exclude rich-text dependencies.
  measure_phase no-video default-binaries cargo cargo build -p animatix-cli -p animatix-gui
  measure_phase no-video minimal-engine cargo cargo check -p animatix --no-default-features
  local pkg tree
  tree=$(measure_phase no-video minimal-dependency-graph cargo cargo tree -p animatix --no-default-features -e normal --prefix none --format '{p}')
  if grep -Eq '^(typst([_-][[:alnum:]_-]+)?|syntect) v' <<<"$tree"; then
    echo "error: minimal engine dependencies include the rich-text stack" >&2
    return 1
  fi
  for pkg in animatix-cli animatix-gui; do
    if ! tree="$(measure_phase no-video "$pkg-feature-graph" cargo cargo tree -e features -p "$pkg" 2>/dev/null)"; then
      echo "error: cargo tree failed for $pkg; cannot verify the video feature is off" >&2
      return 1
    fi
    if grep -q 'rsmpeg' <<<"$tree"; then
      echo "error: $pkg default feature resolution enables rsmpeg (the video feature)" >&2
      return 1
    fi
  done
  echo "no-video: minimal engine excludes rich text; default CLI/GUI exclude rsmpeg (video)"
}

gate_canonical_audit() {
  want_nix_shell canonical-audit
  measure_phase canonical-audit build-cli cargo cargo build --bin animatix
  local evidence
  mkdir -p target/ci/canonical-audit
  evidence=$(mktemp -d target/ci/canonical-audit/run-XXXXXXXX)
  measure_phase canonical-audit audit-scenes execution python3 tools/quality/audit-canonical.py \
    --output "$evidence/raw" --ci-summary "$evidence/summary.json"
}

gate_scene_audit() {
  local audit_mode="${1:-standalone}" timing_gate=scene-audit
  if [ "$audit_mode" = joint ]; then timing_gate=scene-budget-audit; fi
  want_nix_shell 'scene-audit'
  measure_phase "$timing_gate" build-cli cargo cargo build --bin animatix

  local jobs="${ANIMATIX_SCENE_AUDIT_JOBS:-2}"
  if ! [[ "$jobs" =~ ^[1-4]$ ]]; then
    echo "error: ANIMATIX_SCENE_AUDIT_JOBS must be between 1 and 4" >&2
    return 2
  fi

  # The scene walk shared with tools/quality/audit-budget.sh: component libraries,
  # site copies, the plugin showcase and the pure-`pub let` module are not
  # standalone scenes; a multi-file project is driven through its main.amx.
  scenes() {
    python3 tools/quality/audit_corpus.py
  }

  audit_one() {
    local f="$1" mode="${2:-standalone}" output
    local -a policy=()
    # The standalone scene gate suppresses CLI stderr. The union exposes it,
    # retaining budget diagnostics that the second standalone pass would show.
    if [ "$mode" = joint ]; then
      policy=(--policy default)
    fi
    output=$(mktemp) || {
      echo "FAIL scene-audit (temp file): $f" >&2
      return 1
    }
    if [ "$mode" = joint ]; then
      local cleanup
      printf -v cleanup 'rm -f -- %q' "$output"
      trap "$cleanup" EXIT
    fi
    if ! {
      if [ "$mode" = joint ]; then
        ./target/debug/animatix audit "$f" --budget tools/quality/audit-budget.json \
          --render never "${policy[@]}" --format json
      else
        ./target/debug/animatix audit "$f" --budget tools/quality/audit-budget.json \
          --render never --format json 2>/dev/null
      fi
    } > "$output"; then
      echo "FAIL scene-audit (exit code): $f" >&2
      rm -f "$output"
      return 1
    fi

    # JSON validation with python3
    if ! python3 -c '
import json, sys
try:
    with open(sys.argv[1]) as f:
        data = json.load(f)
    if data.get("schema_version") != 3:
        print("schema_version %s != 3" % data.get("schema_version"), file=sys.stderr)
        sys.exit(1)
    if data.get("kind") != "audit":
        print("kind %s != audit" % data.get("kind"), file=sys.stderr)
        sys.exit(1)
    if data.get("summary", {}).get("errors", 0) != 0:
        print("errors %s != 0" % data.get("summary", {}).get("errors"), file=sys.stderr)
        sys.exit(1)
except Exception as e:
    print(f"JSON error: {e}", file=sys.stderr)
    sys.exit(1)
' "$output"; then
      echo "FAIL scene-audit (validation): $f" >&2
      rm -f "$output"
      return 1
    fi
    rm -f "$output"
    printf 'OK scene-audit: %s\n' "$f"
  }
  export -f audit_one scenes

  # GNU xargs keeps each pathname intact and reports nonzero if any worker fails.
  if ! measure_phase "$timing_gate" audit-corpus execution bash -u -o pipefail -c \
    'scenes | xargs -d "\n" -r -n 1 -P "$1" bash -c '\''audit_one "$2" "$1"'\'' _ "$2"' _ "$jobs" "$audit_mode"; then
    echo "scene-audit: one or more scene audits failed" >&2
    return 1
  fi
}

gate_budget_audit() {
  want_nix_shell 'budget-audit'
  measure_phase budget-audit build-cli cargo cargo build --bin animatix
  measure_phase budget-audit check-budget execution bash tools/quality/audit-budget.sh --check
}

gate_render_smoke() {
  want_cargo
  cargo build --bin animatix
  ANIMATIX_REQUIRE_GPU=1 bash tools/quality/render_smoke.sh
}

gate_examples() {
  want_cargo
  cargo build --bin animatix
  bash tools/quality/check_examples.sh
}

gate_ext_bench() {
  want_cargo
  bash tools/quality/extension-bench.sh --quick --max-plan-ns 10000
}

gate_doc() {
  want_cargo
  RUSTDOCFLAGS="-D warnings" measure_phase doc rustdoc cargo cargo doc --workspace --no-deps
  # Preserve cargo run exactly; its build + program cost is honestly mixed.
  measure_phase doc property-reference mixed cargo run -p animatix --example property_reference -- --check
}

gate_doc_links() { bash tools/docs/check-docs.sh; }

gate_ci_policy_tests() {
  python3 tools/quality/test-legacy-contract-fixtures.py
  python3 tools/quality/test-audit-corpus.py
  python3 tools/ci/test-ci-control.py
  python3 tools/ci/test-security-audit.py
  python3 tools/ci/test-security-policy.py
  python3 tools/ci/test-phase-timing.py
  python3 tools/ci/test-phase-gates.py
  python3 tools/ci/test-joint-audit.py
  python3 tools/ci/test-ci-plan.py
  python3 tools/ci/test-main-baseline.py
  python3 tools/release/release-tests.py
  python3 tools/ci/test-release-workflow.py
  python3 web/tools/test/test-web-smoke-gates.py
  python3 web/tools/dev/test-serve-web-base.py
  python3 -m unittest discover -s tools/ci/lib -p 'test_*.py'
}

gate_ci_plan() {
  mkdir -p target/ci
  local event="${GITHUB_EVENT_NAME:-workflow_dispatch}"
  if [ -n "${ANIMATIX_CI_CHECKOUT_REF:-}" ] || [ "${ANIMATIX_CI_REUSABLE:-false}" = true ]; then
    event=workflow_call
  fi
  python3 tools/ci/main-baseline.py --repo "$root" \
    --event-file "${GITHUB_EVENT_PATH:-}" --repository "${GITHUB_REPOSITORY:-}" \
    --head "${GITHUB_SHA:-HEAD}" --event "$event" \
    --checkout-ref "${ANIMATIX_CI_CHECKOUT_REF:-}" --output target/ci/main-baseline.json
  python3 tools/ci/ci-plan.py --repo "$root" --event "$event" \
    --main-baseline target/ci/main-baseline.json \
    --base "${ANIMATIX_CI_BASE:-}" --head "${GITHUB_SHA:-HEAD}" > target/ci/plan.json
  python3 tools/ci/ci-control.py plan target/ci/plan.json
}

gate_ci_result() { python3 tools/ci/ci-control.py required; }

# Web content and SDK boundary checks.
gate_cloudflare_pages_publish() {
  python3 web/tools/deploy/cloudflare-pages.py publish
}

gate_splitter_probe_installer() { python3 tools/ci/experiments/splitter_probe.py installer; }
gate_splitter_probe_tests() { python3 tools/ci/experiments/test_splitter_probe.py; }
gate_splitter_probe_input() { python3 tools/ci/experiments/splitter_probe.py input; }
gate_splitter_probe_compare() { python3 tools/ci/experiments/splitter_probe.py compare; }

gate_cloudflare_pages_initialize() {
  python3 web/tools/deploy/cloudflare-pages-release.py --initialize
}

gate_cloudflare_pages_release() {
  python3 web/tools/deploy/cloudflare-pages-release.py
}

gate_cloudflare_pages_build() {
  python3 web/tools/deploy/cloudflare-pages.py build
}

gate_cloudflare_sdk_publish() {
  want_node
  python3 web/tools/deploy/cloudflare-sdk-publisher.py
}

gate_cloudflare_sdk_dispatch() {
  python3 web/tools/deploy/cloudflare-sdk-dispatch.py
}

gate_cloudflare_prepare() {
  want_nix_shell 'cloudflare-prepare'
  python3 web/tools/deploy/cloudflare-deploy-test.py
  python3 web/tools/deploy/cloudflare-site-check-test.py
  python3 web/tools/deploy/cloudflare-site-check.py _site
  npm --prefix web/deploy ci --no-audit --no-fund
  npm --prefix web/deploy exec --offline -- wrangler --version
}

gate_cloudflare_deploy() {
  want_nix_shell 'cloudflare-deploy'
  bash web/tools/deploy/cloudflare-deploy.sh
}

gate_site_build() {
  want_nix_shell 'site-build'
  python3 web/tools/deploy/cloudflare-retirement-test.py
  python3 web/tools/deploy/cloudflare-pages-test.py
  python3 web/tools/deploy/cloudflare-pages-release-test.py
  python3 web/tools/deploy/cloudflare-permit-test.py
  python3 web/tools/deploy/cloudflare-sdk-dispatch-test.py
  python3 web/tools/deploy/cloudflare-sdk-consumer-test.py
  python3 web/tools/deploy/cloudflare-nix-portable-test.py
  python3 web/tools/deploy/cloudflare-build-experiment-test.py
  python3 web/tools/deploy/cloudflare-nix-bootstrap-test.py
  python3 web/tools/deploy/cloudflare-deploy-test.py
  python3 web/tools/deploy/cloudflare-site-check-test.py
  bash web/tools/site/site-build.sh
}

gate_site_scenes() {
  want_cargo
  # The SDK contains all capabilities; browser dependency tests prove that only
  # the required partitions are downloaded. No separate feature-profile run.
  cargo test -p animatix-web --test site_scenes
  # Astro source pages are not HTML: explicitly check the canonical new scenes.
  cargo build --bin animatix
  local scene count=0
  for scene in web/content/scenes/*.amx; do
    [ -f "$scene" ] || continue
    target/debug/animatix check "$scene" || return 1
    count=$((count + 1))
  done
  [ "$count" -ge 10 ] || { echo "error: expected at least 10 canonical site scenes, found $count" >&2; return 1; }
}


gate_embed_build() {
  # Build the <amx-player> web component bundle twice from the same source and
  # require byte-identical output. The bundle is gitignored, so a clean
  # checkout has no committed artifact to compare against; a single build only
  # proves "a non-empty file appeared", which is not a gate. Two builds catch a
  # bundle that silently omits a module (the old raw-copy failure) and a build
  # that mutates its own inputs.
  command -v npm >/dev/null 2>&1 || { echo "error: npm not on PATH" >&2; return 1; }
  local src_before src_after first_hash second_hash
  embed_src_digest() {
    ( cd web/embed/src && find . -type f -print0 |
        LC_ALL=C sort -z | xargs -0 sha256sum ) | sha256sum | awk '{print $1}'
  }
  src_before="$(embed_src_digest)"
  if ! npm --prefix web/tools ci --silent; then
    echo "error: npm ci failed for web/tools" >&2
    return 1
  fi
  if ! npm --prefix web/tools run build:embed; then
    echo "error: npm run build:embed failed (first build)" >&2
    return 1
  fi
  if [ ! -s web/embed/amx-player.js ]; then
    echo "error: web/embed/amx-player.js missing or empty after the first build" >&2
    return 1
  fi
  first_hash="$(sha256sum web/embed/amx-player.js | awk '{print $1}')"
  src_after="$(embed_src_digest)"
  if [ "$src_before" != "$src_after" ]; then
    echo "error: the embed build modified web/embed/src; refusing to certify a" >&2
    echo "       bundle built from a moving source tree." >&2
    return 1
  fi
  if ! npm --prefix web/tools run build:embed; then
    echo "error: npm run build:embed failed (second build)" >&2
    return 1
  fi
  if [ ! -s web/embed/amx-player.js ]; then
    echo "error: web/embed/amx-player.js missing or empty after the second build" >&2
    return 1
  fi
  second_hash="$(sha256sum web/embed/amx-player.js | awk '{print $1}')"
  src_after="$(embed_src_digest)"
  if [ "$src_before" != "$src_after" ]; then
    echo "error: the embed source tree changed between builds; cannot compare outputs" >&2
    return 1
  fi
  if [ "$first_hash" != "$second_hash" ]; then
    echo "error: two consecutive builds of web/embed/src produced different bundles" >&2
    echo "       (first $first_hash, second $second_hash). The build is not deterministic." >&2
    return 1
  fi
  echo "embed bundle build smoke: two consecutive builds are byte-identical ($second_hash)"
}

gate_web_runtime_tests() {
  want_node
  node --test web/tools/runtime.test.mjs web/tools/perf-statistics.test.mjs web/tools/test/rgba-comparison.test.mjs web/tools/test/cdp-client.test.mjs
}

gate_web_sdk_tests() {
  want_node
  node web/tools/test/web-sdk-tests.mjs
  node --test web/tools/third-party-licenses.test.mjs
}

gate_native_package_tests() {
  python3 tools/release/native-package-tests.py
  python3 crates/animatix-gui/tools/packaging/platform-icon-tests.py
  python3 tools/branding/icon-tests.py
}

gate_perf_policy_tests() {
  python3 crates/animatix-render/tools/test-offscreen-blur-area.py
  python3 tools/quality/test-perf-policy.py
  python3 tools/quality/test-extension-bench.py
}

gate_web_split_proof() {
  want_cargo; want_wasm_std 'web-split-proof'; want_nix_shell 'web-split-proof'
  bash web/tools/build/web-split-proof.sh
}

gate_web_build() {
  want_cargo; want_wasm_std 'web-build'; want_nix_shell 'web-build'
  . web/tools/build/wasm-tools.sh
  check_wbg
  bash web/tools/build/build-web.sh
  gate_web_sdk_integrity
  bash tools/quality/size-report.sh --check
}

gate_web_sdk_integrity() {
  want_node
  node web/tools/sdk.mjs verify web/runtime --release --current --commit "$(git rev-parse HEAD)" --version "$(workspace_version)"
}

gate_text_parity() {
  want_cargo; want_node; want_nix_shell 'text-parity'
  gate_web_sdk_integrity
  node web/tools/test/text-parity.mjs web/runtime/runtime-manifest.json
}

gate_web_raster_proof() {
  want_node
  gate_web_sdk_integrity
  node web/tools/test/web-raster-proof.mjs web/runtime
}

# Hardware evidence gates run explicitly, outside the PR matrix. The harnesses
# require WPR_GPU=1 and reject software adapters; performance also needs an idle
# host, as documented in docs/performance_evaluation.md.
gate_text_visual_parity() {
  want_cargo; want_node; want_nix_shell 'text-visual-parity'
  gate_web_sdk_integrity
  node web/tools/test/text-visual-parity.mjs
}

gate_web_performance() {
  want_node; want_nix_shell 'web-performance'
  gate_web_sdk_integrity
  node web/tools/test/web-runtime-perf.mjs
}

workspace_version() {
  sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1
}

gate_web_artifact_verify() {
  want_node
  node web/tools/sdk.mjs verify web/runtime --release --current --commit "${ANIMATIX_ARTIFACT_COMMIT:-$(git rev-parse HEAD)}" --version "$(workspace_version)"
}

gate_web_artifact_stage() {
  gate_web_artifact_verify
  rm -rf target/web-runtime-artifact
  mkdir -p target/web-runtime-artifact
  cp -RL web/runtime/. target/web-runtime-artifact/
  node web/tools/sdk.mjs verify target/web-runtime-artifact --release
}

gate_web_artifact_absence() {
  python3 tools/ci/ci-control.py artifact-absence \
    --run-id "${ANIMATIX_SOURCE_RUN_ID:?missing source workflow run ID}" --name web-runtime
}

gate_web_artifact_restore() {
  want_node
  # Downloads stay separate from the public SDK until validation succeeds.
  # Only API-confirmed missing/expired artifacts may fall back to a source build.
  node web/tools/sdk.mjs verify target/downloaded-web-runtime --release --current --commit "${ANIMATIX_ARTIFACT_COMMIT:-$(git rev-parse HEAD)}" --version "$(workspace_version)"
  if [ -e web/runtime ] || [ -L web/runtime ]; then
    echo 'error: refusing to overwrite an existing SDK during artifact restore' >&2
    return 1
  fi
  cp -R target/downloaded-web-runtime web/runtime
  gate_web_artifact_verify
}

gate_web_transactions() {
  want_nix_shell 'web-transactions'
  want_node; want_chromium
  command -v xvfb-run >/dev/null || { echo 'web-transactions requires xvfb-run from the web-ci shell' >&2; return 1; }
  node --test web/tools/test/web-transactions-browser.test.mjs
  bash web/tools/build/web-distribution-check.sh
}

# Retry only failures classified by the runner as pre-navigation environment
# startup errors (75). Scene, shader, JS, resource and paint failures never retry.
run_browser_smoke() {
  local suite="$1" attempt status
  for attempt in 1 2; do
    if SMOKE_SUITE="$suite" SMOKE_ATTEMPT="$attempt" \
      SMOKE_OUTPUT_DIR="target/web-smoke/$suite/attempt-$attempt" node web/tools/test/web-smoke.mjs; then
      return 0
    else
      status=$?
    fi
    if [ "$status" -ne 75 ] || [ "$attempt" -eq 2 ]; then
      return "$status"
    fi
    echo "web-smoke: browser environment unavailable; one clean startup retry" >&2
    sleep 2
  done
}

gate_web_smoke() {
  want_nix_shell 'web-smoke'; want_node; want_chromium
  command -v xvfb-run >/dev/null || { echo 'web-smoke requires xvfb-run from the web-ci shell' >&2; return 1; }
  node --test web/tools/test/web-smoke-*.test.mjs
  run_browser_smoke sdk
}

gate_site_smoke() {
  want_nix_shell 'site-smoke'; want_node; want_chromium
  command -v xvfb-run >/dev/null || { echo 'site-smoke requires xvfb-run from the web-ci shell' >&2; return 1; }
  # Never fall back to web/: it contains the obsolete pre-Astro website.
  [ -f _site/index.html ] || { echo 'site-smoke requires gate site-artifact first' >&2; return 1; }
  ANIMATIX_WEB_ROOT="$root/_site" run_browser_smoke site
}

gate_web_site_smoke() {
  want_nix_shell 'web-site-smoke'; want_node; want_chromium
  # Same job, same verified SDK. Assembly deliberately does not rebuild wasm.
  gate_site_artifact
  gate_site_smoke
}

gate_wasm_check() {
  want_cargo; want_wasm_std 'wasm-check'; want_nix_shell 'wasm-check'
  # animatix-web's render module is #[cfg(target_arch = "wasm32")], so a change to
  # the frame-presenting path can compile everywhere, pass every test, and still
  # break the site. This is the check that looks at it.
  cargo check -p animatix-web --target wasm32-unknown-unknown
  cargo check -p animatix-web --target wasm32-unknown-unknown --no-default-features
  cargo clippy -p animatix --no-default-features --all-targets -- -D warnings
}

gate_content_sync() {
  # python3, git and coreutils only — no toolchain, so this gate is cheap enough
  # to run on every PR.
  bash tools/content/content-sync.sh
}

gate_structure() {
  # Direct ceilings on file and function length (docs/ecs.md §8).  The
  # allowlists freeze existing offenders at their current size; splitting a
  # function or file deletes its row.
  bash tools/quality/structure.sh
}

gate_meta_version() {
  want_cargo
  # Version single-source: every workspace member must inherit [workspace.package],
  # so `tools/release/bump-version.sh` (explicit version update) has exactly one place to
  # write and a hand-edited member cannot hide.
  python3 - "$root" <<'PY'
import json, re, subprocess, sys, tomllib
root = sys.argv[1]
meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
    cwd=root, check=True, capture_output=True).stdout)
with open(meta["workspace_root"] + "/Cargo.toml") as source:
    text = source.read()
m = re.search(r'^\[workspace\.package\][^\[]*?^version = "([^"]+)"', text, re.M | re.S)
if not m:
    print("error: no [workspace.package] version in the root Cargo.toml", file=sys.stderr)
    sys.exit(1)
want = m.group(1)
bad = [(p["name"], p["version"]) for p in meta["packages"] if p["version"] != want]
for package in meta["packages"]:
    with open(package["manifest_path"], "rb") as source:
        declared = tomllib.load(source).get("package", {}).get("version")
    if declared != {"workspace": True}:
        bad.append((package["name"], "must declare version.workspace = true"))
if bad:
    print("error: packages not at workspace version %s:" % want, file=sys.stderr)
    for name, ver in bad:
        print("  %s = %s" % (name, ver), file=sys.stderr)
    sys.exit(1)
print("all %d workspace packages at version %s" % (len(meta["packages"]), want))
PY
}

# The honesty gate. Without it the duplication this file removes simply returns:
# someone adds a `run: cargo test ...` line to a workflow, it passes, and the next
# person copies that instead of registering a gate.
gate_workflows() {
  local bad=0 f
  for f in .github/workflows/*.yml .github/workflows/*.yaml; do
    [ -e "$f" ] || continue
    if awk '
      # A `run:` key opens a block; its body is every later line indented deeper.
      /^[[:space:]]*-[[:space:]]+run:[[:space:]]*/ { inrun = 1; indent = match($0, /[^[:space:]]/); inline = $0; sub(/^[[:space:]]*-[[:space:]]+run:[[:space:]]*/, "", inline); body(inline, FILENAME, FNR); next }
      /^[[:space:]]*run:[[:space:]]*/            { inrun = 1; indent = match($0, /[^[:space:]]/); inline = $0; sub(/^[[:space:]]*run:[[:space:]]*/, "", inline); body(inline, FILENAME, FNR); next }
      { if (inrun) { if ($0 ~ /^[[:space:]]*$/ || match($0, /[^[:space:]]/) > indent) body($0, FILENAME, FNR); else inrun = 0 } }
      END { exit 0 }
      function body(line, file, n,    trimmed) {
        trimmed = line
        sub(/^[[:space:]]+/, "", trimmed)
        if (trimmed == "" || trimmed ~ /^#/) return
        # Commands that mean "build/test/lint this repo" must be a gate. Matched
        # anywhere in the line, not just at the start: `nix develop --command cargo
        # build ...` is the same command wearing the shell as a prefix.
        if (trimmed ~ /(^|[ ;])cargo[ ]/ || trimmed ~ /tools\// || trimmed ~ /scripts\//) {
          if (trimmed !~ /tools\/ci\/ci\.sh/) { printf "%s:%d: %s\n", file, n, trimmed; bad = 1 }
        }
      }
    ' "$f" | grep .; then
      bad=1
    fi
  done
  if [ "$bad" -ne 0 ]; then
    cat >&2 <<'MSG'
error: the workflow lines above run cargo or a repository tool directly.
       Every check is a gate in tools/ci/ci.sh — add or reuse one and call
       `tools/ci/ci.sh gate <name>`. A command typed into YAML is a command
       that local CI, the other workflows and the release pipeline do not run.
MSG
    return 1
  fi
  echo "workflows route every command through tools/ci/ci.sh"

  # A job-level `if:` is evaluated before matrix expansion; GitHub rejects the
  # whole workflow with "Unrecognized named-value: 'matrix'". Step-level `if:`
  # (eight spaces) may use matrix.*, so only flag the four-space job scope.
  if grep -nE '^    if:.*\bmatrix\.' .github/workflows/*.yml .github/workflows/*.yaml 2>/dev/null; then
    cat >&2 <<'MSG'
error: a job-level `if:` references matrix.* — GitHub evaluates job-level
       conditions before the matrix exists and refuses the whole workflow.
       Move the condition into the matrix/matrix-generation step instead.
MSG
    return 1
  fi

  # Parity check: every pr/release gate must be scheduled in JOB_TABLE, every
  # job gate must exist in GATE_TABLE, and the job's shell/os/stage/setup must
  # be compatible with every gate it runs. The old check compared gate names
  # only, which let a `web` gate run in a `web-ci` job and a site-stage gate sit
  # in no matrix at all without a word.
  python3 tools/ci/ci-environment-tests.py
  python3 tools/ci/ci-registry-check.py "$GATE_TABLE" "$JOB_TABLE"
}

gate_eparts() {
  want_cargo
  cargo check -p eparts --all-targets
  cargo check -p eparts --features theme-json --all-targets
  cargo test -p eparts --lib
  cargo test -p eparts --lib --features theme-json
}

gate_audit() {
  want_nix_shell 'audit'
  command -v cargo-audit >/dev/null 2>&1 || cargo audit --version >/dev/null 2>&1 || {
    echo "error: cargo-audit not in this shell — it comes from nixpkgs in devShells.default" >&2
    return 1
  }
  python3 tools/ci/security-audit.py
}

gate_release_resolve() {
  local tag
  if [ "${GITHUB_EVENT_NAME:-}" = workflow_dispatch ]; then
    tag="${DISPATCHED:-}"
  elif [ "${GITHUB_EVENT_NAME:-}" = push ] && [ "${GITHUB_REF:-}" = "refs/tags/${TAGGED:-}" ]; then
    tag="${TAGGED:-}"
  else
    echo 'release resolution requires an explicit tag push or manual dispatch' >&2
    return 1
  fi
  local args=(resolve "$tag" --github-output "$GITHUB_OUTPUT")
  if [ "${GITHUB_EVENT_NAME:-}" = push ]; then
    args+=(--expected-commit "$GITHUB_SHA")
  fi
  python3 tools/release/release.py "${args[@]}"
  local publish=false
  if [ "${GITHUB_EVENT_NAME:-}" = push ] || [ "${PUBLISH_RELEASE:-false}" = true ]; then
    publish=true
  fi
  echo "publish=$publish" >> "$GITHUB_OUTPUT"
}

gate_release_verify() {
  python3 tools/release/release.py verify "$ANIMATIX_RELEASE_VERSION" \
    --expected-commit "$ANIMATIX_RELEASE_COMMIT" \
    --expected-tag-object "$ANIMATIX_RELEASE_TAG_OBJECT" --remote origin
}

gate_release_notes() {
  python3 tools/release/release.py notes "$ANIMATIX_RELEASE_VERSION" \
    --expected-commit "$ANIMATIX_RELEASE_COMMIT" \
    --expected-tag-object "$ANIMATIX_RELEASE_TAG_OBJECT" --output RELEASE_BODY.md
}

gate_release_publish() {
  python3 tools/release/release.py publish "$ANIMATIX_RELEASE_VERSION" \
    --expected-commit "$ANIMATIX_RELEASE_COMMIT" \
    --expected-tag-object "$ANIMATIX_RELEASE_TAG_OBJECT" --assets-dir out \
    --repository "$GITHUB_REPOSITORY"
}

# ── release-stage gates ───────────────────────────────────────────────────────
gate_site_artifact() {
  # Assemble only — pages.yml deploys this directory, and it owes nobody a zip.
  # Packaging the same tree into a release asset is dist-site.
  bash web/tools/site/site-artifact.sh _site
  bash tools/quality/size-report.sh --check --site _site
}

gate_dist_site() {
  bash tools/release/dist-package.sh site
}

gate_dist_web_sdk() {
  bash tools/release/dist-package.sh web-sdk
}

# The version a shipped binary reports is the only proof the artifact matches the
# tag it was built from. Without it, a stale target directory is indistinguishable
# from a fresh build at download time.
check_binary() {
  local bin="$1" want="$2" out
  [ -x "$bin" ] || { echo "error: $bin not produced by the build" >&2; return 1; }
  out="$("$bin" --version 2>&1)" || { echo "error: $bin --version failed" >&2; return 1; }
  echo "  $out"
  if [ -n "$want" ] && ! grep -q "$want" <<<"$out"; then
    echo "error: $bin reports '$out', expected to contain '$want' (the tag)" >&2
    return 1
  fi
}

gate_dist_linux() {
  # The Nix dist-driver provides orchestration tools only.
  # The fixed Ubuntu 22.04 image supplies the baseline runtime ABI and private
  # FFmpeg headers. Do not build portable artifacts inside the Nix dev shell.
  command -v docker >/dev/null || { echo 'error: Docker is required for Linux distribution' >&2; return 1; }
  bash tools/release/dist-linux.sh target/dist-linux-package
  bash tools/release/dist-package.sh linux target/dist-linux-package
  local label="${ANIMATIX_RELEASE_LABEL:-v$(workspace_version)}"
  local unpacked=target/dist-linux-acceptance
  rm -rf "$unpacked"
  mkdir -p "$unpacked"
  tar -xzf "dist/animatix-${label}-x86_64-linux-gnu-video.tar.gz" -C "$unpacked"
  bash tools/release/test-dist-linux.sh "$unpacked"
}

gate_dist_macos() {
  want_cargo
  # No `video` here on purpose: a macOS FFmpeg would be whatever major version
  # brew happens to ship, which is not ours to pin. The GUI degrades to a
  # "requires the 'video' feature (FFmpeg)" message instead of shipping a binary
  # linked against an unpinned library. This is the feature set the pr-stage
  # `no-video` gate actually tests.
  local want="${ANIMATIX_RELEASE_VERSION:-}"
  want="${want#v}"
  cargo build --profile dist -p animatix-cli -p animatix-gui
  check_binary target/dist/animatix "$want"
  check_binary target/dist/animatix-gui "$want"
  bash tools/release/dist-package.sh macos target/dist
}

gate_dist_windows() {
  want_cargo
  local want="${ANIMATIX_RELEASE_VERSION:-}"
  want="${want#v}"
  cargo build --profile dist -p animatix-cli -p animatix-gui
  check_binary target/dist/animatix.exe "$want"
  check_binary target/dist/animatix-gui.exe "$want"
  bash tools/release/dist-package.sh windows target/dist
}

# ── setup ─────────────────────────────────────────────────────────────────────
# Provisioning the runner needs that is *not* a toolchain. The toolchain hints
# (rust-stable / rust-nightly / node / nix) are installed by GitHub Actions and
# verified here, because a script that installs a toolchain inside a job that
# already picked one is how two toolchains end up on PATH.
setup_one() {
  case "$1" in
    ffmpeg|gpu)
      echo "setup $1: provided by the nix dev shell; nothing to install here"
      ;;
    wasm-bindgen)
      # The CLI must equal the crate's `=0.2.128` pin exactly; a mismatch fails
      # at glue generation, several build minutes in. Version read from the pin.
      # .#web-build already carries the verified pinned CLI, so this is a no-op
      # inside that shell; the non-Nix fallback downloads and checks the tarball.
      # shellcheck source=web/tools/build/wasm-tools.sh
      . web/tools/build/wasm-tools.sh
      local want
      want="$(wbg_pin)"
      if command -v wasm-bindgen >/dev/null 2>&1 && \
         [ "$(wasm-bindgen --version | awk '{print $NF}')" = "$want" ]; then
        echo "setup wasm-bindgen: $(wasm-bindgen --version) already on PATH"
      else
        install_wbg
      fi
      ;;
    rust-stable|rust-nightly|node|nix|none)
      echo "setup $1: provided by the workflow's own steps; nothing to install here"
      ;;
    *)
      echo "error: unknown setup token: $1" >&2
      return 1
      ;;
  esac
}

# ── dispatch ──────────────────────────────────────────────────────────────────
run_gate() {
  local name="$1" fn
  fn="gate_$(tr '-' '_' <<<"$name")"
  if ! declare -F "$fn" >/dev/null; then
    echo "error: no gate named '$name'" >&2
    echo "known gates:" >&2
    gate_names >&2
    return 2
  fi
  # The measured child has its own errexit context. `if "$fn"` or `"$fn" ||`
  # here would disable errexit throughout a gate and could turn failures green.
  if [ "${ANIMATIX_GATE_CHILD:-}" != 1 ]; then
    python3 tools/ci/ci-control.py time --gate "$name" --shell "$(gate_shell "$name")" -- \
      env ANIMATIX_GATE_CHILD=1 bash "$root/tools/ci/ci.sh" gate "$name"
    return
  fi
  printf '\n\033[1m── gate: %s (shell=%s)\033[0m\n' "$name" "$(gate_shell "$name")"
  local required
  required="$(gate_shell "$name")"
  if [ "$required" != native ] && [ "$required" != none ]; then
    want_nix_shell "$name"
    if [ "${ANIMATIX_CI_SHELL:-}" != "$required" ]; then
      echo "error: $name requires shell=$required; got ${ANIMATIX_CI_SHELL:-unmarked}. Start from a clean shell: $(shell_for "$required") tools/ci/ci.sh gate $name" >&2
      return 1
    fi
  fi
  "$fn"
}

# This is a same-invocation union, not a registered gate or reusable cache.
# Scene audit already checks the default budget; its JSON validation adds the
# remaining contract. Keep both independent gates available for all other calls.
run_scene_budget_pair() {
  if [ "${ANIMATIX_GATE_CHILD:-}" != 1 ]; then
    python3 tools/ci/ci-control.py time --gate scene-budget-audit --shell nix -- \
      env ANIMATIX_GATE_CHILD=1 bash "$root/tools/ci/ci.sh" gate scene-audit budget-audit
    return
  fi
  want_nix_shell scene-audit
  want_nix_shell budget-audit
  if [ "${ANIMATIX_CI_SHELL:-}" != nix ]; then
    echo "error: scene-audit and budget-audit require shell=nix" >&2
    return 1
  fi
  printf '\n── gates: scene-audit + budget-audit (shared execution)\n'
  gate_scene_audit joint
  printf 'OK gate: scene-audit\nOK gate: budget-audit\n'
}

run_explicit_gates() {
  local arg g i=0
  local -a requested=()
  for arg in "$@"; do
    for g in $arg; do requested+=("$g"); done
  done
  while [ "$i" -lt "${#requested[@]}" ]; do
    if [ "${requested[$i]}" = scene-audit ] && \
       [ "${requested[$((i + 1))]:-}" = budget-audit ] && \
       [ -z "${ANIMATIX_BIN:-}" ] && [ -z "${ANIMATIX_AUDIT_BUDGET:-}" ]; then
      run_scene_budget_pair
      i=$((i + 2))
    else
      run_gate "${requested[$i]}"
      i=$((i + 1))
    fi
  done
}

gate_names() {
  awk -F'|' -v s="${1:-}" 'NF>1 && $1!="" && (s=="" || s=="all" || $5==s) { if (!seen[$1]++) print $1 }' <<<"$GATE_TABLE"
}

rows() {
  awk -F'|' -v s="${1:-}" 'NF>1 && $1!="" && (s=="" || s=="all" || $5==s) { printf "%s\t%s\t%s\t%s\t%s\n", $1, $2, $3, $4, $5 }' <<<"$GATE_TABLE"
}

job_rows() {
  awk -F'|' -v s="${1:-}" 'NF>1 && $1!="" && ((s=="" || s=="all") ? ($6=="pr" || $6=="release") : $6==s) { printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\n", $1, $2, $3, $4, $5, $6, $7 }' <<<"$JOB_TABLE"
}

stage="${2:-}"
case "${1:-}" in
  plan)
    shift
    python3 tools/ci/ci-plan.py --repo "$root" "$@"
    ;;
  --list)
    rows "$stage"
    ;;
  --matrix)
    case "$stage" in
      ""|all|pr|release) ;;
      *)
        echo "error: --matrix supports pr|release only; '$stage' gates are composite (run by name)" >&2
        exit 2
        ;;
    esac
    # Retain the manual --skip-web compatibility filter. Hosted CI selection
    # uses the dependency planner and never puts matrix context in a job-level if.
    if [ "${3:-}" = "--skip-web" ]; then
      job_rows "$stage" | grep -v '^Web Build'$'\t' || true
    else
      job_rows "$stage"
    fi | python3 -c '
import json, sys
rows = [l.rstrip("\n").split("\t") for l in sys.stdin if l.strip()]
print(json.dumps({"include": [
    {"name": "%s (%s)" % (n, o) if not n.endswith(")") else n,
     "gate": g, "shell": s, "os": o, "setup": d, "stage": st, "id": ident}
    for n, g, s, o, d, st, ident in rows]}))
'
    ;;
  gates)
    gate_names "$stage" | while read -r g; do
      printf '%-14s %-8s shell=%-7s setup=%s\n' "$g" "$(gate_stage "$g")" "$(gate_shell "$g")" "$(gate_setup "$g")"
    done
    ;;
  shell) shift; shell_for "$(gate_shell "${1:?usage: ci.sh shell <gate>}")"
    ;;
  gate)
    shift
    [ $# -ge 1 ] || { echo "usage: ci.sh gate <name> [<name>…]" >&2; exit 2; }
    run_explicit_gates "$@"
    ;;
  setup)
    shift
    [ $# -ge 1 ] || { echo "usage: ci.sh setup <token>…" >&2; exit 2; }
    for arg in "$@"; do
      for t in $arg; do setup_one "$t"; done
    done
    ;;
  all)
    if [ -z "$stage" ] || [ "$stage" = all ]; then
      { gate_names pr; gate_names release; } | awk '!seen[$0]++' |
        while read -r g; do run_gate "$g"; done
    elif [ "$stage" = control ]; then
      echo "error: control gates require an explicit gate name" >&2
      exit 2
    elif [ "$stage" = pr ] || [ "$stage" = release ] || [ "$stage" = site ]; then
      gate_names "$stage" | while read -r g; do run_gate "$g"; done
    else
      echo "error: unknown stage: $stage" >&2
      exit 2
    fi
    ;;
  ""|-h|--help)
    sed -n '1,/^set -euo pipefail/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'
    ;;
  *)
    echo "error: unknown command: $1" >&2
    echo "usage: ci.sh [--list|--matrix|gates|shell <gate>|gate <name>…|setup <token>…|all] [pr|release|all]" >&2
    exit 2
    ;;
esac
