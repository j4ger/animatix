// Site chrome — one shared header and footer for every page.
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
  inner.append(wordmark, linksBox);
  nav.append(inner);
  document.body.prepend(nav);

  const wrap = document.querySelector(".wrap");
  if (!wrap) return;
  const footer = document.createElement("footer");
  const left = document.createElement("span");
  left.innerHTML =
    'Built with <strong style="color:var(--dim)">Animatix</strong> — a layout-first animation DSL. MIT licensed.';
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
})();
