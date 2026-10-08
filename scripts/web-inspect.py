#!/usr/bin/env python3
"""Inspect, verify and screenshot <amx-player> scenes across the web demo pages.

Uses `scripts/agent-browser.sh` over Chrome DevTools Protocol (CDP) to:
1. Preflight check WASM freshness (via scripts/ensure-web-pkg.sh).
2. Load any site demo page (e.g. http://localhost:8124/demos/transformer/).
3. Query all `<amx-player>` elements on the page, eagerly loading them.
4. Verify WebGPU context, scene compilation, duration, and frame counts.
5. Capture real WebGPU canvas frames via `canvas.toDataURL()`.
6. Catch and report any scene error veils or missing actions.

Usage:
  scripts/web-inspect.py --url http://127.0.0.1:8124/
  scripts/web-inspect.py --url http://127.0.0.1:8124/demos/transformer/ --output-dir /tmp/shots
  scripts/web-inspect.py --check --url http://127.0.0.1:8124/gallery.html
"""
import argparse
import base64
import json
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AGENT_BROWSER = os.path.join(ROOT, "scripts", "agent-browser.sh")


def run_agent_browser(args, timeout=30):
    cmd = [AGENT_BROWSER] + args
    res = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    return res


def eval_js(expr, timeout=30):
    res = run_agent_browser(["eval", expr], timeout=timeout)
    if res.returncode != 0:
        raise RuntimeError(f"agent-browser eval failed: {res.stderr}\nOutput: {res.stdout}")
    # eval outputs json-serialized string
    try:
        return json.loads(res.stdout)
    except json.JSONDecodeError:
        return res.stdout.strip()


def check_server(url):
    import urllib.request
    try:
        req = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(req, timeout=3) as resp:
            return resp.status == 200
    except Exception:
        return False


def main():
    parser = argparse.ArgumentParser(description="Inspect <amx-player> scenes on web pages")
    parser.add_argument("--url", default="http://127.0.0.1:8124/", help="Target URL to inspect")
    parser.add_argument("--output-dir", default=None, help="Directory to save rendered canvas frames")
    parser.add_argument("--seek", type=float, default=None, help="Seek each player to specific time (seconds)")
    parser.add_argument("--check", action="store_true", help="Fail with exit code 1 if any player has errors")
    parser.add_argument("--timeout", type=int, default=15, help="Timeout in seconds for scene initialization")
    args = parser.parse_args()

    # Preflight 1: verify dev server is reachable
    if not check_server(args.url):
        print(f"error: server at {args.url} is not responding.", file=sys.stderr)
        print("       Start it with: python3 scripts/serve-web.py", file=sys.stderr)
        sys.exit(1)

    # Preflight 2: check WASM freshness
    check_script = os.path.join(ROOT, "scripts", "ensure-web-pkg.sh")
    if os.path.isfile(check_script):
        res = subprocess.run([check_script, "--check"], capture_output=True, text=True)
        if res.returncode != 0:
            print("⚠️  " + res.stderr.strip(), file=sys.stderr)
            if args.check:
                print("Aborting: WASM is outdated. Run scripts/ensure-web-pkg.sh --dev first.", file=sys.stderr)
                sys.exit(1)

    print(f"Connecting to {args.url} via agent-browser...")
    open_res = run_agent_browser(["open", args.url])
    if open_res.returncode != 0:
        print(f"error: failed to open {args.url}: {open_res.stderr}", file=sys.stderr)
        sys.exit(1)

    # Verify WebGPU support in page
    has_webgpu = eval_js("Boolean(navigator.gpu)")
    if not has_webgpu:
        print("error: navigator.gpu is not available in browser.", file=sys.stderr)
        sys.exit(1)

    # Trigger eager loading of all players on the page
    eval_js("""
    (() => {
      const players = document.querySelectorAll('amx-player');
      players.forEach(p => {
        if (typeof p.loadNow === 'function') {
          p.loadNow();
        } else {
          p.scrollIntoView({ behavior: 'instant', block: 'center' });
        }
      });
      return players.length;
    })()
    """)

    # Poll until all players are either ready or error (up to timeout)
    start_time = time.time()
    player_count = 0
    while time.time() - start_time < args.timeout:
        status_info = eval_js("""
        (() => {
          const players = Array.from(document.querySelectorAll('amx-player'));
          return {
            total: players.length,
            settled: players.filter(p => p._state === 'ready' || p._state === 'error').length
          };
        })()
        """)
        player_count = status_info.get("total", 0)
        settled = status_info.get("settled", 0)
        if player_count > 0 and settled == player_count:
            break
        time.sleep(0.5)

    if player_count == 0:
        print(f"No <amx-player> elements found on {args.url}.")
        return

    # Extract player details
    players_data = eval_js("""
    (() => {
      const players = Array.from(document.querySelectorAll('amx-player'));
      return players.map((p, idx) => {
        const shadow = p.shadowRoot;
        const canvas = shadow ? shadow.querySelector('canvas') : null;
        const veil = shadow ? shadow.querySelector('.veil') : null;
        const isErrorVeil = veil ? veil.classList.contains('error') && veil.classList.contains('show') : false;
        const veilText = veil ? veil.innerText.trim() : '';

        return {
          index: idx,
          src: p.getAttribute('src') || '',
          state: p._state || 'unknown',
          hasCanvas: Boolean(canvas),
          duration: p.duration || 0,
          time: p.time || 0,
          frame: p.frame || 0,
          totalFrames: p.totalFrames || 0,
          debugReport: typeof p.debugReport === 'function' ? p.debugReport() : '',
          isError: isErrorVeil || p._state === 'error',
          errorMsg: isErrorVeil ? veilText : (p._state === 'error' ? veilText || 'Failed to load' : '')
        };
      });
    })()
    """)

    try:
        if args.output_dir:
            os.makedirs(args.output_dir, exist_ok=True)

        print(f"\nFound {len(players_data)} <amx-player> scene(s) on {args.url}:\n")
        print(f"{'#':<3} {'SRC':<42} {'STATE':<8} {'DUR':<6} {'FRAMES':<10} {'STATUS'}")
        print("-" * 80)

        has_errors = False
        for p in players_data:
            idx = p["index"]
            src = p["src"]
            state = p["state"]
            dur = f"{p['duration']:.2f}s"
            frames = f"{p['frame']}/{p['totalFrames']}"
            status = "OK"

            if p["isError"]:
                has_errors = True
                status = f"ERROR: {p['errorMsg']}"
            elif not p["hasCanvas"]:
                status = "NO CANVAS"

            print(f"{idx:<3} {src:<42} {state:<8} {dur:<6} {frames:<10} {status}")

            # Capture frame if requested
            if args.output_dir and p["hasCanvas"] and not p["isError"]:
                if args.seek is not None:
                    eval_js(f"document.querySelectorAll('amx-player')[{idx}].seek({args.seek})")
                    time.sleep(0.1)

                data_url = eval_js(f"""
                document.querySelectorAll('amx-player')[{idx}].shadowRoot.querySelector('canvas').toDataURL()
                """)
                if isinstance(data_url, str) and data_url.startswith("data:image/png;base64,"):
                    raw_bytes = base64.b64decode(data_url.split(",", 1)[1])
                    safe_name = re.sub(r"[^a-zA-Z0-9_\-]", "_", os.path.basename(src) or f"player_{idx}")
                    out_path = os.path.join(args.output_dir, f"{idx:02d}_{safe_name}.png")
                    with open(out_path, "wb") as f:
                        f.write(raw_bytes)
                    print(f"    ↳ Saved frame: {out_path} ({len(raw_bytes)} bytes)")

        print("-" * 80)
        if has_errors and args.check:
            print("\n❌ Scene verification FAILED: one or more players encountered errors.", file=sys.stderr)
            sys.exit(1)
        elif not has_errors:
            print("\n✓ All scenes verified cleanly.")
    finally:
        run_agent_browser(["close"])


if __name__ == "__main__":
    main()
