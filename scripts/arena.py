#!/usr/bin/env python3
"""Animatix Arena CLI — Manage comparative evaluation battles and agent feedback.

Usage:
  python3 scripts/arena.py stage <slug> --title "Title" --brief "Brief" --variants a.amx b.amx
  python3 scripts/arena.py serve [--port 8124] [--battle <slug>]
  python3 scripts/arena.py ingest <slug>
  python3 scripts/arena.py list
  python3 scripts/arena.py report
"""

import argparse
import json
import os
import shutil
import subprocess
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_DIR = os.path.dirname(SCRIPT_DIR)
ARENA_DIR = os.path.join(PROJECT_DIR, "dogfood", "arena")
BATTLES_DIR = os.path.join(ARENA_DIR, "battles")
FEEDBACK_LOG = os.path.join(ARENA_DIR, "feedback.jsonl")


def run_cmd(cmd, check=True):
    return subprocess.run(cmd, cwd=PROJECT_DIR, check=check, text=True, capture_output=True)


def cmd_stage(args):
    slug = args.slug.strip()
    target_dir = os.path.join(BATTLES_DIR, slug)
    os.makedirs(target_dir, exist_ok=True)

    print(f"=== Staging Arena Battle: {slug} ===")

    variants_meta = []
    for idx, var_path in enumerate(args.variants):
        if not os.path.isfile(var_path):
            print(f"Error: Variant file not found: {var_path}", file=sys.stderr)
            sys.exit(1)

        # Validate with animatix check
        print(f"Checking variant: {var_path}...")
        check_proc = subprocess.run(
            ["cargo", "run", "--quiet", "--bin", "animatix", "--", "check", var_path],
            cwd=PROJECT_DIR,
            capture_output=True,
            text=True,
        )
        if check_proc.returncode != 0:
            print(f"Warning/Error in {var_path}:\n{check_proc.stderr or check_proc.stdout}", file=sys.stderr)

        vid = chr(ord('a') + idx) if idx < 26 else f"v{idx}"
        dest_filename = f"{vid}.amx"
        dest_path = os.path.join(target_dir, dest_filename)
        shutil.copyfile(var_path, dest_path)

        label = args.labels[idx] if args.labels and idx < len(args.labels) else f"Variant {vid.upper()}"
        variants_meta.append({
            "id": vid,
            "label": label,
            "file": dest_filename,
        })

    title = args.title or slug.replace("-", " ").title()
    brief = args.brief or "No brief specified."
    tags = args.tags or ["dogfood"]

    battle_meta = {
        "id": slug,
        "title": title,
        "category": args.category,
        "tags": tags,
        "brief": brief,
        "variants": variants_meta,
    }

    meta_path = os.path.join(target_dir, "battle.json")
    with open(meta_path, "w", encoding="utf-8") as f:
        json.dump(battle_meta, f, indent=2)

    brief_path = os.path.join(target_dir, "brief.md")
    with open(brief_path, "w", encoding="utf-8") as f:
        f.write(f"# Brief: {title}\n\n{brief}\n")

    print(f"\n✓ Battle staged at {target_dir}")
    print(f"  Review in browser with: python3 scripts/arena.py serve --battle {slug}")


def cmd_serve(args):
    port = args.port
    serve_script = os.path.join(SCRIPT_DIR, "serve-web.py")
    battle_param = f"?battle={args.battle}" if args.battle else ""
    url = f"http://127.0.0.1:{port}/arena/{battle_param}"

    print(f"\n=======================================================")
    print(f"  Animatix Web Arena running at:")
    print(f"  --> {url}")
    print(f"=======================================================\n")

    cmd = [sys.executable, serve_script, str(port)]
    try:
        subprocess.run(cmd, cwd=PROJECT_DIR)
    except KeyboardInterrupt:
        print("\nArena server stopped.")


def cmd_ingest(args):
    slug = args.slug.strip()
    target_dir = os.path.join(BATTLES_DIR, slug)
    feedback_file = os.path.join(target_dir, "feedback.json")

    if not os.path.isfile(feedback_file):
        print(f"No feedback.json found for battle: {slug}", file=sys.stderr)
        print("Submit feedback via the Web Arena first.", file=sys.stderr)
        sys.exit(1)

    with open(feedback_file, "r", encoding="utf-8") as f:
        data = json.load(f)

    print(f"\n# Arena Ingestion Report: {slug}\n")
    print(f"- **Decision:** `{data.get('decision', 'unknown')}`")
    winner = data.get("winner")
    if winner:
        winner_file = os.path.join(target_dir, f"{winner}.amx")
        print(f"- **Winner:** `{winner}` ({winner_file})")
    else:
        print(f"- **Winner:** None / Tie")

    rubric = data.get("rubric", {})
    if rubric:
        print("\n### Rubric Scores:")
        for k, val in rubric.items():
            if isinstance(val, dict):
                sub_str = ", ".join(f"{sub_k}={sub_v}/5" for sub_k, sub_v in val.items())
                print(f"- **Variant {k.upper()}:** {sub_str}")
            else:
                print(f"- **{k.title()}:** {val} / 5")

    notes = data.get("notes", [])
    if notes:
        print("\n### Timeline Notes & Critiques:")
        for n in notes:
            t = f" @ {(n.get('time_ms', 0) / 1000):.2f}s" if n.get("time_ms") is not None else ""
            tag = f" [{n.get('tag')}]" if n.get("tag") else ""
            print(f"- {t}{tag}: {n.get('note')}")

    summary = data.get("outcome_summary")
    if summary:
        print(f"\n### Directives for Agent:\n> {summary}\n")


def cmd_list(args):
    if not os.path.isdir(BATTLES_DIR):
        print("No battles directory found.")
        return

    battles = sorted(os.listdir(BATTLES_DIR))
    if not battles:
        print("No arena battles found.")
        return

    print(f"\nStaged Arena Battles ({len(battles)}):\n")
    for slug in battles:
        bdir = os.path.join(BATTLES_DIR, slug)
        if not os.path.isdir(bdir):
            continue

        meta_file = os.path.join(bdir, "battle.json")
        title = slug
        if os.path.isfile(meta_file):
            try:
                with open(meta_file, "r", encoding="utf-8") as f:
                    meta = json.load(f)
                    title = meta.get("title", slug)
            except Exception:
                pass

        has_feedback = os.path.isfile(os.path.join(bdir, "feedback.json"))
        status = "✓ Evaluated" if has_feedback else "● Pending Review"

        var_count = len([f for f in os.listdir(bdir) if f.endswith(".amx")])
        print(f"  [{status}] {slug:<28} | {var_count} variants | {title}")
    print()


def cmd_report(args):
    if not os.path.isfile(FEEDBACK_LOG):
        print(f"No feedback history found at {FEEDBACK_LOG}")
        return

    records = []
    with open(FEEDBACK_LOG, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    records.append(json.loads(line))
                except Exception:
                    pass

    print(f"# Arena Historical Leaderboard ({len(records)} entries)\n")
    for r in records:
        bid = r.get("battle_id", "unknown")
        dec = r.get("decision", "unknown")
        win = r.get("winner", "none")
        print(f"- **{bid}**: Decision `{dec}` (Winner: `{win}`)")
        summary = r.get("outcome_summary")
        if summary:
            print(f"  _{summary}_")


def main():
    parser = argparse.ArgumentParser(description="Animatix Arena CLI")
    sub = parser.add_subparsers(dest="command", required=True)

    # stage
    p_stage = sub.add_parser("stage", help="Stage a new arena battle")
    p_stage.add_argument("slug", help="Battle slug name")
    p_stage.add_argument("--title", help="Battle title")
    p_stage.add_argument("--brief", help="Content goal or hypothesis")
    p_stage.add_argument("--category", default="language", help="Category (language, effects, primitives)")
    p_stage.add_argument("--tags", nargs="*", default=["dogfood"], help="Tags")
    p_stage.add_argument("--variants", nargs="+", required=True, help="List of candidate .amx files")
    p_stage.add_argument("--labels", nargs="*", help="Optional labels for variants")

    # serve
    p_serve = sub.add_parser("serve", help="Launch the Web Arena server")
    p_serve.add_argument("--port", type=int, default=8124, help="Server port (default: 8124)")
    p_serve.add_argument("--battle", help="Default battle to open")

    # ingest
    p_ingest = sub.add_parser("ingest", help="Ingest feedback and output markdown report")
    p_ingest.add_argument("slug", help="Battle slug")

    # list
    sub.add_parser("list", help="List all staged arena battles")

    # report
    sub.add_parser("report", help="Display aggregated historical report")

    args = parser.parse_args()
    if args.command == "stage":
        cmd_stage(args)
    elif args.command == "serve":
        cmd_serve(args)
    elif args.command == "ingest":
        cmd_ingest(args)
    elif args.command == "list":
        cmd_list(args)
    elif args.command == "report":
        cmd_report(args)


if __name__ == "__main__":
    main()
