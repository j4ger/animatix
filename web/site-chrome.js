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

(function () {
  const script = document.currentScript;
  const root = ((script?.dataset.root ?? "./").replace(/\/*$/, "/"));
  const section = script?.dataset.section ?? "";
  const REDUCED = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

  // ── nav + footer ──────────────────────────────────────────────────

  const external = "https://github.com/j4ger/animatix";
  const links = [
    ["", "Home", "home"],
    ["demos/", "Showcase", "demos"],
    ["tour/", "Tour", "tour"],
    ["arena/", "Arena", "arena"],
    [external, "GitHub ↗", "github"],
  ];

  const nav = document.createElement("nav");
  nav.className = "site";
  const inner = document.createElement("div");
  inner.className = "site-inner";
  const wordmark = document.createElement("a");
  wordmark.className = "wordmark";
  wordmark.href = root;
  wordmark.setAttribute("aria-label", "Animatix home");
  const mark = document.createElement("img");
  mark.src = root + "logo.svg";
  mark.alt = ""; // The adjacent wordmark already names the home link.
  mark.width = 32;
  mark.height = 32;
  const name = document.createElement("span");
  name.innerHTML = "Animatix<i>.</i>";
  wordmark.append(mark, name);
  const linksBox = document.createElement("div");
  linksBox.className = "site-links";
  for (const [href, label, key] of links) {
    const a = document.createElement("a");
    a.href = key === "github" ? href : root + href;
    a.textContent = label;
    if (key === "github") {
      a.className = "github-star";
      a.target = "_blank";
      a.rel = "noopener noreferrer";
    }
    if (key === section || (key === "demos" && (section === "gallery" || section === "recipes"))) {
      a.setAttribute("aria-current", "page");
    }
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
  nav.append(inner, ruler);
  document.body.prepend(nav);

  // A page may split its body across several .wrap bands (full-bleed stages
  // between them). The footer and the section ruler belong to the LAST band,
  // not the first: appending to the first used to bury the rest of the page
  // under the footer.
  const wraps = document.querySelectorAll(".wrap");
  const wrap = wraps.length ? wraps[wraps.length - 1] : null;
  if (!wrap) return;
  const footer = document.createElement("footer");
  const left = document.createElement("span");
  left.innerHTML =
    'Built with <strong>Animatix</strong> — declarative animation DSL & 60 FPS Rust engine. MIT licensed.';
  const right = document.createElement("span");
  const parts = [
    ["", "Home"],
    ["tour/", "Language"],
    ["demos/", "Demos"],
    ["gallery.html", "Gallery"],
    ["recipes/", "Recipes"],
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
  gh.target = "_blank";
  gh.rel = "noopener noreferrer";
  right.append(gh);
  footer.append(left, right);
  wrap.appendChild(footer);

  // ── copy to clipboard support ─────────────────────────────────────
  document.querySelectorAll("button[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.preventDefault();
      const text = btn.dataset.copy;
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        btn.classList.add("copied");
        const origHtml = btn.innerHTML;
        btn.innerHTML = `<span class="term-prompt">✓</span> Copied to clipboard!`;
        setTimeout(() => {
          btn.classList.remove("copied");
          btn.innerHTML = origHtml;
        }, 2200);
      } catch (err) {
        console.warn("copy failed", err);
      }
    });
  });

  // ── the master timeline ───────────────────────────────────────────
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
    if (!head || head.querySelector(".stamp")) return;
    const stamp = document.createElement("span");
    stamp.className = "stamp";
    stamp.textContent = fmt(BEAT * (i + 1));
    stamp.title = "this section's keyframe on the page's master timeline";
    head.appendChild(stamp);
  });

  let ticking = false;
  const cue = document.querySelector(".scroll-cue");
  function measure() {
    const doc = document.documentElement;
    const maxScroll = Math.max(doc.scrollHeight - window.innerHeight, 1);
    for (const { el, tick } of ticks) {
      const arrive = Math.min(Math.max((el.offsetTop - 56) / maxScroll, 0), 1);
      tick.style.left = `${arrive * 100}%`;
    }
  }

  function sync() {
    ticking = false;
    const doc = document.documentElement;
    const maxScroll = Math.max(doc.scrollHeight - window.innerHeight, 1);
    const p = Math.min(Math.max(window.scrollY / maxScroll, 0), 1);
    rulerFill.style.transform = `scaleX(${p})`;
    rulerHead.style.setProperty("--head", `${p * 100}%`);
    timecode.textContent = fmt(p * duration);
    if (cue) {
      const gone = Math.min(Math.max((window.scrollY - 60) / 200, 0), 1);
      cue.style.opacity = String(1 - gone);
    }
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
  window.addEventListener("load", measure);
  measure();
  sync();

  // ── player readiness markers ──────────────────────────────────────
  // Pages paint a `.poster-plate` under each player; CSS needs to know when
  // the first real frame has arrived so the plate can fade out. Errors also
  // count as "done" so a retry veil is never hidden behind the plate.
  for (const el of document.querySelectorAll("amx-player")) {
    el.addEventListener("amxready", () => { el.dataset.ready = "1"; }, { once: true });
    el.addEventListener("amxerror", () => { el.dataset.error = "1"; }, { once: true });
  }

  // ── hover-play & touch-toggle posters ─────────────────────────────
  for (const el of document.querySelectorAll("amx-player[data-hoverplay]")) {
    const card = el.closest("a.hub-card, a.demo-card, .hub-card, .demo-card, a.work-tile, .work-tile") ?? el;
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

  // ── ensure ledger rows are always visible ─────────────────────────
  const rows = document.querySelectorAll(".ledger .row");
  if (rows.length) {
    rows.forEach((r) => r.classList.add("in"));
  }

  // ── theater mode (gallery) ────────────────────────────────────────
  for (const figure of document.querySelectorAll(".gallery figure, figure.tour-embed, .gallery-grid figure, .recipes-grid figure, .figure-grid figure")) {
    const caption = figure.querySelector("figcaption");
    if (!caption || !document.fullscreenEnabled) continue;
    if (caption.querySelector(".theater-btn")) continue;
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "theater-btn";
    btn.textContent = "theater ⤢";
    btn.setAttribute("aria-label", "Fullscreen this figure");
    btn.addEventListener("click", () => {
      if (document.fullscreenElement) document.exitFullscreen();
      else figure.requestFullscreen?.();
    });
    const actions = caption.querySelector(".fig-actions") ?? caption;
    actions.appendChild(btn);
  }

  // ── the duality: scroll-scrubbed + click-interactive code/stage ───
  const duality = document.querySelector(".duality");
  if (duality) {
    const player = duality.querySelector("amx-player");
    const lines = [...duality.querySelectorAll("pre .line[data-t]")];
    const beatOut = duality.querySelector(".stage-note .beat");
    let ready = false;
    player?.addEventListener("amxready", () => { ready = true; scrub(); });

    // Click line to jump to beat
    lines.forEach((ln) => {
      ln.addEventListener("click", () => {
        if (!player) return;
        const [a] = ln.dataset.t.split(",").map(Number);
        player.seek(a + 0.05);
        if (beatOut) beatOut.textContent = fmt(a);
        lines.forEach((other) => other.classList.toggle("live", other === ln));
      });
    });

    const clamp01 = (x) => Math.min(Math.max(x, 0), 1);
    let lastScrubT = -1;
    let lastScrubAt = -Infinity;
    let scrubTimer = 0;
    let scrubQueued = false;
    function scrub() {
      scrubQueued = false;
      if (!ready || !player) return;
      const r = duality.getBoundingClientRect();
      const vh = window.innerHeight;
      // Scrolling anywhere else on the page used to seek this offscreen player
      // and force a full scene render per scroll frame. Only scrub when the
      // duality is actually in view, and skip sub-frame deltas.
      if (r.bottom < -vh * 0.1 || r.top > vh * 1.1) return;
      const span = r.height + vh * 0.5;
      const p = clamp01((vh * 0.85 - r.top) / span);
      const t = p * player.duration;
      if (Math.abs(t - lastScrubT) < 1 / 120) return;
      lastScrubT = t;
      // `seek()` renders the frame synchronously. During a scroll that is a
      // GPU frame wedged into every scroll tick; defer the draw to the settle
      // point and only update the editorial state now.
      if (performance.now() - lastScrubAt < 160) {
        clearTimeout(scrubTimer);
        scrubTimer = setTimeout(() => { player.seek(t); }, 180);
      } else {
        player.seek(t);
      }
      if (beatOut) beatOut.textContent = fmt(t);
      for (const ln of lines) {
        const [a, b] = ln.dataset.t.split(",").map(Number);
        ln.classList.toggle("live", t >= a && t < b);
      }
    }
    const queueScrub = () => {
      // One rAF per frame at most. A scroll event fires more often than the
      // compositor paints; without this guard every event forced its own
      // getBoundingClientRect() layout read and seek.
      if (scrubQueued) return;
      scrubQueued = true;
      requestAnimationFrame(scrub);
    };
    window.addEventListener("scroll", () => {
      lastScrubAt = performance.now();
      queueScrub();
    }, { passive: true });
    window.addEventListener("resize", queueScrub, { passive: true });
  }
})();
