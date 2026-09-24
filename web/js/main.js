// Animatix web demo shell: modes, transport, diagnostics, examples.
// The engine (animatix-web wasm) renders every frame through WebGPU; this
// file owns only UI state and orchestration.

import { EXAMPLES, find } from "./examples.js";
import { createEditor, editorApi } from "./editor.js";

const $ = (id) => document.getElementById(id);

const els = {
  status: $("engine-status"),
  exampleSelect: $("example-select"),
  editorHost: $("editor"),
  diagSummary: $("diag-summary"),
  diagToggle: $("diag-toggle"),
  diagList: $("diag-list"),
  canvasFrame: $("canvas-frame"),
  canvas: $("stage"),
  staleChip: $("stale-chip"),
  canvasHint: $("canvas-hint"),
  btnPlay: $("btn-play"),
  iconPlay: $("icon-play"),
  btnRestart: $("btn-restart"),
  timeReadout: $("time-readout"),
  scrubber: $("scrubber"),
  btnLoop: $("btn-loop"),
  fpsBadge: $("fps-badge"),
  dimsBadge: $("dims-badge"),
  buildBadge: $("build-badge"),
  divider: $("divider"),
  modeEditor: $("mode-editor"),
  modePlayer: $("mode-player"),
  playerstage: $("playerstage"),
  playerCanvasHost: $("player-canvas-host"),
  playerBack: $("player-back"),
  pbtnPlay: $("pbtn-play"),
  piconPlay: $("picon-play"),
  pscrubber: $("pscrubber"),
  ptimeReadout: $("ptime-readout"),
  playerControls: $("player-controls"),
  bootOverlay: $("boot-overlay"),
  bootTitle: $("boot-title"),
  bootDetail: $("boot-detail"),
  failOverlay: $("fail-overlay"),
  failTitle: $("fail-title"),
  failDetail: $("fail-detail"),
  failTech: $("fail-tech"),
};

const EDIT_DEBOUNCE_MS = 260;
const FPS_WINDOW_MS = 500;

// Automated smoke probe (?probe=1): after 30 rendered frames, sample the
// canvas pixels and publish stats on window.__probe_state for the CDP driver.
// ?fill=1 instead bypasses the engine and raw-clears the canvas green —
// bisects present-chain problems from scene-content problems.
const PROBE = new URLSearchParams(location.search).get("probe") === "1";
const PROBE_FILL = new URLSearchParams(location.search).get("fill") === "1";
// Baseline that bypasses the engine entirely: raw WebGPU clear from JS.
const PROBE_JSFILL = new URLSearchParams(location.search).get("jsfill") === "1";
// GPU-side readback of the rendered scene, independent of the compositor.
const PROBE_READBACK = new URLSearchParams(location.search).get("readback") === "1";

const app = {
  player: null,
  editor: null,
  engineMod: null,
  mode: "edit",
  exampleId: null,
  playing: true,
  loop: true,
  time: 0,
  duration: 0,
  sceneW: 16,
  sceneH: 9,
  buildMs: 0,
  stale: false,
  renderDirty: true,
  fps: { rendered: 0, since: performance.now(), value: 0 },
  lastErrorShown: null,
  diagnostics: [],
};

// ── Boot ────────────────────────────────────────────────────────────

function setBoot(title, detail) {
  els.bootTitle.textContent = title;
  if (detail) els.bootDetail.textContent = detail;
}

function fail(title, detail, err) {
  els.bootOverlay.classList.add("hidden");
  els.failTitle.textContent = title;
  els.failDetail.textContent = detail;
  els.failTech.textContent = err ? String(err).slice(0, 500) : "";
  els.failOverlay.classList.remove("hidden");
}

async function boot() {
  fillExampleSelect();
  app.editor = createEditor(els.editorHost, { onEdit: scheduleRebuild });

  const params = new URLSearchParams(location.search);
  app.mode = params.get("mode") === "play" ? "play" : "edit";
  const requested = find(params.get("example")) ? params.get("example") : EXAMPLES[0].id;
  els.exampleSelect.value = requested;

  if (!("gpu" in navigator)) {
    fail(
      "WebGPU is required",
      "This demo renders through WebGPU, which this browser does not expose. " +
        "Try Chrome/Edge 113+, Firefox 141+, or Safari 26+.",
      `navigator.gpu: undefined — ${navigator.userAgent}`,
    );
    return;
  }

  setBoot("Loading Animatix", "Fetching the engine module — first visit downloads ~12 MB.");
  let mod;
  try {
    mod = await import("../pkg/animatix_web.js");
  } catch (err) {
    fail("Could not load the engine", "The wasm module failed to download or compile.", err);
    return;
  }
  app.engineMod = mod;
  try {
    await mod.default();
  } catch (err) {
    fail("Could not start the engine", "The wasm module failed to initialize.", err);
    return;
  }

  setBoot("Starting WebGPU", "Requesting an adapter and device…");
  els.status.textContent = "init webgpu…";
  try {
    app.player = await mod.create_player(els.canvas);
  } catch (err) {
    fail(
      "WebGPU initialization failed",
      "WebGPU is present but the renderer could not start. On Linux, a Vulkan-capable GPU or software renderer may be required.",
      err,
    );
    return;
  }

  enterMode(app.mode, { replace: false });
  if (!(PROBE && PROBE_FILL)) {
    await loadExample(requested);
  }

  els.bootOverlay.classList.add("hidden");
  els.status.textContent = "ready";
  console.info("animatix-web: player ready");

  window.addEventListener("resize", () => { fitCanvasPixels(); app.renderDirty = true; });
  requestAnimationFrame(tick);
}

// ── Examples & rebuild ──────────────────────────────────────────────

function fillExampleSelect() {
  for (const ex of EXAMPLES) {
    const option = document.createElement("option");
    option.value = ex.id;
    option.textContent = `${ex.title} — ${ex.hint}`;
    els.exampleSelect.appendChild(option);
  }
  els.exampleSelect.addEventListener("change", () => loadExample(els.exampleSelect.value));
}

async function loadExample(id) {
  const ex = find(id);
  if (!ex) return;
  app.exampleId = id;
  let source;
  try {
    const response = await fetch(`./pkg/examples/${ex.file}`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    source = await response.text();
  } catch (err) {
    fail("Could not load example", `Fetching ${ex.file} failed. Serve the site over HTTP (see web/README.md).`, err);
    return;
  }
  editorApi.setDoc(app.editor, source);
  app.time = 0;
  app.playing = true;
  syncPlayButtons();
  rebuildNow();
}

let rebuildTimer = null;
function scheduleRebuild() {
  clearTimeout(rebuildTimer);
  rebuildTimer = setTimeout(rebuildNow, EDIT_DEBOUNCE_MS);
}

function rebuildNow() {
  if (!app.player) return;
  const source = editorApi.getDoc(app.editor);
  const t0 = performance.now();
  let result;
  try {
    result = app.player.load_source(source);
  } catch (err) {
    console.error("animatix-web: load_source failed", err);
    return;
  }
  app.buildMs = performance.now() - t0;
  app.diagnostics = result.diagnostics ?? [];
  app.renderDirty = true;

  if (result.ok) {
    app.duration = Math.max(result.duration_s, 0.05);
    app.sceneW = result.width;
    app.sceneH = result.height;
    app.stale = false;
    if (app.time > app.duration) app.time = 0;
  } else {
    // Nothing replaced the previous document — flag the preview as stale
    // only if there is something previous to be stale from.
    app.stale = app.player.has_document();
  }

  fitCanvasPixels();
  renderDiagnostics();
  els.dimsBadge.textContent = `${app.sceneW}×${app.sceneH}`;
  els.buildBadge.textContent = `build ${app.buildMs.toFixed(0)} ms`;
  els.scrubber.max = Math.round(app.duration * 1000);
  els.pscrubber.max = els.scrubber.max;
  console.info(
    `animatix-web: build ok=${result.ok} diags=${app.diagnostics.length} ` +
      `${app.buildMs.toFixed(0)}ms`,
  );
}

// ── Diagnostics UI ──────────────────────────────────────────────────

function renderDiagnostics() {
  editorApi.setDiagnostics(app.editor, app.diagnostics);
  els.diagList.textContent = "";

  const errors = app.diagnostics.filter((d) => d.severity === "error").length;
  const warnings = app.diagnostics.filter((d) => d.severity === "warning").length;
  if (app.diagnostics.length === 0) {
    els.diagSummary.textContent = "no issues";
  } else {
    const parts = [];
    if (errors) parts.push(`${errors} error${errors > 1 ? "s" : ""}`);
    if (warnings) parts.push(`${warnings} warning${warnings > 1 ? "s" : ""}`);
    const others = app.diagnostics.length - errors - warnings;
    if (others > 0) parts.push(`${others} other`);
    els.diagSummary.textContent = parts.join(" · ");
  }

  if (app.diagnostics.length === 0) {
    const li = document.createElement("li");
    li.className = "diag-empty";
    li.textContent = "Scene builds cleanly.";
    els.diagList.appendChild(li);
    return;
  }
  for (const diag of app.diagnostics) {
    const li = document.createElement("li");
    li.className = `diag-row ${diag.severity}`;
    const sev = document.createElement("span");
    sev.className = "sev";
    const code = document.createElement("span");
    code.className = "code";
    code.textContent = diag.code || diag.severity;
    const msg = document.createElement("span");
    msg.className = "msg";
    msg.textContent = diag.subject ? `${diag.message} (${diag.subject})` : diag.message;
    const loc = document.createElement("span");
    loc.className = "loc";
    loc.textContent = diag.line ? `L${diag.line}${diag.column ? `:${diag.column}` : ""}` : "";
    li.append(sev, code, msg, loc);
    li.addEventListener("click", () => editorApi.jumpTo(app.editor, diag));
    els.diagList.appendChild(li);
  }
  els.staleChip.classList.toggle("hidden", !app.stale);
}

// ── Render loop ─────────────────────────────────────────────────────

let lastNow = performance.now();
let probeFrames = 0;
let probeSampled = false;

function tick(now) {
  const dt = Math.min((now - lastNow) / 1000, 0.1);
  lastNow = now;

  let rendered = false;
  if (app.playing && app.duration > 0) {
    app.time += dt;
    if (app.time >= app.duration) {
      if (app.loop) app.time %= app.duration;
      else { app.time = app.duration; setPlaying(false); }
    }
    rendered = renderFrame(app.time);
  } else if (app.renderDirty) {
    rendered = renderFrame(app.time);
  }

  if (PROBE) probeStep(rendered);
  updateTransport();
  updateFps(now, rendered);
  requestAnimationFrame(tick);
}

function probeStep(rendered) {
  if (PROBE_READBACK) {
    if (!probeSampled && app.player && app.player.has_document()) {
      probeSampled = true;
      const buildId = app.engineMod?.build_id?.() ?? null;
      window.__probe_state = { phase: "running", buildId };
      app.player.debug_readback(app.time, (stats) => {
        window.__probe_state = { phase: "done", buildId, readback: JSON.parse(stats) };
      });
    }
    return;
  }
  if (PROBE_JSFILL) {
    if (!probeSampled) {
      probeSampled = true;
      jsFillBaseline(els.canvas)
        .then(() => probeSampleCanvas())
        .catch((err) => { window.__probe_state = { phase: "done", jsFillError: String(err) }; });
    }
    return;
  }
  if (PROBE_FILL) {
    if (!probeSampled) {
      probeSampled = true;
      try {
        app.player?.debug_fill(0.16, 0.8, 0.3);
        probeSampleCanvas();
      } catch (err) {
        window.__probe_state = { phase: "done", fillError: String(err) };
      }
    }
    return;
  }
  if (rendered && !probeSampled && probeFrames >= 30) {
    probeSampled = true;
    probeSampleCanvas();
    return;
  }
  if (rendered) probeFrames += 1;
  if (probeFrames < 30) {
    window.__probe_state = {
      phase: "running",
      playerReady: !!app.player,
      hasDocument: app.player ? app.player.has_document() : false,
      lastBuildOk: app.diagnostics.length === 0,
      diagnostics: app.diagnostics.length,
      frames: probeFrames,
      fps: app.fps.value,
    };
  }
}

async function jsFillBaseline(canvas) {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error("no adapter for js baseline");
  const device = await adapter.requestDevice();
  const context = canvas.getContext("webgpu");
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });
  const encoder = device.createCommandEncoder();
  const pass = encoder.beginRenderPass({
    colorAttachments: [{
      view: context.getCurrentTexture().createView(),
      loadOp: "clear",
      storeOp: "store",
      clearValue: { r: 0.16, g: 0.8, b: 0.3, a: 1 },
    }],
  });
  pass.end();
  device.queue.submit([encoder.finish()]);
}

function probeSampleCanvas() {
  try {
    const sample = document.createElement("canvas");
    sample.width = els.canvas.width;
    sample.height = els.canvas.height;
    const ctx = sample.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(els.canvas, 0, 0);
    const data = ctx.getImageData(0, 0, sample.width, sample.height).data;
    let nonBlack = 0;
    let nonWhite = 0;
    for (let i = 0; i < data.length; i += 40) {
      const sum = data[i] + data[i + 1] + data[i + 2];
      if (sum > 24) nonBlack += 1;
      if (sum < 720) nonWhite += 1;
    }
    window.__probe_canvas = sample.toDataURL("image/png");
    window.__probe_state = {
      phase: "done",
      playerReady: true,
      frames: probeFrames,
      fps: app.fps.value,
      sampledPixels: Math.floor(data.length / 40),
      nonBlackPixels: nonBlack,
      nonWhitePixels: nonWhite,
      time: app.time,
    };
  } catch (err) {
    window.__probe_state = { phase: "done", sampleError: String(err) };
  }
}

function renderFrame(time) {
  if (!app.player || !app.player.has_document()) return false;
  try {
    app.player.render_frame(time);
    app.renderDirty = false;
    return true;
  } catch (err) {
    // Show the first render error; spamming the console per-frame is noise.
    if (app.lastErrorShown !== String(err)) {
      app.lastErrorShown = String(err);
      console.error("animatix-web: render_frame failed", err);
    }
    return false;
  }
}

function updateFps(now, rendered) {
  if (now - app.fps.since >= FPS_WINDOW_MS) {
    app.fps.value = Math.round((app.fps.rendered * 1000) / (now - app.fps.since));
    app.fps.since = now;
    app.fps.rendered = 0;
    els.fpsBadge.textContent = `${app.fps.value} fps`;
  }
  if (rendered) app.fps.rendered += 1;
}

function updateTransport() {
  const text = `${app.time.toFixed(2)} / ${app.duration.toFixed(2)} s`;
  els.timeReadout.textContent = text;
  els.ptimeReadout.textContent = `${app.time.toFixed(2)} s`;
  if (!scrubbing) {
    els.scrubber.value = Math.round((app.time / Math.max(app.duration, 0.05)) * 1000);
    els.pscrubber.value = els.scrubber.value;
  }
}

// ── Transport controls ──────────────────────────────────────────────

let scrubbing = false;

function setPlaying(playing) {
  app.playing = playing;
  syncPlayButtons();
}

function syncPlayButtons() {
  for (const icon of [els.iconPlay, els.piconPlay]) {
    icon.classList.toggle("paused", !app.playing);
  }
}

function togglePlay() {
  if (!app.player) return;
  if (!app.playing && app.time >= app.duration && app.duration > 0) app.time = 0;
  setPlaying(!app.playing);
}

function bindTransport() {
  els.btnPlay.addEventListener("click", togglePlay);
  els.pbtnPlay.addEventListener("click", togglePlay);
  els.btnRestart.addEventListener("click", () => { app.time = 0; app.renderDirty = true; });

  for (const bar of [els.scrubber, els.pscrubber]) {
    bar.addEventListener("input", () => {
      scrubbing = true;
      app.time = (Number(bar.value) / 1000) * app.duration;
      app.renderDirty = true;
      updateTransport();
    });
    bar.addEventListener("change", () => { scrubbing = false; });
    bar.addEventListener("pointerdown", () => { scrubbing = true; });
    bar.addEventListener("pointerup", () => { scrubbing = false; });
  }

  els.btnLoop.addEventListener("click", () => {
    app.loop = !app.loop;
    els.btnLoop.classList.toggle("toggled", app.loop);
    els.btnLoop.setAttribute("aria-pressed", String(app.loop));
  });

  document.addEventListener("keydown", (event) => {
    const inEditor = event.target.closest?.(".cm-editor");
    if (event.code === "Space" && !inEditor) {
      event.preventDefault();
      togglePlay();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      rebuildNow();
    } else if (event.key === "Escape" && app.mode === "play") {
      enterMode("edit");
    }
  });

  // click-to-toggle on the player canvas
  els.playerCanvasHost.addEventListener("click", (event) => {
    if (event.target === els.canvas) togglePlay();
  });
}

// ── Modes & layout ──────────────────────────────────────────────────

function enterMode(mode, { replace = true } = {}) {
  app.mode = mode;
  document.body.classList.toggle("mode-editor", mode === "edit");
  document.body.classList.toggle("mode-player", mode === "play");
  els.modeEditor.classList.toggle("active", mode === "edit");
  els.modePlayer.classList.toggle("active", mode === "play");
  els.modeEditor.setAttribute("aria-selected", String(mode === "edit"));
  els.modePlayer.setAttribute("aria-selected", String(mode === "play"));

  if (mode === "play") {
    els.playerCanvasHost.appendChild(els.canvas);
    els.playerstage.classList.remove("hidden");
    armIdleTimer();
    setPlaying(true);
  } else {
    els.canvasFrame.insertBefore(els.canvas, els.canvasFrame.firstChild);
    els.playerstage.classList.add("hidden");
  }
  fitCanvasPixels();
  app.renderDirty = true;

  const params = new URLSearchParams(location.search);
  params.set("mode", mode === "play" ? "play" : "edit");
  if (app.exampleId) params.set("example", app.exampleId);
  const url = `${location.pathname}?${params}`;
  if (replace) history.replaceState(null, "", url);
  else history.pushState(null, "", url);
}

els.modeEditor.addEventListener("click", () => enterMode("edit"));
els.modePlayer.addEventListener("click", () => enterMode("play"));
els.playerBack.addEventListener("click", (event) => {
  event.preventDefault();
  enterMode("edit");
});

let idleTimer = null;
function armIdleTimer() {
  clearTimeout(idleTimer);
  els.playerstage.classList.remove("idle");
  idleTimer = setTimeout(() => els.playerstage.classList.add("idle"), 2400);
}
els.playerstage.addEventListener("mousemove", armIdleTimer);
els.playerstage.addEventListener("touchstart", armIdleTimer, { passive: true });

// canvas keeps scene aspect at device-pixel resolution
function fitCanvasPixels() {
  const container = app.mode === "play"
    ? els.playerCanvasHost
    : els.canvasFrame;
  const availableW = Math.max(container.clientWidth - (app.mode === "play" ? 0 : 44), 64);
  const availableH = Math.max(container.clientHeight - (app.mode === "play" ? 0 : 44), 64);
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const aspect = app.sceneW / app.sceneH;

  let cssW = availableW;
  let cssH = cssW / aspect;
  if (cssH > availableH) { cssH = availableH; cssW = cssH * aspect; }

  els.canvas.style.width = `${Math.round(cssW)}px`;
  els.canvas.style.height = `${Math.round(cssH)}px`;
  const pxW = Math.max(Math.round(cssW * dpr), 2);
  const pxH = Math.max(Math.round(cssH * dpr), 2);
  if (els.canvas.width !== pxW || els.canvas.height !== pxH) {
    els.canvas.width = pxW;
    els.canvas.height = pxH;
  }
}

// divider drag
{
  let dragging = false;
  els.divider.addEventListener("pointerdown", (event) => {
    dragging = true;
    els.divider.classList.add("dragging");
    event.target.setPointerCapture(event.pointerId);
  });
  els.divider.addEventListener("pointerup", (event) => {
    dragging = false;
    els.divider.classList.remove("dragging");
  });
  els.divider.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    const total = window.innerWidth;
    const fraction = Math.min(Math.max(event.clientX / total, 0.2), 0.75);
    document.querySelector(".workspace").style.gridTemplateColumns =
      `minmax(300px, ${fraction * 100}%) 4px 1fr`;
    fitCanvasPixels();
    app.renderDirty = true;
  });
}

els.diagToggle.addEventListener("click", () => {
  const expanded = els.diagToggle.getAttribute("aria-expanded") === "true";
  els.diagToggle.setAttribute("aria-expanded", String(!expanded));
  els.diagList.classList.toggle("collapsed", expanded);
});

bindTransport();
boot();
