#!/usr/bin/env python3
"""Derive the epicycles plate (scene.amx) from the five act files.

The page embeds each act standalone *and* as one playthrough. The composition
engine can only chain scenes through a `play` written inside a scene body, so an
imported act that stays chain-free for its own embed cannot be told what comes
next from the outside — the plate has to carry the act text itself. Hand-copying
that text is how §05 ended up spinning clockwise in one file and
counter-clockwise in the other, with a duplicated `font_size` in one copy only.

So the acts are the source and the plate is generated:

    scripts/epicycles-plate.py            # rewrite scene.amx from the acts
    scripts/epicycles-plate.py --check    # fail if scene.amx is stale

The plate also gets the one thing the standalone acts must not have: a closing
beat that clears each heading before its wipe. All five acts title at
`scene.top, offset: (0, 52)`, so a crossfade superimposes two headings into an
unreadable third. Clearing them in an act file would erase part of the cast from
the frame its embed holds (see scripts/site-rest-check.py), which is why the
cascade belongs to the plate and not to the acts.
"""

from __future__ import annotations

import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent.parent / "web" / "demos" / "epicycles"

# (file, scene label, actors the plate clears before the wipe to the next act)
ACTS = [
    ("goal.amx", "Goal", ["goal_title", "goal_axis", "goal_hint"]),
    ("onecircle.amx", "OneCircle", ["one_title", "one_hint"]),
    ("hero.amx", "Hero", ["hero_title", "hero_hint", "eq_row"]),
    ("spectrum.amx", "Spectrum", ["spec_title", "spec_eq", "spec_note"]),
    ("complexplane.amx", "ComplexPlane", []),
]

TRANSITION_MS = 500

HEADER = """// scene.amx — the plate: the whole walkthrough in one playthrough.
//
// GENERATED — do not edit. This file is derived from the five act files by
// `scripts/epicycles-plate.py`; edit an act and re-run it. `--check` fails when
// this file no longer matches its sources, so the two cannot drift apart
// quietly.
//
// Why a generated file and not a thin plate: a `play` statement only lives
// inside a scene body, so an imported act that stays chain-free for its own
// embed on the page can never be told what comes next from the outside.
// (examples/gallery/brand_reel/main.amx gets away with a thin plate only
// because none of its scenes is also embedded standalone.)
//
// Two things worth knowing about the schedule:
//
//   * A `play` fires when its scene's `duration` elapses, minus the transition.
//     The `#Xs` stamp in front of a `play` is parsed but dropped by the
//     composition builder, so the durations above are the schedule.
//   * Every join is a wipe, and each act's headings clear themselves just
//     before it. All five acts title at `scene.top, offset: (0, 52)`; a `fade`
//     would superimpose two of them into an unreadable third.

import "../../demos/lib/theme.amx"
import "fourier.amx" as geo

config {
  colorscheme: "ink",
  resolution: (1280, 720),
  dynamic_layout: true,
}
"""


def act_body(fname: str, scene: str) -> str:
    """The act's own text, from its `# Scene` header to the end of the file."""
    lines = (HERE / fname).read_text().splitlines()
    start = next(i for i, ln in enumerate(lines) if ln.strip() == f"# {scene}")
    return "\n".join(lines[start:]).rstrip()


def outro(clear: list[str], duration: float) -> str:
    """The plate-only beats: lift the headings out of the way of the wipe."""
    if not clear:
        return ""
    at = duration - TRANSITION_MS / 1000.0 - 0.35
    stmts = "\n".join(f"fade-out {name} [350ms, ease: ease-in]" for name in clear)
    return f"\n\n// Plate only: the next act titles at the same anchor.\n#{at:.2f}s\n{stmts}\n"


def duration_of(body: str) -> float:
    import re

    m = re.search(r"duration:\s*([0-9.]+)", body)
    if not m:
        raise SystemExit("an act declares no duration; the plate cannot schedule its wipe")
    return float(m.group(1))


def build() -> str:
    out = [HEADER]
    for i, (fname, scene, clear) in enumerate(ACTS):
        rule = "─" * max(4, 56 - len(scene) - len(fname))
        out.append(f"\n// ── act: {scene}  (source: {fname}) {rule}\n\n")
        body = act_body(fname, scene)
        out.append(body)
        out.append(outro(clear, duration_of(body)))
        if i + 1 < len(ACTS):
            nxt = ACTS[i + 1][1]
            out.append(f"\n\nplay {nxt} [wipe-left, {TRANSITION_MS}ms]\n")
    return "".join(out).rstrip() + "\n"


def main() -> int:
    check = "--check" in sys.argv[1:]
    target = HERE / "scene.amx"
    want = build()
    if check:
        have = target.read_text() if target.exists() else ""
        if have != want:
            print("scene.amx is stale — run scripts/epicycles-plate.py", file=sys.stderr)
            return 1
        print("scene.amx matches its five acts")
        return 0
    target.write_text(want)
    print(f"wrote {target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
