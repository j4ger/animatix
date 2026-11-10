#!/usr/bin/env node
// web-page-perf — real-GPU page baseline: serve web/, drive Chromium through
// CDP, wait for every <amx-player> to reach `ready`, then measure a 5 s
// requestAnimationFrame window and snapshot each player's adaptive-quality
// state (render scale, pressure, target cadence, raster pixels).
//
// Why this exists: the smoke gate answers "did the page render?", not "how did
// the page pace?". A baseline needs the real adapter, because the numbers from
// SwiftShader are not comparable to a host GPU and a page that drops frames at
// 60 Hz can still pass every static gate. This script therefore defaults to a
// headed Chromium on the host display with native Vulkan (Dawn selects the
// NVIDIA ICD through VK_ICD_FILENAMES), and refuses to report SwiftShader
// numbers unless they were explicitly requested.
//
// Usage:
//   nix develop -c bash -lc 'DISPLAY=:0 \
//     VK_ICD_FILENAMES=/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json \
//     node scripts/web-page-perf.mjs'
//   WPP_PAGE=/demos/transformer/index.html ... node scripts/web-page-perf.mjs
//
// Env:
//   WPP_PAGE         page to measure (default /tour/)
//   WPP_SECONDS      measurement window in seconds (default 5)
//   WPP_HEADED       1 (default) real window on $DISPLAY; 0 = --headless=new
//   WPP_SOFTWARE     1 = SwiftShader (CI/no-GPU); implies headless. Default 0.
//   WPP_SETTLE_MS    quiet time before the window (default 2000)
//   WPP_ALL_VISIBLE  1 = emulate a viewport tall enough to show every player
//                    (a stress run; default measures the natural viewport)
//   WPP_OUT          JSON output path (default target/perf/web-baseline.json)
//   WPP_WINDOW       WxH window (default 1440x900)
//   WPP_EXTRA_FLAGS  extra Chromium flags, whitespace separated
//   CHROME, PYTHON   override the binaries
//
// Readiness follows scripts/web-smoke.mjs: switch deck pages to Article mode,
// then walk each player into view and (for lazy players) call `loadNow()`
// until `_state === "ready"` and a canvas exists in its shadow root.
//
// The measured window sits at the last player the readiness walk centred, so
// at least one figure is actually rendering (hidden players idle by design).
// The JSON records `window.visible_players` so the number stays interpretable;
// WPP_ALL_VISIBLE=1 turns the same window into an all-figures stress run.
//
// Scope: this is a page-cadence baseline, not a GPU frame-cost measurement.
// rAF ticks are capped by the display refresh (and a backgrounded or headless
// tab can deliver none at all — docs/performance_evaluation.md §3.7), so `fps`
// says whether the page keeps up with its compositor, while `pressure` and
// `render_scale` show the player's adaptive-quality response. Per-frame GPU
// cost is what web/dev/perf-probe.html (`debug_bench_frames` +
// `debug_gpu_drain`) measures.
//
// Output: one compact JSON object on stdout, a pretty copy at WPP_OUT, and a
// per-page snapshot under target/perf/pages/. Progress and diagnostics go to
// stderr, so stdout stays machine-readable.
//
// Exit: 0 on success; 1 on load failure, no players, player error, missing
// WebGPU, or a SwiftShader adapter when WPP_SOFTWARE is not set.
//
// No third-party dependencies: Node 22+ has fetch and WebSocket.

import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import net from "node:net";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const CHROME = process.env.CHROME || "chromium";
const PYTHON = process.env.PYTHON || "python3";
const PAGE = process.env.WPP_PAGE || "/tour/";
const SECONDS = Math.max(1, Number(process.env.WPP_SECONDS || 5));
const HEADED = process.env.WPP_HEADED !== "0";
const SOFTWARE = process.env.WPP_SOFTWARE === "1";
const SETTLE_MS = Math.max(0, Number(process.env.WPP_SETTLE_MS || 2000));
const ALL_VISIBLE = process.env.WPP_ALL_VISIBLE === "1";
const OUT = process.env.WPP_OUT || join(ROOT, "target/perf/web-baseline.json");
const WINDOW_SIZE = process.env.WPP_WINDOW || "1440x900";
const BUNDLE = join(ROOT, "web/embed/amx-player.js");
const BUNDLE_SRC = join(ROOT, "web/embed/src/amx-player.js");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const log = (message) => process.stderr.write(`web-page-perf: ${message}\n`);

// Detached children get their own process group; kill the whole group so a
// crashed Chromium does not leave renderer/GPU processes behind.
const children = [];
function killGroup(child) {
  if (!child || child.exitCode !== null) return;
  try {
    process.kill(-child.pid, "SIGKILL");
  } catch {
    try {
      child.kill("SIGKILL");
    } catch {
      // already gone
    }
  }
}
function killChildren() {
  for (const child of children) killGroup(child);
}
process.on("exit", killChildren);
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    killChildren();
    process.exit(130);
  });
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

async function waitFor(what, timeoutMs, probe) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      if (await probe()) return;
    } catch (error) {
      last = error;
    }
    await sleep(200);
  }
  throw new Error(`timed out after ${timeoutMs}ms waiting for ${what}${last ? `: ${last.message}` : ""}`);
}

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.nextId = 0;
    this.pending = new Map();
    this.onEvent = null;
  }

  async open() {
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
    this.ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.method && this.onEvent) this.onEvent(message);
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(JSON.stringify(message.error)));
      else pending.resolve(message.result);
    });
  }

  send(method, params = {}) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    try {
      this.ws.close();
    } catch {
      // already closed
    }
  }
}

// Evaluate a self-contained page function and return its JSON value. Keeping
// the probe source in a function (rather than a string template) means Node's
// syntax check covers it, and `awaitPromise` covers the async ones.
async function evaluate(cdp, fn, ...args) {
  const expression = `(${fn.toString()})(${args.map((arg) => JSON.stringify(arg) ?? "undefined").join(", ")})`;
  const { result, exceptionDetails } = await cdp.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (exceptionDetails) {
    const detail = exceptionDetails.exception?.description || exceptionDetails.text || "unknown error";
    throw new Error(`page evaluation failed: ${detail}`);
  }
  return result.value;
}

function sha256(path) {
  try {
    return createHash("sha256").update(readFileSync(path)).digest("hex");
  } catch {
    return null;
  }
}

function git(args) {
  try {
    return execFileSync("git", args, { cwd: ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return null;
  }
}

// Which file scripts/serve-web.py actually serves for /embed/amx-player.js:
// the built bundle, or the newer src module (it prefers src when that is
// newer, so a baseline can silently measure unbundled code).
function servedBundle() {
  if (!existsSync(BUNDLE)) return { served: null, path: null };
  try {
    if (existsSync(BUNDLE_SRC) && statSync(BUNDLE_SRC).mtimeMs > statSync(BUNDLE).mtimeMs) {
      return { served: "src", path: relative(ROOT, BUNDLE_SRC) };
    }
  } catch {
    // fall through to the built bundle
  }
  return { served: "dist", path: relative(ROOT, BUNDLE) };
}

// ── page-side probes ────────────────────────────────────────────────────────

// Runs before any document script: capture amxerror so a failed scene is a
// readable failure instead of a timeout.
function preDocumentProbe() {
  window.__wpp = { errors: [] };
  addEventListener(
    "amxerror",
    (event) => {
      const detail = event.detail || {};
      let text = "amxerror";
      if (detail.message) text = String(detail.message);
      else if (detail.error) text = detail.error.message || String(detail.error);
      else if (detail.diagnostics) {
        try {
          text = JSON.stringify(detail.diagnostics).slice(0, 800);
        } catch {
          // keep the default text
        }
      }
      window.__wpp.errors.push(text);
    },
    true,
  );
}

// Deck pages (web/demos/deck.js) build their mode buttons at runtime and hide
// inactive slides with display:none, which leaves those players with a 1x1
// canvas. Article mode restores layout for every figure; the button only
// exists on deck pages, so this is a no-op elsewhere.
async function preparePlayers() {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const button = document.querySelector('.btn-deck-mode[data-mode="article"]');
  if (button) {
    button.click();
    await sleep(400);
  }
  return {
    count: document.querySelectorAll("amx-player").length,
    articleMode: Boolean(button),
  };
}

// Bring player `index` to `ready` exactly the way scripts/web-smoke.mjs does.
// Kept per player so the driver can report progress and apply a global
// deadline without one unbounded evaluate.
async function ensurePlayer(index) {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const players = [...document.querySelectorAll("amx-player")];
  const player = players[index];
  if (!player) return { error: `player ${index} disappeared from the page` };
  const src = player.getAttribute("src") || `#${index}`;
  const deadline = Date.now() + 120000;
  player.scrollIntoView({ block: "center", behavior: "instant" });
  if (player._state === "idle" && typeof player.loadNow === "function") player.loadNow();
  while (player._state !== "ready") {
    const errors = (window.__wpp && window.__wpp.errors) || [];
    if (errors.length) {
      return { error: `amxerror on ${src}: ${errors[0]}`, states: players.map((p) => p._state) };
    }
    if (player._state === "error") {
      return { error: `player error on ${src}`, states: players.map((p) => p._state) };
    }
    if (Date.now() > deadline) {
      return {
        error: `timed out waiting for ${src} to be ready (state: ${player._state})`,
        states: players.map((p) => p._state),
      };
    }
    await sleep(100);
  }
  // The first frame follows ready; give it a beat while still on screen, as
  // the smoke gate does, so the canvas has painted before we measure.
  await sleep(300);
  const canvas = player.shadowRoot && player.shadowRoot.querySelector("canvas");
  if (!canvas) return { error: `${src}: no canvas in the player's shadow root` };
  return { src, width: canvas.width, height: canvas.height };
}

// WebGPU caps. `adapter.info` is the only place the host GPU is named, and the
// SwiftShader check below keys off it; without it a "fast" software run could
// masquerade as a hardware baseline.
async function probeCaps() {
  const caps = {
    webgpu: Boolean(navigator.gpu),
    adapter: null,
    error: null,
    user_agent: navigator.userAgent,
    device_pixel_ratio: window.devicePixelRatio,
    hardware_concurrency: navigator.hardwareConcurrency,
    visibility_state: document.visibilityState,
  };
  if (!navigator.gpu) {
    caps.error = "navigator.gpu is undefined (WebGPU unavailable)";
    return caps;
  }
  try {
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) {
      caps.error = "navigator.gpu.requestAdapter() returned null";
      return caps;
    }
    const info = adapter.info || {};
    caps.adapter = {
      vendor: info.vendor ?? null,
      architecture: info.architecture ?? null,
      device: info.device ?? null,
      description: info.description ?? null,
      fallback: adapter.isFallbackAdapter ?? null,
    };
  } catch (error) {
    caps.error = `requestAdapter failed: ${error}`;
  }
  return caps;
}

// Snapshot the adaptive-quality state. The player exposes `pressure` and
// `targetFps` getters and keeps `_renderScale` / `_targetFps` privately; there
// is no raster-pixel accessor today, so accept either spelling if one lands
// and report null (not an estimate) when neither exists.
function snapshotPlayers() {
  const finite = (value) => (typeof value === "number" && Number.isFinite(value) ? value : null);
  const call = (player, name) => {
    try {
      return typeof player[name] === "function" ? player[name]() : null;
    } catch {
      return null;
    }
  };
  const read = (player, name) => {
    try {
      return player[name];
    } catch {
      return undefined;
    }
  };
  return [...document.querySelectorAll("amx-player")].map((player, index) => {
    const canvas = (player.shadowRoot && player.shadowRoot.querySelector("canvas")) || null;
    let rasterPixels = null;
    for (const name of ["rasterPixels", "_rasterPixels"]) {
      const value = call(player, name);
      if (value !== null && value !== undefined) {
        rasterPixels = finite(value) ?? null;
        break;
      }
    }
    return {
      index,
      src: player.getAttribute("src") || null,
      state: player._state ?? null,
      visible: player._visible ?? null,
      // `playing` is a draw in the window; `parked` is the page-level
      // concurrency cap holding this embed off the clock
      // (`window.__amxMaxPlaying`), which is not the same as an idle/offscreen
      // player. Reading both is what makes "vis 7, play 1" interpretable.
      playing: player._playing ?? null,
      parked: player._parkedFromPage ?? null,
      render_scale: finite(read(player, "renderScale") ?? read(player, "_renderScale")),
      pressure: finite(read(player, "pressure") ?? read(player, "_pressure")),
      target_fps: finite(read(player, "_targetFps") ?? read(player, "targetFps")),
      raster_pixels: rasterPixels,
      canvas_pixels: canvas ? canvas.width * canvas.height : null,
      canvas_css: canvas ? { width: canvas.clientWidth, height: canvas.clientHeight } : null,
    };
  });
}

// The measurement: collect rAF timestamps for `seconds`, then derive cadence
// and tail latencies. Percentiles use nearest-rank (ceil), the same convention
// as the render probes; intervals are the gaps between consecutive callbacks.
async function measureFrames(seconds) {
  const windowMs = Math.max(1, seconds) * 1000;
  const stamps = [];
  const start = performance.now();
  await new Promise((resolve) => {
    const tick = (timestamp) => {
      stamps.push(timestamp);
      if (timestamp - start < windowMs) requestAnimationFrame(tick);
      else resolve();
    };
    requestAnimationFrame(tick);
  });

  const intervals = [];
  for (let i = 1; i < stamps.length; i += 1) intervals.push(stamps[i] - stamps[i - 1]);
  const sorted = [...intervals].sort((a, b) => a - b);
  const round = (value) => (value === null || value === undefined ? null : Math.round(value * 100) / 100);
  const percentile = (p) => {
    if (!sorted.length) return null;
    const rank = Math.ceil((p / 100) * sorted.length);
    return sorted[Math.min(sorted.length - 1, Math.max(0, rank - 1))];
  };
  const span = stamps.length > 1 ? stamps[stamps.length - 1] - stamps[0] : 0;
  const mean = intervals.length ? intervals.reduce((sum, value) => sum + value, 0) / intervals.length : null;
  return {
    count: stamps.length,
    duration_ms: round(span),
    fps: span > 0 ? round(((stamps.length - 1) / span) * 1000) : 0,
    min_ms: round(sorted[0] ?? null),
    mean_ms: round(mean),
    p50_ms: round(percentile(50)),
    p95_ms: round(percentile(95)),
    p99_ms: round(percentile(99)),
    max_ms: round(sorted[sorted.length - 1] ?? null),
    long_33: intervals.filter((value) => value > 33).length,
    long_50: intervals.filter((value) => value > 50).length,
    long_100: intervals.filter((value) => value > 100).length,
  };
}

// Count the figures that are actually on screen during the window. Hidden
// players pause by design, so this is what makes the fps number interpretable.
function visiblePlayers() {
  return [...document.querySelectorAll("amx-player")].filter((player) => player._visible === true).length;
}

// ── Chromium launch ─────────────────────────────────────────────────────────

function chromeFlags({ cdpPort, profile, headed }) {
  const flags = [
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--disable-gpu-sandbox",
    "--no-first-run",
    "--no-default-browser-check",
    "--mute-audio",
    "--disable-background-networking",
    "--disable-extensions",
    // A baseline must not be throttled because the window lost focus.
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
    "--disable-background-timer-throttling",
    `--window-size=${WINDOW_SIZE}`,
    `--remote-debugging-port=${cdpPort}`,
    `--user-data-dir=${profile}`,
    "about:blank",
  ];

  if (SOFTWARE) {
    // CI/no-GPU: the bundled SwiftShader ICD is the only WebGPU adapter. The
    // unsafe flag is required to expose it; it also disables the GPU blocklist
    // reasoning, which is why the hardware path below must not pass it.
    flags.unshift("--headless=new");
    flags.push(
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-vulkan-surface",
      "--enable-features=Vulkan",
      "--ignore-gpu-blocklist",
    );
  } else {
    // Native Vulkan: Dawn selects the host ICD through VK_ICD_FILENAMES.
    // NB: do NOT pass --enable-unsafe-webgpu here — it is what selects the
    // SwiftShader WebGPU adapter.
    flags.push("--use-vulkan=native", "--enable-features=Vulkan,WebGPUService", "--ignore-gpu-blocklist");
    if (!headed) {
      // Headless has no Vulkan surface; ANGLE still needs pointing at Vulkan
      // for the compositor.
      flags.unshift("--headless=new", "--disable-vulkan-surface", "--use-gl=angle", "--use-angle=vulkan");
    }
  }

  if (process.env.WPP_EXTRA_FLAGS) {
    flags.push(...process.env.WPP_EXTRA_FLAGS.split(/\s+/).filter(Boolean));
  }
  return flags;
}

function adapterLooksSoftware(adapter) {
  if (!adapter) return false;
  const haystack = [adapter.vendor, adapter.architecture, adapter.device, adapter.description]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return adapter.fallback === true || /swiftshader|llvmpipe|softpipe|software/.test(haystack);
}

// ── main ────────────────────────────────────────────────────────────────────

async function run() {
  if (!existsSync(join(ROOT, "web/pkg/animatix_web_bg.wasm"))) {
    throw new Error("web/pkg/animatix_web_bg.wasm is missing — run scripts/ci.sh gate web-build first");
  }

  const bundleBefore = sha256(BUNDLE);
  const served = servedBundle();
  if (served.served === "src") {
    log(`warning: serve-web.py will serve ${served.path} (newer than the built bundle)`);
  }

  const sitePort = await freePort();
  const cdpPort = await freePort();
  const base = `http://127.0.0.1:${sitePort}`;
  const profile = mkdtempSync(join(tmpdir(), "amx-page-perf-"));
  const headed = HEADED && !SOFTWARE;

  let serverLog = "";
  let chromeLog = "";
  let consoleLog = "";

  const server = spawn(PYTHON, [join(ROOT, "scripts/serve-web.py"), String(sitePort)], {
    cwd: ROOT,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  children.push(server);
  server.stdout.on("data", (chunk) => (serverLog += chunk));
  server.stderr.on("data", (chunk) => (serverLog += chunk));

  const chrome = spawn(CHROME, chromeFlags({ cdpPort, profile, headed }), {
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  children.push(chrome);
  chrome.stdout.on("data", (chunk) => (chromeLog += chunk));
  chrome.stderr.on("data", (chunk) => (chromeLog += chunk));

  let cdp;
  try {
    await waitFor("the local site server", 20000, async () => (await fetch(`${base}/`)).ok);
    const pageResponse = await fetch(`${base}${PAGE}`);
    if (!pageResponse.ok) {
      throw new Error(`${PAGE} returned HTTP ${pageResponse.status} from the local server`);
    }
    await waitFor("Chromium's CDP endpoint", 30000, async () => {
      const response = await fetch(`http://127.0.0.1:${cdpPort}/json/version`);
      return response.ok;
    });

    // about:blank first, so the pre-document probe is installed before the
    // target page's own scripts run.
    let response = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent("about:blank")}`, {
      method: "PUT",
    });
    if (!response.ok) {
      response = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent("about:blank")}`);
    }
    if (!response.ok) throw new Error(`could not create a CDP target (HTTP ${response.status})`);
    const target = await response.json();

    cdp = new Cdp(target.webSocketDebuggerUrl);
    cdp.onEvent = (message) => {
      if (message.method === "Runtime.consoleAPICalled") {
        const args = (message.params.args || [])
          .map((arg) => arg.value ?? arg.description ?? arg.type ?? "")
          .join(" ");
        consoleLog += `[${message.params.type}] ${args}\n`;
      } else if (message.method === "Runtime.exceptionThrown") {
        const details = message.params.exceptionDetails || {};
        consoleLog += `[exception] ${details.text || ""} ${details.exception?.description || ""}\n`;
      }
    };
    await cdp.open();
    await cdp.send("Page.enable");
    await cdp.send("Runtime.enable");
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `(${preDocumentProbe.toString()})()` });

    const navigation = await cdp.send("Page.navigate", { url: `${base}${PAGE}` });
    if (navigation.errorText) throw new Error(`${PAGE} failed to load: ${navigation.errorText}`);
    await waitFor(`load of ${PAGE}`, 60000, async () => {
      const { result } = await cdp.send("Runtime.evaluate", {
        expression: "document.readyState",
        returnByValue: true,
      });
      return result.value === "complete";
    });

    // Adapter first: a page without WebGPU will fail every player, and the
    // caps are the useful part of that failure.
    const caps = await evaluate(cdp, probeCaps);
    log(
      `mode=${SOFTWARE ? "software" : headed ? "gpu-headed" : "gpu-headless"} ` +
        `webgpu=${caps.webgpu} adapter=${JSON.stringify(caps.adapter)}` +
        (caps.error ? ` error=${caps.error}` : ""),
    );
    if (!caps.webgpu || !caps.adapter) {
      const error = new Error(`no WebGPU adapter: ${caps.error || "adapter unavailable"}`);
      error.caps = caps;
      throw error;
    }
    if (adapterLooksSoftware(caps.adapter) && !SOFTWARE) {
      const error = new Error(
        `WebGPU adapter is software (${JSON.stringify(caps.adapter)}) — ` +
          "rerun with WPP_SOFTWARE=1 to measure software, or fix the native Vulkan path",
      );
      error.caps = caps;
      throw error;
    }

    const prepared = await evaluate(cdp, preparePlayers);
    log(`${PAGE}: ${prepared.count} <amx-player> element(s)${prepared.articleMode ? ", article mode" : ""}`);
    if (!prepared.count) {
      const error = new Error(`${PAGE}: no <amx-player> element on the page`);
      error.caps = caps;
      throw error;
    }

    const globalDeadline = Date.now() + 600000;
    for (let index = 0; index < prepared.count; index += 1) {
      if (Date.now() > globalDeadline) {
        throw new Error(`${PAGE}: timed out waiting for players to become ready`);
      }
      const outcome = await evaluate(cdp, ensurePlayer, index);
      if (outcome.error) {
        const error = new Error(`${PAGE}: ${outcome.error}`);
        error.caps = caps;
        error.states = outcome.states;
        throw error;
      }
      log(`ready ${index + 1}/${prepared.count} (${outcome.src}, canvas ${outcome.width}x${outcome.height})`);
    }

    const pageErrors = await evaluate(cdp, () => (window.__wpp && window.__wpp.errors) || []);
    if (pageErrors.length) throw new Error(`${PAGE}: amxerror: ${pageErrors[0]}`);

    if (ALL_VISIBLE) {
      // Stress variant: one viewport tall enough to intersect every figure.
      // Emulation keeps the headed window untouched while changing layout.
      const height = await evaluate(cdp, () =>
        Math.ceil(Math.max(document.documentElement.scrollHeight, document.body.scrollHeight)),
      );
      const [width, fallbackHeight] = WINDOW_SIZE.split("x").map(Number);
      const dpr = await evaluate(cdp, () => window.devicePixelRatio || 1);
      await cdp.send("Emulation.setDeviceMetricsOverride", {
        width: width || 1440,
        height: Math.min(Math.max(height, fallbackHeight || 900), 20000),
        deviceScaleFactor: dpr,
        mobile: false,
      });
      await evaluate(cdp, () => {
        window.scrollTo(0, 0);
        // The page may leave figures paused until scrolled into view (or until
        // the user interacts). The page-raster budget only allocates over
        // *playing* embeds, so a stress run must actually play them; otherwise
        // it measures an idle compositor and the budget never engages.
        for (const player of document.querySelectorAll("amx-player")) {
          if (player._state === "ready" && !player._playing) player.play?.();
        }
        return true;
      });
      log(`all-visible: emulated viewport ${width}x${Math.min(height, 20000)}`);
    }

    await sleep(SETTLE_MS);
    const frame = await evaluate(cdp, measureFrames, SECONDS);
    if (frame.count < 2) {
      // A zero/one-frame window is not a fast page: it is a tab that was not
      // compositing (backgrounded, occluded, or headless). Refuse to record it.
      throw new Error(
        `${PAGE}: the rAF window collected ${frame.count} frame(s) — the tab was not rendering; ` +
          "rerun headed on the host display (docs/performance_evaluation.md §3.7)",
      );
    }
    const players = await evaluate(cdp, snapshotPlayers);
    const visible = await evaluate(cdp, visiblePlayers);
    const viewport = await evaluate(cdp, () => ({
      width: window.innerWidth,
      height: window.innerHeight,
      device_pixel_ratio: window.devicePixelRatio,
      scroll_y: Math.round(window.scrollY),
    }));

    const bundleAfter = sha256(BUNDLE);
    const changed = bundleBefore !== null && bundleAfter !== null && bundleBefore !== bundleAfter;
    if (changed) log(`warning: web/embed/amx-player.js changed during the run (${bundleBefore} -> ${bundleAfter})`);
    const consoleErrors = consoleLog
      .split("\n")
      .filter((line) => line.startsWith("[error]") || line.startsWith("[exception]"));
    if (consoleErrors.length) {
      log(`warning: ${consoleErrors.length} page console error(s), first: ${consoleErrors[0]}`);
    }

    const result = {
      created_utc: new Date().toISOString(),
      commit: git(["rev-parse", "HEAD"]),
      worktree_dirty: Boolean(git(["status", "--porcelain", "--untracked-files=no"])),
      page: PAGE,
      players,
      frame,
      render_scales: players.map((player) => player.render_scale),
      mode: SOFTWARE ? "software-headless" : headed ? "gpu-headed" : "gpu-headless",
      adapter: caps.adapter,
      caps: {
        webgpu: caps.webgpu,
        user_agent: caps.user_agent,
        hardware_concurrency: caps.hardware_concurrency,
        visibility_state: caps.visibility_state,
      },
      window: {
        seconds: SECONDS,
        settle_ms: SETTLE_MS,
        all_visible: ALL_VISIBLE,
        visible_players: visible,
        viewport,
      },
      launch: {
        headed,
        software: SOFTWARE,
        chrome: CHROME,
        flags: chromeFlags({ cdpPort, profile, headed }).filter(
          (flag) => !flag.startsWith("--user-data-dir") && !flag.startsWith("--remote-debugging-port"),
        ),
      },
      bundle: {
        path: relative(ROOT, BUNDLE),
        served: served.served,
        served_path: served.path,
        sha256_before: bundleBefore,
        sha256_after: bundleAfter,
        changed,
      },
    };

    mkdirSync(dirname(OUT), { recursive: true });
    writeFileSync(OUT, `${JSON.stringify(result, null, 2)}\n`);
    const slug = PAGE.replace(/^\/+|\/+$/g, "").replace(/[^A-Za-z0-9._-]+/g, "-") || "index";
    const snapshot = join(ROOT, "target/perf/pages", `${slug}.json`);
    mkdirSync(dirname(snapshot), { recursive: true });
    writeFileSync(snapshot, `${JSON.stringify(result, null, 2)}\n`);

    process.stdout.write(`${JSON.stringify(result)}\n`);
    log(
      `${PAGE}: ${frame.count} frames in ${frame.duration_ms}ms → ${frame.fps} fps, ` +
        `p50 ${frame.p50_ms}ms, p95 ${frame.p95_ms}ms, p99 ${frame.p99_ms}ms, ` +
        `max ${frame.max_ms}ms, long>33 ${frame.long_33}, long>50 ${frame.long_50}, ` +
        `${visible} visible player(s), mode ${result.mode}`,
    );
    log(`wrote ${relative(ROOT, OUT)} and ${relative(ROOT, snapshot)}`);
    return result;
  } catch (error) {
    if (error.caps) log(`caps: ${JSON.stringify(error.caps)}`);
    if (error.states) log(`player states: ${error.states.join(",")}`);
    if (consoleLog) log(`page console:\n${consoleLog.slice(-4000)}`);
    if (chromeLog) log(`chromium:\n${chromeLog.slice(-2000)}`);
    if (serverLog) log(`serve-web.py:\n${serverLog.slice(-1000)}`);
    throw error;
  } finally {
    if (cdp) cdp.close();
    killGroup(chrome);
    killGroup(server);
    await sleep(200);
    rmSync(profile, { recursive: true, force: true });
  }
}

run().catch((error) => {
  process.stderr.write(`web-page-perf: FAILED: ${error.message}\n`);
  process.exitCode = 1;
});
