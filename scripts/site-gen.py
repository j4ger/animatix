#!/usr/bin/env python3
"""Regenerate the manifest-driven blocks inside the portfolio pages.

`web/works.json` is the single source of truth for every showcased work. The
pages carry generated regions delimited by markers:

    <!-- site-gen:<block>:start -->
    …generated html…
    <!-- site-gen:<block>:end -->

Supported blocks, per page:

    index.html         works-featured   flagship feature + three work tiles
    demos/index.html   works-channels   the full-bleed stage's channel rail
    demos/index.html   works-data       <script type="application/json"> payload
    demos/index.html   works-catalog    the filterable work grid
    gallery.html       works-gallery    the standalone figure grid

Usage:
    scripts/site-gen.py            # rewrite the blocks in place
    scripts/site-gen.py --check    # exit 1 if any block disagrees with the manifest

`content-sync.sh` runs --check, so a hand-edited generated block is a gate
failure rather than a drift nobody notices. The emitter prints markup in the
v2 class vocabulary (web/site/v2.css); the manifest is authored, not uploaded,
so escaping is limited to what html/json require.
"""

from __future__ import annotations

import html
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
MANIFEST = WEB / "works.json"

BLOCKS: dict[str, pathlib.Path] = {
    "works-featured": WEB / "index.html",
    "works-channels": WEB / "demos" / "index.html",
    "works-data": WEB / "demos" / "index.html",
    "works-catalog": WEB / "demos" / "index.html",
    "works-gallery": WEB / "gallery.html",
}


def load_works() -> list[dict]:
    return json.loads(MANIFEST.read_text())["works"]


def e(value: str) -> str:
    return html.escape(value, quote=True)


# ── shared fragments ────────────────────────────────────────────────────────


def tile(w: dict, prefix: str, hover: bool = True) -> list[str]:
    play = " data-hoverplay" if hover else " autoplay loop hold=\"1.5\""
    out = [
        f'''        <a class="work-tile" data-palette="{e(w['palette'])}" href="{prefix}{e(w['deck'])}">''',
        '''          <div class="stage-frame">''',
        f'''            <amx-player src="{prefix}{e(w['poster'])}" sealed{play} aspect="16:9"''',
        f'''                        title="{e(w['title'])}"></amx-player>''',
        '''          </div>''',
        f'''          <span class="kicker">{e(w['kicker'])} · {w['scenes']} scenes</span>''',
        f'''          <h3>{e(w['title'])}</h3>''',
        f'''          <p>{e(w['blurb'])}</p>''',
        '''          <div class="tag-strip">''',
    ]
    out += [f'''            <span class="tag-chip">{e(t)}</span>''' for t in w["tags"][:3]]
    out += [
        '''          </div>''',
        '''        </a>''',
    ]
    return out


# ── emitters ────────────────────────────────────────────────────────────────


def emit_featured(works: list[dict]) -> str:
    flag = next(w for w in works if w.get("flagship"))
    rest = [w for w in works if w is not flag][:3]
    out = [
        f'''      <div class="work-feature" data-palette="{e(flag['palette'])}">''',
        '''        <div class="stage-frame">''',
        f'''          <amx-player src="./demos/{e(flag['poster'])}" sealed autoplay loop hold="1.5" aspect="16:9"''',
        f'''                      title="{e(flag['title'])}"></amx-player>''',
        '''        </div>''',
        '''        <div class="work-feature-copy">''',
        f'''          <span class="kicker">{e(flag['kicker'])} · {flag['scenes']} scenes · flagship</span>''',
        f'''          <h3>{e(flag['title'])}</h3>''',
        f'''          <p>{e(flag['blurb'])}</p>''',
        '''          <div class="tag-strip">''',
    ]
    out += [f'''            <span class="tag-chip">{e(t)}</span>''' for t in flag["tags"]]
    out += [
        '''          </div>''',
        f'''          <span><a class="btn primary" href="./demos/{e(flag['deck'])}">Launch walkthrough →</a></span>''',
        '''        </div>''',
        '''      </div>''',
        '''      <div class="work-grid">''',
    ]
    for w in rest:
        out += tile(w, "./demos/")
    out.append('''      </div>''')
    return "\n".join(out)


def emit_channels(works: list[dict]) -> str:
    out = []
    for i, w in enumerate(works):
        selected = "true" if i == 0 else "false"
        out += [
            f'''        <button type="button" data-channel="{i}" role="tab" aria-selected="{selected}" data-palette="{e(w['palette'])}">''',
            f'''          <span class="n">{i + 1:02d}</span>''',
            f'''          <span>{e(w['short'])}</span>''',
            f'''          <span class="n">{e(w['kicker'])}</span>''',
            '''        </button>''',
        ]
    return "\n".join(out)


def emit_data(works: list[dict]) -> str:
    payload = [
        {
            "id": w["id"],
            "src": w["poster"],
            "file": w["poster"],
            "kicker": f"{w['kicker']} · {w['scenes']} Scenes"
            + (" · Flagship" if w.get("flagship") else ""),
            "title": w["title"],
            "desc": w["blurb"],
            "link": f"./{w['deck']}",
            "palette": w["palette"],
            "params": w.get("params", []),
        }
        for w in works
    ]
    body = json.dumps(payload, indent=2, ensure_ascii=False)
    indented = "\n".join("    " + line for line in body.splitlines())
    return f'''    <script type="application/json" id="works-data">\n{indented}\n    </script>'''


def emit_catalog(works: list[dict]) -> str:
    out = ['''      <div class="work-grid" id="catalog-grid">''']
    for w in works:
        out += tile(w, "")
        out.append('''''')
    out.append('''      </div>''')
    return "\n".join(out).rstrip()


def emit_gallery(works: list[dict]) -> str:
    out = ['''    <div class="figure-grid">''']
    for w in works:
        for fig in w.get("figures", []):
            out += [
                f'''      <figure data-palette="{e(w['palette'])}">''',
                '''        <div class="figure-head">''',
                f'''          <h3>{e(fig['title'])}</h3>''',
                f'''          <span>{e(w['kicker'])}</span>''',
                '''        </div>''',
                '''        <div class="stage-frame">''',
                f'''          <amx-player src="./demos/{e(fig['scene'])}"''',
                '''                      autoplay loop controls hold="1" aspect="16:9"''',
                f'''                      title="{e(fig['title'])}"></amx-player>''',
                '''        </div>''',
                '''        <figcaption>''',
                f'''          <span>{e(fig['caption'])}</span>''',
                f'''          <a href="./demos/{e(fig['scene'])}">{e(pathlib.Path(fig['scene']).name)} ↗</a>''',
                '''        </figcaption>''',
                '''      </figure>''',
            ]
    out.append('''    </div>''')
    return "\n".join(out)


EMITTERS = {
    "works-featured": emit_featured,
    "works-channels": emit_channels,
    "works-data": emit_data,
    "works-catalog": emit_catalog,
    "works-gallery": emit_gallery,
}


# ── per-deck parameter rails ────────────────────────────────────────────────
# Each walkthrough deck page carries a `works-params` block: one slider row per
# build-time `let` the work's scenes expose (web/DESIGN.md §3). deck.js wires
# the inputs to `player.setVars`, which rebuilds in place through the engine's
# one `--set` mechanism. A work with no params generates an empty block.


def emit_params(works: list[dict], page: pathlib.Path) -> str:
    work_id = page.parent.name
    work = next((w for w in works if w["id"] == work_id), None)
    params = (work or {}).get("params") or []
    if not params:
        return ""
    out = [
        f'''    <div class="param-rail" data-work="{e(work_id)}" data-palette="{e(work['palette'])}">''',
        '''      <span class="readout">Live parameters · rebuilds the scenes in place</span>''',
    ]
    for p in params:
        name = e(p["name"])
        out += [
            '''      <div class="param-row">''',
            f'''        <label for="var-{name}">{name}</label>''',
            f'''        <input id="var-{name}" type="range" min="{p['min']}" max="{p['max']}" step="{p['step']}" value="{p['default']}" data-var="{name}">''',
            f'''        <output>{p['default']}</output>''',
            '''      </div>''',
        ]
    out.append('''    </div>''')
    return "\n".join(out)


PARAM_PAGES = [WEB / "demos" / w / "index.html" for w in (
    "transformer", "topology", "epicycles", "sorting", "matrix",
    "gradient", "morph", "kinetic", "hash",
)]


# ── splice ──────────────────────────────────────────────────────────────────


def splice(page: pathlib.Path, block: str, body: str) -> tuple[str, bool]:
    text = page.read_text()
    start = f"<!-- site-gen:{block}:start -->"
    end = f"<!-- site-gen:{block}:end -->"
    if start not in text or end not in text:
        return text, False
    head, _, tail = text.partition(start)
    _, _, rest = tail.partition(end)
    return head + start + "\n" + body + "\n" + end + rest, True


def main() -> int:
    check = "--check" in sys.argv
    works = load_works()
    failed = False
    jobs: list[tuple[str, pathlib.Path, str]] = [
        (block, page, EMITTERS[block](works)) for block, page in BLOCKS.items()
    ]
    jobs += [("works-params", page, emit_params(works, page)) for page in PARAM_PAGES]
    for block, page, body in jobs:
        if not page.exists():
            print(f"skip: {page.relative_to(ROOT)} missing (block {block} not wired yet)")
            continue
        new_text, found = splice(page, block, body)
        if not found:
            print(f"skip: {page.relative_to(ROOT)} has no {block} markers yet")
            continue
        if check:
            if new_text != page.read_text():
                print(f"FAIL: {page.relative_to(ROOT)} block {block} disagrees with works.json"
                      f" — run: scripts/site-gen.py", file=sys.stderr)
                failed = True
            else:
                print(f"ok: {page.relative_to(ROOT)} block {block} matches works.json")
        else:
            page.write_text(new_text)
            print(f"wrote: {page.relative_to(ROOT)} block {block}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
