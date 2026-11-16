#!/usr/bin/env python3
"""Check the Gradient content's numerical and parameter contracts.

This is a source-level regression check, not browser/renderer verification.
Run the CLI with --set and inspect actual frames before publishing changed art.
"""
from __future__ import annotations

import math
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "web/demos/gradient"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


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

    # Learning-rate figure now compares normalized loss on a shared Cartesian axis.
    # Equal references must have identical plotted loss; no arbitrary rescaling.
    require("let initial_loss = 2 * x0^2 + y0^2" in rates, "normalization must use the common initial loss")
    require("2*x0^2*pow(abs(kx),2*n) + y0^2*pow(abs(ky),2*n)" in rates, "readout must use loss at the discrete iterate")
    require("let n = floor(clamp((t-1)/6,0,1)*steps)" in rates, "selected marker must sample discrete optimizer steps")
    require("550 - ratio*240" in rates, "selected loss marker must share the plot's loss scale")
    for rate in (0.02, 0.10, 0.35, 0.50):
        for x,y in iterates(rate,3.4,1.6,48):
            require(0 <= (2*x*x+y*y)/25.68 <= 1+1e-12, "supported loss curve must remain within 0–100%")

    require('"<0.1% remains"' in rates, "small positive losses must not be presented as exact zero")
    require('"After 1 update,"' in rates, "single-step readout must preserve natural grammar")
    require('"Its distance never shrinks."' in rates, "boundary explanation must distinguish sign flips from convergence")

    generator = ROOT / "tools/content/gradient-plate.py"
    if generator.exists():
        subprocess.run([sys.executable, str(generator), "--check"], cwd=ROOT, check=True)
    print("Gradient contracts passed: recurrence, 0.10/0.35/0.50 regimes, fixed rates and generated source.")
    print("Scope: static/numerical checks only; retained sources are regression inputs, not current website UI.")


if __name__ == "__main__":
    check()
