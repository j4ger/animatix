#!/usr/bin/env python3
"""Check the Gradient content's numerical and parameter contracts.

This is a source-level regression check, not browser/renderer verification.
Run the CLI with --set and inspect actual frames before publishing changed art.
"""
from __future__ import annotations

from html.parser import HTMLParser
import json
import math
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
HERE = ROOT / "web/demos/gradient"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


class Page(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.inputs: dict[str, dict[str, str | None]] = {}
        self.players: list[dict[str, str | None]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if tag == "input" and values.get("data-var"):
            self.inputs[str(values["data-var"])] = values
        if tag == "amx-player":
            self.players.append(values)


def iterates(rate: float, x: float, y: float, steps: int) -> list[tuple[float, float]]:
    points = [(x, y)]
    for _ in range(steps):
        x, y = x - rate * 4 * x, y - rate * 2 * y
        points.append((x, y))
    return points


def check() -> None:
    # The exact closed form independently checks the repeated-update oracle,
    # including the sign handling needed by the scene's animated cursor.
    for rate in (0.10, 0.35, 0.50):
        for x0, y0 in ((3.4, 1.6), (3.6, 1.8)):
            for steps in (8, 24, 48):
                points = iterates(rate, x0, y0, steps)
                for n, (x, y) in enumerate(points):
                    require(math.isclose(x, x0 * (1 - 4 * rate) ** n, abs_tol=1e-12), "x recurrence differs from closed form")
                    require(math.isclose(y, y0 * (1 - 2 * rate) ** n, abs_tol=1e-12), "y recurrence differs from closed form")
                losses = [2 * x * x + y * y for x, y in points]
                require(all(b <= a + 1e-12 for a, b in zip(losses, losses[1:])), "loss must not increase for these rates")
                if rate == 0.50:
                    require(all(math.isclose(abs(x), x0) for x, _ in points), "boundary x must persist, not converge")
                    require(all(y == 0 for _, y in points[1:]), "boundary y must vanish after one step")
                    require(math.isclose(losses[-1], 2 * x0 * x0), "boundary loss must remain positive")
                elif rate == 0.35:
                    require(all(x * nx < 0 for (x, _), (nx, _) in zip(points, points[1:])), "0.35 must zigzag across x=0")
                    require(losses[-1] < losses[0], "zigzag still converges")
                else:
                    require(all(x > 0 and y > 0 for x, y in points), "0.10 approaches without sign flips")

    # Tie the oracle to authored formulas, rather than testing a detached toy.
    for name in ("losssurface", "gradientdirection", "descent", "learningrate"):
        body = (HERE / f"{name}.amx").read_text()
        require("2 * x^2 + y^2" in body, f"{name}: loss definition changed; update numerical contract")
        if name == "losssurface":
            require("(4 * x, 2 * y)" in body, "vector field must equal gradient of loss")
        else:
            require("let kx = 1 - 4 * lr" in body and "let ky = 1 - 2 * lr" in body, f"{name}: cursor factors differ from the gradient")
            for axis, factor in (("x", 4), ("y", 2)):
                pattern = rf"let n{axis} = if i < steps \{{ ([pm]{axis}) - lr \* {factor} \* \1 \}}"
                require(re.search(pattern, body) is not None, f"{name}: drawn path and cursor recurrence differ for {axis}")
    rates = (HERE / "learningrate.amx").read_text()
    for name, value in (("lr", 0.1), ("r_left", 0.02), ("r_right", 0.35)):
        match = re.search(rf"^let {name} = ([0-9.]+)$", rates, re.M)
        require(match is not None and float(match[1]) == value, f"{name}: comparison rate changed")

    work = next(w for w in json.loads((ROOT / "web/works.json").read_text())["works"] if w["id"] == "gradient")
    page = Page()
    page.feed((HERE / "index.html").read_text())
    for param in work["params"]:
        slider = page.inputs[param["name"]]
        for attr, field in (("min", "min"), ("max", "max"), ("step", "step"), ("value", "default")):
            require(float(str(slider[attr])) == float(param[field]), f"slider {param['name']} {attr} disagrees with works.json")
    lr = next(p for p in work["params"] if p["name"] == "lr")
    require(lr["min"] > 0 and lr["max"] == 0.5, "domain includes convergence and its boundary, not divergence")
    require({p.get("src") for p in page.players} == {"scene.amx", "losssurface.amx", "gradientdirection.amx", "descent.amx", "learningrate.amx"}, "preserve all existing player/source URLs")
    composed = next(p for p in page.players if p.get("src") == "scene.amx")
    require("data-param-static" in composed, "composed plate must disclose/exclude unsupported variable overrides")
    require(all("data-param-static" not in p for p in page.players if p.get("src") != "scene.amx"), "individual figures must remain interactive")
    deck = (ROOT / "web/demos/deck.js").read_text()
    rail = deck.split("function initParamRails(root)", 1)[1]
    require(rail.count("amx-player:not([data-param-static])") == 2, "both scoped and fallback parameter targets must exclude static plates")
    generator = ROOT / "scripts/gradient-plate.py"
    if generator.exists():
        subprocess.run([sys.executable, str(generator), "--check"], cwd=ROOT, check=True)
    print("Gradient contracts passed: recurrence, 0.10/0.35/0.50 regimes, fixed rates, sliders, source URLs.")
    print("Scope: static/numerical checks only; browser interactions and rendered captions require visual QA.")


if __name__ == "__main__":
    check()
