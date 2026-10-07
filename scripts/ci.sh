#!/usr/bin/env bash
# The one definition of "is this green".
#
# Before this file, the answer lived in three GitHub workflows that could not see
# each other: `ci.yml` ran ten jobs on PR but never built wasm, never ran
# `site_scenes` and never checked the embed bundle; `pages.yml` did all three, but
# only on push to main — so the gate that catches a stale `web/embed/amx-player.js`
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
#   native  plain runner + rustup toolchain. Everything that is pure Rust.
#           Stays native on purpose: `Swatinem/rust-cache` only pays off outside
#           nix, and `flake.checks` would route every gate through a store fetch.
#   nix     `nix develop` — provides ALSA + FFmpeg headers + cog + cargo-audit.
#   web     `nix develop .#web-build` — the only shell with a wasm32 std.
#   none    needs no Rust toolchain at all.
#
# `ci.sh` deliberately never enters a shell itself. DevShells *prepend* to PATH
# rather than replacing it, so entering one from inside another (or from a
# direnv-loaded login shell) keeps the OUTER rustc first — `rustc --print sysroot`
# then reports the toolchain with no wasm32 std and the build fails exactly as
# before, while wasm-opt and brotli resolve correctly, which makes the shell look
# like it loaded fine. The caller decides the shell; `gate` asserts the one it got.
#
# Usage
#   scripts/ci.sh --list                 name<TAB>shell<TAB>os<TAB>setup, one row per gate
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
# Two stages, because a release owes a different set of things than a pull request:
#   pr        every pull request runs it, and the release pipeline runs it first —
#             an artifact nobody gated is an artifact that ships broken.
#   release   build-only: the distribution jobs that produce the binaries and the
#             site zip. Expensive enough that a PR should not pay for them.
#
# Adding a gate here is the whole job: `--list` feeds the matrix, so CI picks it
# up. Nothing in .github/ has to learn its name.
GATE_TABLE='
fmt|native|ubuntu-latest|rust-nightly|pr
lint|native|ubuntu-latest|rust-stable ffmpeg|pr
check|native|ubuntu-latest|rust-stable ffmpeg|pr
test|native|ubuntu-latest|rust-stable ffmpeg gpu|pr
no-video|native|ubuntu-latest|rust-stable|pr
render-smoke|native|ubuntu-latest|rust-stable ffmpeg gpu|pr
examples|native|ubuntu-latest|rust-stable ffmpeg|pr
ext-bench|native|ubuntu-latest|rust-stable|pr
doc|native|ubuntu-latest|rust-stable ffmpeg|pr
site-scenes|native|ubuntu-latest|rust-stable ffmpeg|pr
embed-drift|native|ubuntu-latest|node|pr
content-sync|native|ubuntu-latest|none|pr
meta-version|native|ubuntu-latest|rust-stable|pr
workflows|native|ubuntu-latest|none|pr
eparts|native|ubuntu-latest|rust-stable|pr
eparts|native|macos-latest|rust-stable|pr
eparts|native|windows-latest|rust-stable|pr
audit|nix|ubuntu-latest|nix|pr
commit|nix|ubuntu-latest|nix|pr
wasm-check|web|ubuntu-latest|nix|pr
web-build|web|ubuntu-latest|nix wasm-bindgen|pr
site-artifact|native|ubuntu-latest|none|site
dist-site|native|ubuntu-latest|none|site
dist-wasm|native|ubuntu-latest|none|site
dist-linux|nix|ubuntu-latest|nix|release
dist-macos|native|macos-latest|rust-stable|release
dist-windows|native|windows-latest|rust-stable|release
'

# The commands each gate expects to be run by. Kept as prose so `ci.sh shell`
# can print them and a human can copy one.
shell_for() {
  case "$1" in
    native) echo '' ;;
    nix) echo 'nix develop --command' ;;
    web) echo 'nix develop .#web-build --command' ;;
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
    echo "       $(shell_for web) scripts/ci.sh gate $1" >&2
    return 1
  fi
}

# `rustfmt.toml` uses unstable options (imports_granularity, group_imports,
# wrap_comments, format_code_in_doc_comments). A stable rustfmt does not reject
# them — it warns, ignores them, and then reports the files it just formatted
# differently from what nightly writes. So `cargo fmt --check` on stable is a
# gate that fails on correctly formatted code.
want_nightly_fmt() {
  FMT_CMD=(cargo fmt)
  if rustc --version | grep -q nightly; then return 0; fi
  if command -v rustup >/dev/null 2>&1 && rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
    FMT_CMD=(rustup run nightly cargo fmt)
    return 0
  fi
  echo "error: the fmt gate needs a nightly rustfmt — rustfmt.toml sets unstable" >&2
  echo "       options, and stable silently ignores them, so it would fail on code" >&2
  echo "       that nightly already formatted correctly." >&2
  echo "       (rustc is $(rustc --version); rustup $(command -v rustup || echo 'absent — nix shell'))" >&2
  echo "       CI runs this gate on dtolnay/rust-toolchain@nightly; locally:" >&2
  echo "       nix shell nixpkgs#rustup -c rustup toolchain install nightly" >&2
  return 1
}

# ── gates ─────────────────────────────────────────────────────────────────────
gate_fmt() {
  want_cargo; want_nightly_fmt
  "${FMT_CMD[@]}" --all -- --check
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
  # The `video` feature must stay truly opt-in. Verified by building the
  # user-facing binaries without FFmpeg present and without the feature, then
  # running the engine with every optional feature off — the profile the web
  # playback build uses, which nothing else looks at.
  cargo build -p animatix-cli -p animatix-gui
  ANIMATIX_REQUIRE_GPU=1 cargo test -p animatix --lib --no-default-features -- --test-threads=1
}

gate_render_smoke() {
  want_cargo
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

gate_embed_drift() {
  # `web/embed/amx-player.js` is the committed esbuild output of
  # `web/embed/src/amx-player.js`, and Pages ships the committed copy — so a src
  # change that skipped the rebuild deploys the previous player while the repo
  # looks fixed. This was a push-to-main gate; a PR now catches it before merge.
  command -v npm >/dev/null 2>&1 || { echo "error: npm not on PATH" >&2; return 1; }
  npm --prefix web/tools ci
  npm --prefix web/tools run build:embed
  git diff --exit-code web/embed/amx-player.js
}

gate_web_build() {
  want_cargo; want_wasm_std 'web-build'; want_nix_shell 'web-build'
  # shellcheck source=scripts/wasm-tools.sh
  . scripts/wasm-tools.sh
  check_wbg
  bash scripts/build-web.sh --slim
  bash scripts/build-web.sh
  # The two bundles are what the pages load; a build that "succeeds" without
  # writing both is how a --slim flag typo used to reach the site.
  for d in web/pkg-slim web/pkg; do
    [ -f "$d/animatix_web_bg.wasm" ] || { echo "error: $d/animatix_web_bg.wasm missing after build" >&2; return 1; }
    [ -f "$d/animatix_web.js" ] || { echo "error: $d/animatix_web.js missing after build" >&2; return 1; }
  done
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

gate_meta_version() {
  want_cargo
  # Version single-source: every workspace member must inherit [workspace.package],
  # so `cargo set-version --workspace` (the bump hook) has exactly one place to
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
  # each one clobber the other's build.
  cargo build --release --target-dir target-video -p animatix-cli -p animatix-gui --features video
  strip target-video/release/animatix target-video/release/animatix-gui
  check_binary target-video/release/animatix "$want"
  check_binary target-video/release/animatix-gui "$want"
  bash scripts/dist-package.sh linux target-video/release
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
  cargo build --release -p animatix-cli -p animatix-gui
  check_binary target/release/animatix "$want"
  check_binary target/release/animatix-gui "$want"
  bash scripts/dist-package.sh macos target/release
}

gate_dist_windows() {
  want_cargo
  local want="${ANIMATIX_RELEASE_VERSION:-}"
  want="${want#v}"
  cargo build --release -p animatix-cli -p animatix-gui
  check_binary target/release/animatix.exe "$want"
  check_binary target/release/animatix-gui.exe "$want"
  bash scripts/dist-package.sh windows target/release
}

# ── setup ─────────────────────────────────────────────────────────────────────
# Provisioning the runner needs that is *not* a toolchain. The toolchain hints
# (rust-stable / rust-nightly / node / nix) are installed by GitHub Actions and
# verified here, because a script that installs a toolchain inside a job that
# already picked one is how two toolchains end up on PATH.
setup_one() {
  case "$1" in
    ffmpeg)
      sudo apt-get update
      sudo apt-get install -y --no-install-recommends libffmpeg-dev pkg-config
      ;;
    gpu)
      # lavapipe, the software Vulkan driver: without it the render gates skip
      # instead of rasterizing, and pass while testing nothing.
      sudo apt-get update
      sudo apt-get install -y --no-install-recommends mesa-vulkan-drivers
      ;;
    wasm-bindgen)
      # The CLI must equal the crate's `=0.2.128` pin exactly; a mismatch fails
      # at glue generation, several build minutes in. Version read from the pin.
      # shellcheck source=scripts/wasm-tools.sh
      . scripts/wasm-tools.sh
      install_wbg
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

stage="${2:-}"
case "${1:-}" in
  --list)
    rows "$stage"
    ;;
  --matrix)
    rows "$stage" | python3 -c '
import json, sys
rows = [l.rstrip("\n").split("\t") for l in sys.stdin if l.strip()]
print(json.dumps({"include": [
    {"gate": n, "shell": s, "os": o, "setup": d, "stage": st,
     "name": "%s (%s)" % (n, o) if st == "pr" or o == "ubuntu-latest" else "%s (%s, %s)" % (n, o, st)}
    for n, s, o, d, st in rows]}))
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
    for g in "$@"; do run_gate "$g"; done
    ;;
  setup)
    shift
    [ $# -ge 1 ] || { echo "usage: ci.sh setup <token>…" >&2; exit 2; }
    for t in "$@"; do setup_one "$t"; done
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
