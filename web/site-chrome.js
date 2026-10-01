// Site chrome — the shared header, footer, and the master-timeline furniture.
//
// Pages carry a single script tag instead of hand-copied nav markup:
//
//   <script src="../site-chrome.js" data-root="../" data-section="tour"></script>
//
// `data-root` is the site root relative to the page (so links resolve at any
// depth), `data-section` (optional) marks the current nav link. The script is
// parser-blocking at the end of <body>, so the chrome is present before first
// paint — no flash, no layout shift.
//
// Beyond nav + footer, this file wires the "the page is a timeline" layer:
//
//   ruler      the amber playhead in the nav's bottom edge — scroll progress
//              as scaleX, one tick per section, a diamond at the head
//   timecode   the `#12.4s` readout in the nav (the page's master clock: one
//              beat of BEAT seconds per section)
//   stamps     each section head gets its master-time stamp (`#8.0s`)
//   hover-play `amx-player[data-hoverplay]` posters play on card hover/focus
//   ledger     `.ledger .row` entries stamp in as they enter the viewport
//   duality    the home page's code/stage figure scrubs with scroll and
//              lights the code line owning the current beat

(function () {
  const script = document.currentScript;
  const root = ((script?.dataset.root ?? "./").replace(/\/*$/, "/"));
  const section = script?.dataset.section ?? "";
  const REDUCED = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

  // ── nav + footer ──────────────────────────────────────────────────

  const external = "https://github.com/j4ger/animatix";
  const links = [
    ["", "Home", "home"],
    ["tour/", "Language", "tour"],
    ["demos/", "Demos", "demos"],
    ["gallery.html", "Gallery", "gallery"],
    [external, "GitHub ↗", "github"],
  ];

  const nav = document.createElement("nav");
  nav.className = "site";
  const inner = document.createElement("div");
  inner.className = "site-inner";
  const wordmark = document.createElement("a");
  wordmark.className = "wordmark";
  wordmark.href = root;
  wordmark.innerHTML = "Animatix<i>.</i>";
  const linksBox = document.createElement("div");
  linksBox.className = "site-links";
  for (const [href, label, key] of links) {
    const a = document.createElement("a");
    a.href = key === "github" ? href : root + href;
    a.textContent = label;
    if (key === section) a.setAttribute("aria-current", "page");
    linksBox.appendChild(a);
  }
  const timecode = document.createElement("span");
  timecode.className = "timecode";
  timecode.textContent = "#0.0s";
  inner.append(wordmark, linksBox, timecode);

  // The ruler: track + playhead fill + diamond head live at the nav's bottom
  // edge; section ticks are positioned once sections have been measured.
  const ruler = document.createElement("div");
  ruler.className = "ruler";
  ruler.setAttribute("aria-hidden", "true");
  const rulerTrack = document.createElement("div");
  rulerTrack.className = "ruler-track";
  const rulerFill = document.createElement("div");
  rulerFill.className = "ruler-fill";
  const rulerHead = document.createElement("div");
  rulerHead.className = "ruler-head";
  ruler.append(rulerTrack, rulerFill, rulerHead);
  nav.append(ruler);
  document.body.prepend(nav);

  const wrap = document.querySelector(".wrap");
  if (!wrap) return;
  const footer = document.createElement("footer");
  const left = document.createElement("span");
  left.innerHTML =
    'Built with <strong>Animatix</strong> — a layout-first animation DSL. MIT licensed.';
  const right = document.createElement("span");
  const parts = [
    ["", "Home"],
    ["tour/", "Language"],
    ["demos/", "Demos"],
    ["gallery.html", "Gallery"],
  ];
  parts.forEach(([href, label], i) => {
    if (i > 0) right.append(" · ");
    const a = document.createElement("a");
    a.href = root + href;
    a.textContent = label;
    right.append(a);
  });
  right.append(" · ");
  const gh = document.createElement("a");
  gh.href = external;
  gh.textContent = "GitHub";
  right.append(gh);
  footer.append(left, right);
  wrap.appendChild(footer);

  // ── the master timeline ───────────────────────────────────────────
  // Each top-level section occupies one BEAT-second beat of the page's master
  // timeline; the hero/header happens at #0.0s. Scroll position is the
  // playhead.

  const BEAT = 4;
  const sections = [...wrap.querySelectorAll(":scope > section[id], :scope > div > section[id]")];
  const duration = BEAT * (sections.length + 1);
  const fmt = (t) => `#${t.toFixed(1).replace(/\.0$/, ".0")}s`;

  const ticks = sections.map((el) => {
    const tick = document.createElement("div");
    tick.className = "ruler-tick";
    ruler.appendChild(tick);
    return { el, tick };
  });

  // Stamp each section head with its master time.
  sections.forEach((el, i) => {
    const head = el.querySelector(".sec-head");
    if (!head) return;
    const stamp = document.createElement("span");
    stamp.className = "stamp";
    stamp.textContent = fmt(BEAT * (i + 1));
    stamp.title = "this section's keyframe on the page's master timeline";
    head.appendChild(stamp);
  });

  let ticking = false;
  function measure() {
    const doc = document.documentElement;
    const maxScroll = Math.max(doc.scrollHeight - window.innerHeight, 1);
    for (const { el, tick } of ticks) {
      // The tick sits where the section's top reaches the nav line.
      const arrive = Math.min(Math.max((el.offsetTop - 54) / maxScroll, 0), 1);
      tick.style.left = `${arrive * 100}%`;
    }
  }

  function sync() {
    ticking = false;
    const doc = document.documentElement;
    const maxScroll = Math.max(doc.scrollHeight - window.innerHeight, 1);
    const p = Math.min(Math.max(window.scrollY / maxScroll, 0), 1);
    rulerFill.style.transform = `scaleX(${p})`;
    rulerHead.style.left = `${p * 100}%`;
    timecode.textContent = fmt(p * duration);
  }
  function onScroll() {
    if (!ticking) {
      ticking = true;
      requestAnimationFrame(sync);
    }
  }

  timecode.classList.add("on");
  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", () => { measure(); onScroll(); }, { passive: true });
  // Fonts and images change offsets; re-measure once they settle.
  window.addEventListener("load", measure);
  measure();
  sync();

  // ── hover-play posters ────────────────────────────────────────────
  // A `data-hoverplay` embed rests on its finished frame (the player's
  // built-in poster) and plays while its card is hovered or focused. Used by
  // the demo hub's live posters; autoplaying everything at once would spend
  // the frame budget on cards nobody is reading.

  for (const el of document.querySelectorAll("amx-player[data-hoverplay]")) {
    const card = el.closest("a.hub-card, a.demo-card, .hub-card, .demo-card") ?? el;
    const whenReady = new Promise((res) => {
      if (el._state === "ready") res();
      else el.addEventListener("amxready", () => res(), { once: true });
    });
    const play = () => { whenReady.then(() => el.play()); };
    const pause = () => el.pause();
    card.addEventListener("pointerenter", play);
    card.addEventListener("pointerleave", pause);
    card.addEventListener("focus", play, true);
    card.addEventListener("blur", pause, true);
  }

  // ── ledger stamp-in ───────────────────────────────────────────────

  const rows = document.querySelectorAll(".ledger .row");
  if (rows.length) {
    if (REDUCED) {
      rows.forEach((r) => r.classList.add("in"));
    } else {
      const io = new IntersectionObserver(
        (entries) => {
          for (const e of entries) {
            if (e.isIntersecting) {
              e.target.classList.add("in");
              io.unobserve(e.target);
            }
          }
        },
        { rootMargin: "0px 0px -8% 0px" },
      );
      rows.forEach((r) => io.observe(r));
    }
  }

  // ── the duality: scroll-scrubbed code/stage figure ────────────────
  // Present only on the home page. Scrolling through the section scrubs the
  // stage across its whole timeline; the code line owning the current beat
  // (each line carries data-t="start,end" in scene seconds) lights up.

  const duality = document.querySelector(".duality");
  if (duality) {
    const player = duality.querySelector("amx-player");
    const lines = [...duality.querySelectorAll("pre .line[data-t]")];
    const beatOut = duality.querySelector(".stage-note .beat");
    let ready = false;
    player?.addEventListener("amxready", () => { ready = true; scrub(); });

    const clamp01 = (x) => Math.min(Math.max(x, 0), 1);
    function scrub() {
      if (!ready || !player) return;
      const r = duality.getBoundingClientRect();
      const vh = window.innerHeight;
      // 0 when the figure's top crosses 85% of the viewport, 1 when its
      // bottom crosses 35% — a scrub window roughly the section's own height.
      const span = r.height + vh * 0.5;
      const p = clamp01((vh * 0.85 - r.top) / span);
      const t = p * player.duration;
      player.seek(t);
      if (beatOut) beatOut.textContent = fmt(t);
      for (const ln of lines) {
        const [a, b] = ln.dataset.t.split(",").map(Number);
        ln.classList.toggle("live", t >= a && t < b);
      }
    }
    window.addEventListener("scroll", () => {
      requestAnimationFrame(scrub);
    }, { passive: true });
    window.addEventListener("resize", () => requestAnimationFrame(scrub), { passive: true });
  }
})();
