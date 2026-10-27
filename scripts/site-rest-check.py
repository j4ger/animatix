#!/usr/bin/env python3
"""Check that every scene the site plays rests on a composition.

An `<amx-player autoplay loop>` cycle is `duration` + `hold`: the finished frame
sits on screen for `hold` seconds, then the embed dissolves it into the next
cycle. A closing beat that fades the cast out therefore erases the figure for
the whole rest of the loop — the reader is left holding an empty plate. The
embed's own dissolve already does what such a cascade was written to do.

This script renders the held frame and the fullest frame of every scene the
pages embed, and fails when the held frame drops far below the plateau:

    scripts/site-rest-check.py            # every scene the site plays
    scripts/site-rest-check.py web/tour   # only paths under this prefix

`animatix verify` is the instrument, so the numbers are the same ones the
transformer `*.verify.txt` files assert against.

A scene may opt out with a marker in its own source, which the script prints
rather than skipping silently, so an exemption stays auditable:

    // rest-check: exempt - <why this one may rest on an empty plate>

That is for figures whose resting frame is *meant* to be the page behind them
(the home page's full-bleed hero), not for a bordered figure the reader is meant
to take something away from.

A multi-scene plate rests on its **last** scene, so that scene is the plateau:
comparing its closing title card with the walkthrough's densest act (a plot, the
hero figure) would fail every plate that ends on a deliberately sparser closing
figure, even though nothing was erased.

What this cannot see, on purpose: the measurement is whole-frame coverage, so it
catches a closing beat that erases *the cast* (the defect it was written for) and
is blind to one that erases a single actor sitting on busy pixels behind it — a
`Glass` card's own heading can vanish here and the frame keeps its ink, because
`verify`'s `visible <t> <label>` measures the coverage of the actor's bounds, not
its own pixels. Isolating one actor needs a per-actor render, which the engine
does not offer. Figures where that matters are accepted by eye and recorded in
`docs/history.md`.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "web"
BIN = pathlib.Path(__file__).resolve().parent.parent / "target" / "debug" / "animatix"

# The held frame must keep at least this much of the plateau's coverage.
REST_FLOOR = 0.35
def players() -> list[pathlib.Path]:
    """Every .amx a page embeds, found through the players rather than the tree."""
    scenes: list[pathlib.Path] = []
    for page in sorted(ROOT.rglob("*.html")):
        for src in re.findall(r"<amx-player[^>]*\ssrc=\"([^\"]+)\"", page.read_text()):
            resolved = (page.parent / src).resolve()
            if resolved.exists() and resolved not in scenes:
                scenes.append(resolved)
    return scenes


def timeline_info(path: pathlib.Path) -> tuple[float, list[tuple[float, float, int]]]:
    """The cycle length and the scene table of a plate.

    A row carries `(start, duration, transition_ms_into_next)`. The transition
    into the *last* scene is on the previous row, so the final scene's own
    content starts after it — until then the outgoing scene is still on screen,
    and sampling the cross-fade would call the walkthrough's densest act the
    closing scene's plateau.
    """
    out = subprocess.run(
        [str(BIN), "timeline", str(path)], capture_output=True, text=True
    ).stdout
    total = 0.0
    if m := re.search(r"total:\s*([\d.]+)s", out):
        total = float(m.group(1))
    if total <= 0:
        m = re.search(r"duration:\s*([0-9.]+)", path.read_text())
        total = float(m.group(1)) if m else 0.0
    scenes = []
    for start, length, tail in re.findall(
        r"(?m)^\s*\d+\s+\S+\s+([\d.]+)\s+([\d.]+)\s+(.*)$", out
    ):
        transition = re.search(r"\[[^,\]]+,\s*(\d+)ms\]", tail)
        scenes.append((float(start), float(length), int(transition.group(1)) if transition else 0))
    return total, scenes


def beats(path: pathlib.Path) -> list[float]:
    return sorted({float(t) for t in re.findall(r"(?m)^#([0-9.]+)s", path.read_text())})


def measure(path: pathlib.Path, times: list[float]) -> dict[float, float]:
    probe = path.with_name("_rest_check.txt")
    probe.write_text("".join(f"ink {t:.2f} 0.0001\n" for t in times))
    try:
        out = subprocess.run(
            [str(BIN), "verify", str(path), "--checks", str(probe)],
            capture_output=True,
            text=True,
        ).stdout
    finally:
        probe.unlink()
    return {
        round(float(t), 2): float(ink)
        for t, ink in re.findall(r"ink ([\d.]+) \S+ — frame ink (\d+\.\d+)%", out)
    }


def exemption(path: pathlib.Path) -> str | None:
    """The reason a scene is allowed to rest on an empty plate, if it declares one."""
    m = re.search(r"(?m)^//\s*rest-check:\s*exempt\s+-\s+(.+)$", path.read_text())
    return m.group(1).strip() if m else None


def check(path: pathlib.Path) -> list[str]:
    rel = path.relative_to(ROOT.parent)
    duration, scenes = timeline_info(path)
    if duration <= 0:
        return [f"{rel}: no duration found"]
    held = round(duration - 0.05, 2)
    if len(scenes) > 1:
        # A plate holds on its *last* scene. Comparing that frame with the
        # densest moment of the whole walkthrough (the hero act, a plot) fails
        # every plate that ends on a deliberately sparser closing figure, even
        # though nothing was erased. The plateau is therefore the last scene's
        # own peak. Sample it directly: `#Ns` stamps are scene-local, so
        # shifting them into absolute time conflates the acts.
        start, length, _ = scenes[-1]
        # The transition into the last scene lives on the previous row; the
        # outgoing act owns that window, not the closing figure.
        content_start = start + scenes[-2][2] / 1000.0
        content_start = min(content_start, held)
        end = min(start + length, duration)
        samples = [
            round(content_start + i * 0.5, 2)
            for i in range(int((end - content_start) / 0.5) + 1)
        ]
        samples = sorted({*samples, round(content_start, 2), held})
    else:
        stamp = [t for t in beats(path) if t <= duration + 0.01]
        # Sample the whole arc so the plateau is the real peak, not one beat's guess.
        samples = sorted({0.0, *stamp, round(duration / 2, 2), held})
    ink = measure(path, samples)
    if not ink:
        return [f"{rel}: verify produced no measurements"]

    peak = max(ink.values())
    problems: list[str] = []
    # A sparse closing title card is a composition too: with a 1% floor its
    # 0.7% text could be erased to 0 and still be skipped as "nearly blank".
    if peak > 0.1 and ink.get(held, 0.0) < REST_FLOOR * peak:
        problems.append(
            f"{rel}: the held frame is {ink[held]:.1f}% ink against a {peak:.1f}% "
            f"plateau — a closing beat erases the cast (dur={duration:.2f}s)"
        )
    return problems


def main() -> int:
    prefixes = sys.argv[1:]
    scenes = players()
    if prefixes:
        scenes = [s for s in scenes if any(p in str(s) for p in prefixes)]
    if not scenes:
        print("no scenes matched", file=sys.stderr)
        return 2
    failures: list[str] = []
    exempt: list[str] = []
    for scene in scenes:
        if why := exemption(scene):
            exempt.append(f"{scene.relative_to(ROOT.parent)}: {why}")
            continue
        for problem in check(scene):
            print(problem)
            failures.append(problem)
    for note in exempt:
        print(f"exempt (declared by the scene): {note}")
    status = "FAIL" if failures else "ok"
    print(
        f"\n{status}: {len(scenes)} embedded scenes checked, "
        f"{len(failures)} resting-composition violation(s)"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
