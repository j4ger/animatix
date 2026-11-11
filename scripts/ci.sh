#!/usr/bin/env bash
# The one definition of "is this green".
#
# Before this file, the answer lived in three GitHub workflows that could not see
# each other: `ci.yml` ran ten jobs on PR but never built wasm, never ran
# `site_scenes` and never checked the embed bundle; `pages.yml` did all three, but
# only on push to main — so the gate that catches a broken `web/embed/amx-player.js`
# fired *after* the merge it was supposed to protect. The same commands were typed
# out 3-4 ways (apt lines, the wasm-bindgen pin, the `.amx` walk), and a fix to one
# copy left the others behind.
#
# Every check is a named *gate* here. `ci.yml`, `pages.yml`, `release.yml` and
# humans all run the same gate names, and the CI matrix is *generated from this
# file* (`--list`), so a gate cannot exist in CI and not locally, or the reverse.
#
# The `shell=` column is the load-bearing part. Animatix needs four different
# environments and the wrong one fails in a way that looks like a code bug:
#
#   native  plain runner + rustup toolchain. Stays native for checks that need
#           no system libraries (fmt, meta-version, eparts on mac/win).
#   nix     `nix develop` — provides ALSA + FFmpeg headers + Vulkan/Mesa + cog + cargo-audit.
#   web     `nix develop .#web-build` — wasm32 std + the pinned wasm-bindgen CLI.
#   web-ci  `nix develop .#web-ci` — web-build plus Chromium/node/python for web-smoke.
#   none    needs no Rust toolchain at all (content-sync, workflows).
#
# `ci.sh` deliberately never enters a shell itself. DevShells *prepend* to PATH
# rather than replacing it, so entering one from inside another (or from a
# direnv-loaded login shell) keeps the OUTER rustc first — `rustc --print sysroot`
# then reports the toolchain with no wasm32 std and the build fails exactly as
# before, while wasm-opt and brotli resolve correctly, which makes the shell look
# like it loaded fine. The caller decides the shell; `gate` asserts the one it got.
#
# Usage
#   scripts/ci.sh --list                 name<TAB>shell<TAB>os<TAB>setup<TAB>stage, one row per gate
#   scripts/ci.sh --matrix               the same as JSON, for the CI matrix
#   scripts/ci.sh gates                  human-readable listing
#   scripts/ci.sh gate <name> [<name>…]  run gates in this shell
#   scripts/ci.sh setup <token>…         provision what the tokens mean here
#   scripts/ci.sh shell <name>           print the shell command this gate expects
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# ── the registry ──────────────────────────────────────────────────────────────
# name | shell | os | setup tokens | stage
#
# Three stages, because a release owes a different set of things than a pull
# request:
#   pr        every pull request runs it, and the release pipeline runs it first —
#             an artifact nobody gated is an artifact that ships broken.
#   release   build-only: the distribution jobs that produce the binaries.
#   site      composite gates: site-artifact/dist-site/dist-wasm need the
#             gitignored web/pkg bundles in the same job that built them, so
#             they are run by name from pages.yml and release.yml and are not
#             matrix rows. `--matrix` accepts pr/release only.
#
# Adding a gate here is the whole job: `--list` feeds the matrix, so CI picks it
# up. Nothing in .github/ has to learn its name.
GATE_TABLE='
fmt|native|ubuntu-latest|rust-stable|pr
lint|nix|ubuntu-latest|nix|pr
check|nix|ubuntu-latest|nix|pr
test|nix|ubuntu-latest|nix|pr
no-video|nix|ubuntu-latest|nix|pr
render-smoke|nix|ubuntu-latest|nix|pr
examples|nix|ubuntu-latest|nix|pr
ext-bench|nix|ubuntu-latest|nix|pr
doc|nix|ubuntu-latest|nix|pr
site-scenes|nix|ubuntu-latest|nix|pr
embed-build|native|ubuntu-latest|node|pr
content-sync|native|ubuntu-latest|none|pr
structure|native|ubuntu-latest|none|pr
meta-version|native|ubuntu-latest|rust-stable|pr
workflows|native|ubuntu-latest|none|pr
eparts|native|ubuntu-latest|rust-stable|pr
eparts|native|macos-latest|rust-stable|pr
eparts|native|windows-latest|rust-stable|pr
audit|nix|ubuntu-latest|nix|pr
scene-audit|nix|ubuntu-latest|nix|pr
budget-audit|nix|ubuntu-latest|nix|pr
commit|nix|ubuntu-latest|nix|pr
wasm-check|web|ubuntu-latest|nix|pr
web-build|web-ci|ubuntu-latest|nix|pr
web-smoke|web-ci|ubuntu-latest|nix|pr
# Composite site-stage gates — never matrix jobs (see the stage note above).
site-artifact|native|ubuntu-latest|none|site
dist-site|native|ubuntu-latest|none|site
dist-wasm|native|ubuntu-latest|none|site
dist-linux|nix|ubuntu-latest|nix|release
dist-macos|native|macos-latest|rust-stable|release
dist-windows|native|windows-latest|rust-stable|release
'

# ── CI matrix jobs ────────────────────────────────────────────────────────────
# name | gates | shell | os | setup tokens | stage
#
# Consolidated jobs for CI. Rather than spawning a separate VM for each
# individual gate (which caused 21 runners to hammer GitHub API and Nix caches,
# taking minutes to spin up each for 3-second checks), related checks run in
# the same job and reuse compiler/toolchain artifacts.
#
# Every gate listed here is defined above in GATE_TABLE and can still be run
# individually via `scripts/ci.sh gate <name>`. The `workflows` gate verifies
# each job's shell/os/stage/setup against every gate it lists, so a job cannot
# silently run a gate in the wrong environment.
JOB_TABLE='
Fast Checks|fmt structure workflows content-sync meta-version embed-build|native|ubuntu-latest|rust-stable node|pr
eparts|eparts|native|ubuntu-latest|rust-stable|pr
eparts|eparts|native|macos-latest|rust-stable|pr
eparts|eparts|native|windows-latest|rust-stable|pr
Check & Lint|commit audit scene-audit budget-audit check lint doc no-video|nix|ubuntu-latest|nix|pr
Test & Examples|test examples|nix|ubuntu-latest|nix|pr
Render Smoke & Scenes|render-smoke site-scenes|nix|ubuntu-latest|nix|pr
Benchmarks|ext-bench|nix|ubuntu-latest|nix|pr
Web Check|wasm-check|web|ubuntu-latest|nix|pr
Web Build|web-build web-smoke|web-ci|ubuntu-latest|nix|pr
dist-linux|dist-linux|nix|ubuntu-latest|nix|release
dist-macos|dist-macos|native|macos-latest|rust-stable|release
dist-windows|dist-windows|native|windows-latest|rust-stable|release
'

# The commands each gate expects to be run by. Kept as prose so `ci.sh shell`
# can print them and a human can copy one.
shell_for() {
  case "$1" in
    native) echo '' ;;
    nix) echo 'nix develop --command' ;;
    web) echo 'nix develop .#web-build --command' ;;
    web-ci) echo 'nix develop .#web-ci --command' ;;
    none) echo '' ;;
    *) echo "unknown shell: $1" >&2; return 1 ;;
  esac
}

gate_shell() { awk -F'|' -v g="$1" '$1==g {print $2; exit}' <<<"$GATE_TABLE"; }
gate_os() { awk -F'|' -v g="$1" '$1==g {print $3; exit}' <<<"$GATE_TABLE"; }
gate_setup() { awk -F'|' -v g="$1" '$1==g {print $4; exit}' <<<"$GATE_TABLE"; }
gate_stage() { awk -F'|' -v g="$1" '$1==g {print $5; exit}' <<<"$GATE_TABLE"; }

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
    echo "       Run it as: $(shell_for web-ci) scripts/ci.sh gate web-smoke" >&2
    return 1
  }
}

want_nix_shell() {
  [ -n "${IN_NIX_SHELL:-}" ] || {
    echo "error: this gate is shell=$(gate_shell "$1") — run it as:" >&2
    echo "       $(shell_for "$(gate_shell "$1")") scripts/ci.sh gate $1" >&2
    return 1
  }
}

want_wasm_std() {
  if ! ls "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" >/dev/null 2>&1; then
    echo "error: this rustc has no wasm32-unknown-unknown std (sysroot: $(rustc --print sysroot))." >&2
    echo "       That is the shell, not the code: the target may be installed under" >&2
    echo "       ~/.rustup while this toolchain cannot see it. Run it as:" >&2
    echo "       $(shell_for "$(gate_shell "$1")") scripts/ci.sh gate $1" >&2
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
  # Render tests need an adapter; without this they skip silently and report green
  # while exercising no pixels (see `animatix_render::testing`).
  # Serialize GPU-backed tests: headless Vello/WGPU teardown can SIGSEGV when
  # renderer tests run concurrently on one adapter.
  ANIMATIX_REQUIRE_GPU=1 cargo test --workspace -- --test-threads=1
}

gate_no_video() {
  want_cargo
  # The `video` feature must stay truly opt-in. This gate runs in the nix dev
  # shell, which ships FFmpeg, so "it built without FFmpeg present" cannot be
  # demonstrated here — the old comment claimed exactly that and was false.
  # Build the default user-facing binaries, run the engine with every optional
  # feature off, and inspect the resolved feature graph: rsmpeg is pulled in
  # only by animatix-render/video, so its presence means a default build turned
  # video on.
  cargo build -p animatix-cli -p animatix-gui
  cargo check -p animatix --no-default-features
  local pkg tree
  for pkg in animatix-cli animatix-gui; do
    if ! tree="$(cargo tree -e features -p "$pkg" 2>/dev/null)"; then
      echo "error: cargo tree failed for $pkg; cannot verify the video feature is off" >&2
      return 1
    fi
    if grep -q 'rsmpeg' <<<"$tree"; then
      echo "error: $pkg default feature resolution enables rsmpeg (the video feature)" >&2
      return 1
    fi
  done
  echo "no-video: default CLI/GUI feature resolution excludes rsmpeg (video)"
}

gate_scene_audit() {
  want_nix_shell 'scene-audit'
  cargo build --bin animatix
  
  local failed=0
  # The scene walk shared with scripts/audit-budget.sh: component libraries,
  # site copies, the plugin showcase and the pure-`pub let` module are not
  # standalone scenes; a multi-file project is driven through its main.amx.
  scenes() {
    find examples web/scenes web/demos -name '*.amx' \
      -not -path '*/lib/*' \
      -not -path '*/_site/*' \
      -not -path '*/plugin_pulse.amx' \
      -not -path '*/epicycles/fourier.amx' |
      sort |
      while IFS= read -r amx; do
        local dir base
        dir=$(dirname "$amx")
        base=$(basename "$amx")
        if [ -f "$dir/main.amx" ] && [ "$base" != "main.amx" ]; then
          continue
        fi
        printf '%s\n' "$amx"
      done
  }

  while IFS= read -r f; do
    [ -z "$f" ] && continue

    local budget_arg="--budget .animatix/audit-budget.json --render never"

    local output=$(mktemp)
    if ! ./target/debug/animatix audit "$f" $budget_arg --format json > "$output" 2>/dev/null; then
      echo "FAIL scene-audit (exit code): $f" >&2
      failed=1
      rm -f "$output"
      continue
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
      failed=1
    fi
    rm -f "$output"
  done < <(scenes)
  return "$failed"
}

gate_budget_audit() {
  want_nix_shell 'budget-audit'
  cargo build --bin animatix
  bash scripts/audit-budget.sh --check
}

gate_render_smoke() {
  want_cargo
  cargo build --bin animatix
  ANIMATIX_REQUIRE_GPU=1 bash scripts/render_smoke.sh
}

gate_examples() {
  want_cargo
  cargo build --bin animatix
  bash scripts/check_examples.sh
}

gate_ext_bench() {
  want_cargo
  bash scripts/extension-bench.sh --quick --max-plan-ns 10000
}

gate_doc() {
  want_cargo
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  bash scripts/check-docs.sh
}

# The four gates PR CI was missing. Each one exists because a real defect walked
# past the old ten jobs.
gate_site_scenes() {
  want_cargo
  # A scene only the full engine can build, or an import/asset URL that resolves
  # to nothing next to its page, shows up as a "Scene error" veil in the browser
  # and is invisible to the engine's own suite. `--slim` is exactly
  # `--no-default-features` (see scripts/build-web.sh), so these two runs cover
  # both shipped bundles.
  cargo test -p animatix-web --test site_scenes
  cargo test -p animatix-web --test site_scenes --no-default-features
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

gate_web_build() {
  want_cargo; want_wasm_std 'web-build'; want_nix_shell 'web-build'
  # shellcheck source=scripts/wasm-tools.sh
  . scripts/wasm-tools.sh
  check_wbg
  # A release artifact without wasm-opt/brotli is several times larger; fail
  # here instead of publishing it (the tools are in .#web-build/.#web-ci).
  ANIMATIX_REQUIRE_WASM_TOOLS=1 bash scripts/build-web.sh --slim
  ANIMATIX_REQUIRE_WASM_TOOLS=1 bash scripts/build-web.sh --assets
  ANIMATIX_REQUIRE_WASM_TOOLS=1 bash scripts/build-web.sh
  # The three bundles are what the pages load; a build that "succeeds" without
  # writing all of them is how a flag typo used to reach the site.
  for d in web/pkg-slim web/pkg-assets web/pkg; do
    [ -f "$d/animatix_web_bg.wasm" ] || { echo "error: $d/animatix_web_bg.wasm missing after build" >&2; return 1; }
    [ -f "$d/animatix_web.js" ] || { echo "error: $d/animatix_web.js missing after build" >&2; return 1; }
    [ -f "$d/.build" ] || { echo "error: $d/.build stamp missing after build" >&2; return 1; }
    [ ! -f "$d/.dev" ] || { echo "error: $d is a --dev bundle after a release build" >&2; return 1; }
  done
  # web-smoke loads web/embed/amx-player.js. It is gitignored, so a fresh CI
  # checkout does not have it and artifacts do not cross jobs; build it in this
  # job before the smoke gate serves the site.
  gate_embed_build
  # The size budget belongs to the build, not only to the Pages assembler: a
  # regression that only the deploy gate catches has already reached main.
  # `_site` and the font pack may not exist yet; size-report skips missing paths.
  bash scripts/size-report.sh --check
}

gate_web_smoke() {
  want_nix_shell 'web-smoke'
  want_node
  want_chromium
  # Loading the real pages is the only thing that exercises the embed's module
  # wiring and the wasm glue together. The static gates all passed while every
  # player on the site was throwing `ReferenceError: qualityStep is not defined`.
  #
  # The browser smoke is the one gate that depends on a real Chromium process
  # and a local HTTP server; a cold or overloaded runner occasionally reaches
  # the player before its scene fetch has completed. One restart catches that
  # startup race without weakening the checks. The per-player timeout is already
  # generous (full-engine wasm can take minutes on SwiftShader), so repeating
  # the whole suite a third time only multiplies the runtime of a real failure.
  local attempt
  for attempt in 1 2; do
    if node scripts/web-smoke.mjs; then
      return 0
    fi
    echo "web-smoke: attempt $attempt failed; retrying after browser startup" >&2
    sleep 2
  done
  return 1
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
  bash scripts/content-sync.sh
}

gate_structure() {
  # Direct ceilings on file and function length (docs/ecs.md §8).  The
  # allowlists freeze existing offenders at their current size; splitting a
  # function or file deletes its row.
  bash scripts/structure.sh
}

gate_meta_version() {
  want_cargo
  # Version single-source: every workspace member must inherit [workspace.package],
  # so `scripts/bump-version.sh` (cog's pre_bump_hook) has exactly one place to
  # write and a hand-edited member cannot hide.
  python3 - "$root" <<'PY'
import json, re, subprocess, sys
root = sys.argv[1]
meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"],
    cwd=root, check=True, capture_output=True).stdout)
text = open(meta["workspace_root"] + "/Cargo.toml").read()
m = re.search(r'^\[workspace\.package\][^\[]*?^version = "([^"]+)"', text, re.M | re.S)
if not m:
    print("error: no [workspace.package] version in the root Cargo.toml", file=sys.stderr)
    sys.exit(1)
want = m.group(1)
bad = [(p["name"], p["version"]) for p in meta["packages"] if p["version"] != want]
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
        if (trimmed ~ /(^|[ ;])cargo[ ]/ || trimmed ~ /scripts\//) {
          if (trimmed !~ /ci\.sh/) { printf "%s:%d: %s\n", file, n, trimmed; bad = 1 }
        }
      }
    ' "$f" | grep .; then
      bad=1
    fi
  done
  if [ "$bad" -ne 0 ]; then
    cat >&2 <<'MSG'
error: the workflow lines above run cargo or a scripts/ file directly.
       Every check is a gate in scripts/ci.sh — add or reuse one and call
       `scripts/ci.sh gate <name>`. A command typed into YAML is a command
       that local CI, the other workflows and the release pipeline do not run.
MSG
    return 1
  fi
  echo "workflows route every command through scripts/ci.sh"

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
  python3 - "$GATE_TABLE" "$JOB_TABLE" <<'PY' || return 1
import sys

ALLOWED_SHELLS = {"native", "nix", "web", "web-ci", "none"}
ALLOWED_STAGES = {"pr", "release", "site"}

def parse_rows(text, ncols, label):
    rows = []
    for lineno, line in enumerate(text.strip().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = [p.strip() for p in line.split("|")]
        if len(parts) != ncols:
            print(f"error: {label} row {lineno} has {len(parts)} columns, expected {ncols}: {line}",
                  file=sys.stderr)
            sys.exit(1)
        rows.append(parts)
    return rows

def setup_tokens(text):
    # `none` means "no setup", not a token the job must also list.
    tokens = set(text.split())
    tokens.discard("none")
    return tokens

gates = []
for name, shell, os_name, setup, stage in parse_rows(sys.argv[1], 5, "GATE_TABLE"):
    if shell not in ALLOWED_SHELLS:
        print(f"error: gate '{name}' has unknown shell '{shell}'", file=sys.stderr)
        sys.exit(1)
    if stage not in ALLOWED_STAGES:
        print(f"error: gate '{name}' has unknown stage '{stage}'", file=sys.stderr)
        sys.exit(1)
    gates.append({
        "name": name, "shell": shell, "os": os_name, "stage": stage,
        "setup": setup_tokens(setup),
    })

jobs = []
for name, gate_list, shell, os_name, setup, stage in parse_rows(sys.argv[2], 6, "JOB_TABLE"):
    if shell not in ALLOWED_SHELLS:
        print(f"error: job '{name}' has unknown shell '{shell}'", file=sys.stderr)
        sys.exit(1)
    if stage not in {"pr", "release"}:
        print(f"error: job '{name}' has stage '{stage}'; JOB_TABLE jobs are pr/release only",
              file=sys.stderr)
        sys.exit(1)
    jobs.append({
        "name": name, "gates": gate_list.split(), "shell": shell, "os": os_name,
        "stage": stage, "setup": setup_tokens(setup),
    })

seen = set()
for gate in gates:
    key = (gate["name"], gate["shell"], gate["os"], gate["stage"], tuple(sorted(gate["setup"])))
    if key in seen:
        print(f"error: duplicate GATE_TABLE row for {gate['name']} on {gate['os']}", file=sys.stderr)
        sys.exit(1)
    seen.add(key)

def compatible(gate, job):
    return (gate["shell"] == job["shell"] and gate["os"] == job["os"]
            and gate["stage"] == job["stage"]
            and gate["setup"].issubset(job["setup"]))

bad = False
for job in jobs:
    for gate_name in job["gates"]:
        candidates = [g for g in gates if g["name"] == gate_name]
        if not candidates:
            print(f"error: job '{job['name']}' schedules unknown gate '{gate_name}'", file=sys.stderr)
            bad = True
            continue
        if not any(compatible(g, job) for g in candidates):
            print(f"error: job '{job['name']}' ({job['shell']}/{job['os']}/{job['stage']}, "
                  f"setup={sorted(job['setup'])}) is incompatible with gate '{gate_name}'", file=sys.stderr)
            for g in candidates:
                print(f"  gate row: {g['shell']}/{g['os']}/{g['stage']} setup={sorted(g['setup'])}",
                      file=sys.stderr)
            bad = True

for gate in gates:
    if gate["stage"] == "site":
        if any(gate["name"] in job["gates"] for job in jobs):
            print(f"error: site-stage composite gate '{gate['name']}' must not be a JOB_TABLE job",
                  file=sys.stderr)
            bad = True
        continue
    if not any(gate["name"] in job["gates"] and compatible(gate, job) for job in jobs):
        print(f"error: gate '{gate['name']}' ({gate['shell']}/{gate['os']}/{gate['stage']}) "
              f"has no compatible JOB_TABLE row", file=sys.stderr)
        bad = True

if bad:
    sys.exit(1)
site_gates = sorted({g["name"] for g in gates if g["stage"] == "site"})
print("GATE_TABLE and JOB_TABLE parity verified (shell/os/setup/stage)")
if site_gates:
    print("site-stage composite gates (run by name, not matrix jobs): " + ", ".join(site_gates))
PY
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
  # Every swallowed advisory is listed in scripts/audit-allow.txt with the reason
  # it cannot be lifted here, and is printed on every run. An unannotated line is
  # an error: the file is a debt register, not a mute button.
  local ignores=() line id
  if [ -f scripts/audit-allow.txt ]; then
    while IFS= read -r line; do
      case "$line" in ''|'#'*) continue ;; esac
      id="${line%%$'\t'*}"
      if [ "$id" = "$line" ]; then
        echo "error: scripts/audit-allow.txt line for $id has no TAB-separated reason" >&2
        return 1
      fi
      ignores+=(--ignore "$id")
    done < scripts/audit-allow.txt
  fi
  if [ "${#ignores[@]}" -gt 0 ]; then
    echo "allowing ${#ignores[@]} advisory(ies) — reasons from scripts/audit-allow.txt:"
    grep -vE '^#|^$' scripts/audit-allow.txt | while IFS=$'\t' read -r a b; do printf '  %s  %s\n' "$a" "$b"; done
  fi
  cargo audit "${ignores[@]}"
}

gate_commit() {
  want_nix_shell 'commit'
  # `cog check` with no range verifies history from the latest tag, and this repo
  # has no semver tag — so it walks from the beginning and fails on `Initial
  # commit`, which predates the convention and always will. The range has to be
  # anchored, in this order: what the event says changed, then the latest version
  # tag once one exists, then how far HEAD is ahead of the default branch.
  local base=""
  if [ -n "${ANIMATIX_COMMIT_BASE:-}" ] &&
     git rev-parse --verify -q "${ANIMATIX_COMMIT_BASE}^{commit}" >/dev/null &&
     git merge-base --is-ancestor "$ANIMATIX_COMMIT_BASE" HEAD 2>/dev/null; then
    # An arbitrary SHA from the event payload is only usable after both checks:
    # it must name a commit, and it must be an ancestor of what we are testing.
    base="$ANIMATIX_COMMIT_BASE"
  elif git tag --list 'v*' | grep -q .; then
    cog check --from-latest-tag
    return
  else
    base="$(git merge-base HEAD origin/main 2>/dev/null || true)"
    [ "$base" = "$(git rev-parse HEAD)" ] && base=""
  fi

  if [ -z "$base" ]; then
    echo "note: nothing to check — no version tag yet, and HEAD is not ahead of a"
    echo "      known base. The first \`cog bump\` creates v0.1.0 and this gate then"
    echo "      checks every commit since it."
    return 0
  fi
  echo "checking commits: $base..HEAD"
  cog check "$base..HEAD"
}

# ── release-stage gates ───────────────────────────────────────────────────────
gate_site_artifact() {
  # Assemble only — pages.yml deploys this directory, and it owes nobody a zip.
  # Packaging the same tree into a release asset is dist-site.
  bash scripts/site-artifact.sh _site
  bash scripts/size-report.sh --check --site _site
}

gate_dist_site() {
  bash scripts/dist-package.sh site
}

gate_dist_wasm() {
  bash scripts/dist-package.sh wasm
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
  want_nix_shell 'dist-linux'
  local want="${ANIMATIX_RELEASE_VERSION:-}"
  want="${want#v}"
  # FFmpeg has to come from the dev shell: the pinned rsmpeg bindings regenerate
  # against the headers nix provides (AGENTS.md, "Video Export"). Outside it,
  # pkg-config finds no FFmpeg and a stale prebuilt binding.rs is used instead.
  # A separate --target-dir because the `video` dependency graph rebuilds
  # rsmpeg/rusty_ffmpeg; sharing target/release with the non-video gates makes
  # each one clobber the other's build. `dist` applies fat LTO, panic=abort and
  # symbol stripping; the explicit strip below is only a portable guard.
  cargo build --profile dist --target-dir target-video-dist -p animatix-cli -p animatix-gui --features video
  strip target-video-dist/dist/animatix target-video-dist/dist/animatix-gui
  check_binary target-video-dist/dist/animatix "$want"
  check_binary target-video-dist/dist/animatix-gui "$want"
  bash scripts/dist-package.sh linux target-video-dist/dist
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
  bash scripts/dist-package.sh macos target/dist
}

gate_dist_windows() {
  want_cargo
  local want="${ANIMATIX_RELEASE_VERSION:-}"
  want="${want#v}"
  cargo build --profile dist -p animatix-cli -p animatix-gui
  check_binary target/dist/animatix.exe "$want"
  check_binary target/dist/animatix-gui.exe "$want"
  bash scripts/dist-package.sh windows target/dist
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
      # shellcheck source=scripts/wasm-tools.sh
      . scripts/wasm-tools.sh
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
  printf '\n\033[1m── gate: %s (shell=%s)\033[0m\n' "$name" "$(gate_shell "$name")"
  "$fn"
}

gate_names() {
  awk -F'|' -v s="${1:-}" 'NF>1 && $1!="" && (s=="" || s=="all" || $5==s) { if (!seen[$1]++) print $1 }' <<<"$GATE_TABLE"
}

rows() {
  awk -F'|' -v s="${1:-}" 'NF>1 && $1!="" && (s=="" || s=="all" || $5==s) { printf "%s\t%s\t%s\t%s\t%s\n", $1, $2, $3, $4, $5 }' <<<"$GATE_TABLE"
}

job_rows() {
  awk -F'|' -v s="${1:-}" 'NF>1 && $1!="" && (s=="" || s=="all" || $6==s) { printf "%s\t%s\t%s\t%s\t%s\t%s\n", $1, $2, $3, $4, $5, $6 }' <<<"$JOB_TABLE"
}

stage="${2:-}"
case "${1:-}" in
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
    # The web build is path-filtered in ci.yml. Filter the generated matrix
    # here rather than putting `matrix` in a job-level `if`: that context is
    # not available at job scope and makes the whole workflow invalid.
    if [ "${3:-}" = "--skip-web" ]; then
      job_rows "$stage" | grep -v '^Web Build'$'\t' || true
    else
      job_rows "$stage"
    fi | python3 -c '
import json, sys
rows = [l.rstrip("\n").split("\t") for l in sys.stdin if l.strip()]
print(json.dumps({"include": [
    {"name": "%s (%s)" % (n, o) if not n.endswith(")") else n,
     "gate": g, "shell": s, "os": o, "setup": d, "stage": st}
    for n, g, s, o, d, st in rows]}))
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
    for arg in "$@"; do
      for g in $arg; do run_gate "$g"; done
    done
    ;;
  setup)
    shift
    [ $# -ge 1 ] || { echo "usage: ci.sh setup <token>…" >&2; exit 2; }
    for arg in "$@"; do
      for t in $arg; do setup_one "$t"; done
    done
    ;;
  all)
    gate_names "$stage" | while read -r g; do run_gate "$g"; done
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
