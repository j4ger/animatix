#!/usr/bin/env python3
"""Regenerate the manifest-driven blocks inside the portfolio pages.

`web/works.json` is the single source of truth for every showcased work. The
pages carry generated regions delimited by markers:

    <!-- site-gen:<block>:start -->
    …generated html…
    <!-- site-gen:<block>:end -->

Supported blocks, per page:

    index.html         works-featured   spotlight card + three supplementary cards
    demos/index.html   works-channels   theater channel switcher buttons
    demos/index.html   works-data       <script type="application/json"> payload
    demos/index.html   works-catalog    the filterable catalog grid
    gallery.html       works-gallery    the standalone figure grid

Usage:
    scripts/site-gen.py            # rewrite the blocks in place
    scripts/site-gen.py --check    # exit 1 if any block disagrees with the manifest

`content-sync.sh` runs --check, so a hand-edited generated block is a gate
failure rather than a drift nobody notices. The emitter is deliberately dumb:
it prints markup in the site's existing class vocabulary and escapes nothing
but what json/html require, because the manifest is authored, not uploaded.
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


# ── emitters ────────────────────────────────────────────────────────────────


def emit_featured(works: list[dict]) -> str:
    flag = next(w for w in works if w.get("flagship"))
    rest = [w for w in works if w is not flag][:3]
    out = [
        f'''      <a class="demo-spotlight" href="./demos/{e(flag['deck'])}" data-work="{e(flag['id'])}">''',
        '''        <div class="spotlight-poster">''',
        f'''          <span class="poster-badge live">Live Scene · {flag['scenes']} scenes</span>''',
        f'''          <amx-player src="./demos/{e(flag['poster'])}" sealed autoplay loop''',
        '''                      hold="1.5" aspect="16:9"'''
        f''' title="{e(flag['title'])}"></amx-player>''',
        '''        </div>''',
        '''        <div class="spotlight-info">''',
        f'''          <span class="demo-tag">{e(flag['kicker'])} · {flag['scenes']} Scenes</span>''',
        f'''          <h3 class="spotlight-title">{e(flag['title'])}</h3>''',
        f'''          <p class="spotlight-blurb">{e(flag['blurb'])}</p>''',
        '''          <span class="spotlight-action">Launch walkthrough →</span>''',
        '''        </div>''',
        '''      </a>''',
        '''      <div class="demos-grid">''',
    ]
    for w in rest:
        out += [
            f'''        <a class="demo-card" href="./demos/{e(w['deck'])}" data-work="{e(w['id'])}">''',
            '''          <div class="demo-poster">''',
            '''            <span class="poster-badge">Hover to play</span>''',
            f'''            <amx-player src="./demos/{e(w['poster'])}" sealed data-hoverplay''',
            f'''                        aspect="16:9" title="{e(w['title'])}"></amx-player>''',
            '''          </div>''',
            '''          <div class="demo-content">''',
            f'''            <span class="demo-tag">{e(w['kicker'])} · {w['scenes']} Scenes</span>''',
            f'''            <h4>{e(w['title'])}</h4>''',
            f'''            <p>{e(w['blurb'])}</p>''',
            '''            <span class="demo-link">Explore walkthrough →</span>''',
            '''          </div>''',
            '''        </a>''',
        ]
    out.append('''      </div>''')
    return "\n".join(out)


def emit_channels(works: list[dict]) -> str:
    out = []
    for i, w in enumerate(works):
        active = " active" if i == 0 else ""
        selected = "true" if i == 0 else "false"
        out += [
            f'''        <button class="channel-btn{active}" type="button" data-channel="{i}" role="tab" aria-selected="{selected}">''',
            f'''          <span class="channel-num">{i + 1:02d}</span>''',
            '''          <span class="channel-info">''',
            f'''            <span class="channel-title">{e(w['short'])}</span>''',
            f'''            <span class="channel-tag">{e(w['kicker'])}</span>''',
            '''          </span>''',
            '''        </button>''',
            '''''',
        ]
    return "\n".join(out).rstrip()


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
    out = []
    for w in works:
        featured = " featured" if w.get("flagship") else ""
        badge = (
            f"Live Scene · {w['scenes']}s" if w.get("flagship")
            else f"Hover to play · {w['scenes']} scene{'s' if w['scenes'] != 1 else ''}"
        )
        autoplay = " autoplay loop\n                        hold=\"1.5\"" if w.get("flagship") else " data-hoverplay"
        out += [
            f'''        <article class="catalog-card{featured}" data-category="{e(w['category'])}" data-work="{e(w['id'])}" data-palette="{e(w['palette'])}">''',
            '''          <div class="catalog-poster">''',
            f'''            <span class="catalog-badge{' live' if w.get('flagship') else ''}">{e(badge)}</span>''',
            f'''            <amx-player src="{e(w['poster'])}" sealed{autoplay} aspect="16:9"''',
            f'''                        title="{e(w['title'])}"></amx-player>''',
            '''          </div>''',
            '''          <div class="catalog-content">''',
            f'''            <span class="catalog-kicker">{e(w['kicker'])} · {w['scenes']} Scenes{' · Flagship Deck' if w.get('flagship') else ''}</span>''',
            f'''            <h3 class="catalog-title">{e(w['title'])}</h3>''',
            f'''            <p class="catalog-blurb">{e(w['blurb'])}</p>''',
            '''            <div class="catalog-tags-strip">''',
        ]
        out += [f'''              <span class="tag-chip">{e(t)}</span>''' for t in w["tags"]]
        out += [
            '''            </div>''',
            '''            <div class="catalog-card-actions">''',
            f'''              <a class="btn-catalog-deck" href="./{e(w['deck'])}">Launch Deck ⊞</a>''',
            f'''              <a class="btn-catalog-src" href="{e(w['poster'])}" target="_blank" rel="noopener noreferrer">Source ↗</a>''',
            '''            </div>''',
            '''          </div>''',
            '''        </article>''',
            '''''',
        ]
    return "\n".join(out).rstrip()


def emit_gallery(works: list[dict]) -> str:
    out = []
    for w in works:
        for fig in w.get("figures", []):
            out += [
                '''      <figure data-work="{0}">'''.format(e(w["id"])),
                '''        <div class="figure-header">''',
                f'''          <h3 class="figure-title">{e(fig['title'])}</h3>''',
                f'''          <span class="figure-tag">{e(w['kicker'])}</span>''',
                '''        </div>''',
                f'''        <amx-player src="./demos/{e(fig['scene'])}"''',
                '''                    autoplay loop controls hold="1" aspect="16:9"''',
                f'''                    title="{e(fig['title'])}"></amx-player>''',
                '''        <figcaption>''',
                f'''          <span class="cap">{e(fig['caption'])}</span>''',
                '''          <div class="fig-actions">''',
                f'''            <a class="src" href="./demos/{e(fig['scene'])}">{e(pathlib.Path(fig['scene']).name)} ↗</a>''',
                '''          </div>''',
                '''        </figcaption>''',
                '''      </figure>''',
                '''''',
            ]
    return "\n".join(out).rstrip()


EMITTERS = {
    "works-featured": emit_featured,
    "works-channels": emit_channels,
    "works-data": emit_data,
    "works-catalog": emit_catalog,
    "works-gallery": emit_gallery,
}


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
    for block, page in BLOCKS.items():
        body = EMITTERS[block](works)
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
