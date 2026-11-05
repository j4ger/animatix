#!/usr/bin/env node
// Drives web/dev/adaptive-probe.html in Chromium and prints the adaptive-quality
// summary it publishes on `window.__adaptive`. See web/dev/perf-probe.html for
// the frame-cost harness (a different probe).
//
// Env:
//   AP_BUNDLE=new|legacy  AP_N=<count>  AP_W=<cssW>  AP_H=<cssH>  AP_SCENE=<url>
//   AP_MANUAL=1|0         manual-tick (default, 1) vs real-rAF (0). Manual mode
//                         appends amxmanualtick=1 and drives the scheduler from a
//                         synthetic 16 ms setInterval — a perfect cadence that
//                         CANNOT expose GPU saturation. Set AP_MANUAL=0 to run
//                         the real requestAnimationFrame scheduler: then we do
//                         NOT append amxmanualtick=1, so core.js keeps its normal
//                         visibility guard. real-rAF is the ONLY mode that can
//                         validate GPU-pressure adaptation.
//   AP_HEADED=1           alias for HB_HEADED=1 (both open a real window).
//                         real-rAF (AP_MANUAL=0) REQUIRES a headed window:
//                         headless/backgrounded tabs deliver no rAF callbacks,
//                         so nothing runs and the measured fps is ~0.
//   AP_INJECT=<steps>     manual mode only: have the page call
//   AP_INJECT_GAP=<ms>    window.__amxInjectPressure(steps, gap) once, so the
//                         controller's down-step path is exercised
//                         deterministically in headless CI (default gap 40 ms).
//   AP_PAGE_PIXELS=<n>    page-wide raster-pixel budget. Installed as
//                         `window.__amxPageRasterPixels` in a
//                         document-start script, i.e. before any player
//                         initialises. The run asserts the measured page
//                         raster stays within 10% of it and exits nonzero
//                         otherwise.
//   HB_HEADED=1           real window on a real GPU (Linux WebGPU needs
//                         HB_UNSAFE_WEBGPU=1; run with the NVIDIA ICD env)
//   HB_SOFTWARE=1         force SwiftShader (reproducible anywhere). Its fps is a
//                         SOFTWARE number, NOT a real-GPU measurement — do not
//                         read it as real-GPU fps.
//   CHROME=/path/to/chromium
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import net from "node:net";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const CHROME = process.env.CHROME || "chromium";
const PYTHON = process.env.PYTHON || "python3";
const HEADED = process.env.HB_HEADED === "1" || process.env.AP_HEADED === "1";
const SOFTWARE = process.env.HB_SOFTWARE === "1";
const UNSAFE = process.env.HB_UNSAFE_WEBGPU === "1";

const bundle = process.env.AP_BUNDLE || "new";
const n = process.env.AP_N || "12";
const w = process.env.AP_W || "320";
const h = process.env.AP_H || "180";
const scene = process.env.AP_SCENE || "./adaptive_probe.amx";
const manual = process.env.AP_MANUAL === "0" ? "0" : "1";
const inject = process.env.AP_INJECT || "";
const injectGap = process.env.AP_INJECT_GAP || "40";
const pagePixelsRaw = process.env.AP_PAGE_PIXELS ?? "";
const pagePixels = Number(pagePixelsRaw);
const pageBudget = Number.isFinite(pagePixels) && pagePixels > 0 ? pagePixels : null;
// Only manual-tick mode installs the synthetic scheduler hook. real-rAF mode
// must NOT carry amxmanualtick=1: that flag replaces the real scheduler AND
// disables core.js's visibility guard, which would hide the very GPU-saturation
// behaviour we are trying to measure.
const manualTick = manual === "1" ? "&amxmanualtick=1" : "";
const injectQuery = manual === "1" && inject
  ? `&inject=${encodeURIComponent(inject)}&injectGap=${encodeURIComponent(injectGap)}`
  : "";
const PAGE = `/dev/adaptive-probe.html?bundle=${encodeURIComponent(bundle)}&n=${n}&w=${w}&h=${h}&scene=${encodeURIComponent(scene)}&manual=${manual}${manualTick}${injectQuery}`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitFor(what, timeoutMs, probe) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try { if (await probe()) return; } catch (e) { last = e; }
    await sleep(200);
  }
  throw new Error(`timed out waiting for ${what}${last ? `: ${last.message}` : ""}`);
}

class Cdp {
  constructor(url) { this.ws = new WebSocket(url); this.nextId = 0; this.pending = new Map(); }
  async open() {
    await new Promise((res, rej) => {
      this.ws.addEventListener("open", res, { once: true });
      this.ws.addEventListener("error", rej, { once: true });
    });
    this.ws.addEventListener("message", (ev) => {
      const m = JSON.parse(ev.data);
      const p = this.pending.get(m.id);
      if (!p) return;
      this.pending.delete(m.id);
      if (m.error) p.reject(new Error(JSON.stringify(m.error)));
      else p.resolve(m.result);
    });
  }
  send(method, params = {}) {
    const id = ++this.nextId;
    return new Promise((res, rej) => { this.pending.set(id, { resolve: res, reject: rej }); this.ws.send(JSON.stringify({ id, method, params })); });
  }
  close() { try { this.ws.close(); } catch {} }
}

async function freePort() {
  return await new Promise((res, rej) => {
    const s = net.createServer();
    s.once("error", rej);
    s.listen(0, "127.0.0.1", () => { const p = s.address().port; s.close(() => res(p)); });
  });
}

function killGroup(c) {
  if (!c || c.exitCode !== null) return;
  try { process.kill(-c.pid, "SIGKILL"); } catch { try { c.kill("SIGKILL"); } catch {} }
}

function flags(cdpPort, profile) {
  const f = [
    "--no-sandbox", "--disable-dev-shm-usage", "--disable-gpu-sandbox",
    "--enable-features=Vulkan,WebGPUService", "--ignore-gpu-blocklist",
    "--no-first-run", "--no-default-browser-check", "--mute-audio",
    "--disable-background-networking", "--disable-extensions",
    "--disable-background-timer-throttling", "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding", "--disable-features=CalculateNativeWinOcclusion",
    "--autoplay-policy=no-user-gesture-required", "--window-size=1600,1000",
  ];
  if (SOFTWARE) f.push("--enable-unsafe-webgpu", "--enable-unsafe-swiftshader", "--use-gl=angle", "--use-angle=swiftshader", "--use-vulkan=swiftshader");
  else {
    f.push("--use-vulkan=native");
    if (!HEADED) f.push("--use-gl=angle", "--use-angle=vulkan");
    if (UNSAFE) f.push("--enable-unsafe-webgpu");
  }
  if (!HEADED) f.unshift("--headless=new");
  f.push(`--remote-debugging-port=${cdpPort}`, `--user-data-dir=${profile}`, "about:blank");
  return f;
}

async function main() {
  if (!existsSync(join(ROOT, "web/embed/amx-player.js"))) throw new Error("embed bundle missing (npm --prefix web/tools run build:embed)");
  const sitePort = await freePort();
  const cdpPort = await freePort();
  const base = `http://127.0.0.1:${sitePort}`;
  const profile = mkdtempSync(join(tmpdir(), "amx-ap-"));
  let sLog = "", cLog = "";
  const server = spawn(PYTHON, [join(ROOT, "scripts/serve-web.py"), String(sitePort)], { cwd: ROOT, detached: true, stdio: ["ignore", "pipe", "pipe"] });
  server.stdout.on("data", (c) => (sLog += c)); server.stderr.on("data", (c) => (sLog += c));
  const chrome = spawn(CHROME, flags(cdpPort, profile), { detached: true, stdio: ["ignore", "pipe", "pipe"] });
  chrome.stdout.on("data", (c) => (cLog += c)); chrome.stderr.on("data", (c) => (cLog += c));
  let cdp;
  try {
    await waitFor("site server", 20000, async () => (await fetch(`${base}/`)).ok);
    await waitFor("CDP", 30000, async () => (await fetch(`http://127.0.0.1:${cdpPort}/json/version`)).ok);
    // Create a blank target and navigate it ourselves: the page budget must be
    // on `window` before the embed module runs, and `/json/new?<page>` would
    // race the module's first `allocateBudget` call.
    let resp = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent("about:blank")}`, { method: "PUT" });
    if (!resp.ok) resp = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent("about:blank")}`);
    const target = await resp.json();
    cdp = new Cdp(target.webSocketDebuggerUrl);
    await cdp.open();
    await cdp.send("Runtime.enable");
    await cdp.send("Page.enable");
    if (pageBudget) {
      await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
        source: `window.__amxPageRasterPixels = ${pageBudget};`,
      });
    }
    await cdp.send("Page.navigate", { url: `${base}${PAGE}` });
    await waitFor("__adaptive", 120000, async () => {
      const { result } = await cdp.send("Runtime.evaluate", { expression: "!!window.__adaptive", returnByValue: true });
      return result.value === true;
    });
    // The page publishes its own summary; page-raster accounting is added here
    // from the live elements, because the probe page is not the only consumer
    // of the embed and the budget belongs to the page, not to one figure.
    // Tolerant reads: a pre-budget bundle has no `rasterPixels`/`_rasterPixels`,
    // so fall back to the engine's own raster size rather than throwing.
    const summaryExpr = `(() => {
      const s = (window.__adaptive && window.__adaptive.summary) || {};
      const els = Array.from(document.querySelectorAll("amx-player"));
      let pageRasterPixels = 0;
      let rasterPlayers = 0;
      let pixelSum = 0;
      let pixelCount = 0;
      for (const el of els) {
        // Mirror the controller's active set: an element that has not built a
        // scene holds no raster, so its placeholder canvas must not be counted.
        if (el._state !== "ready") continue;
        let px = NaN;
        try {
          const v = el.rasterPixels;
          if (typeof v === "number" && Number.isFinite(v)) px = v;
        } catch { /* getter unavailable */ }
        if (!Number.isFinite(px)) {
          try { px = el._rasterPixels(); } catch { /* not this bundle */ }
        }
        if (!Number.isFinite(px)) {
          try {
            const p = el._player;
            if (p && typeof p.raster_width === "function") {
              px = p.raster_width() * p.raster_height();
            }
          } catch { /* player gone */ }
        }
        if (!Number.isFinite(px) || px <= 0) continue;
        pageRasterPixels += px;
        rasterPlayers += 1;
        let ps = NaN;
        try { ps = el.pixelScale; } catch { /* getter unavailable */ }
        if (!Number.isFinite(ps)) { try { ps = el._pixelScale; } catch { /* ignore */ } }
        if (Number.isFinite(ps)) { pixelSum += ps; pixelCount += 1; }
      }
      return JSON.stringify({
        ...s,
        pageRasterPixels: rasterPlayers ? pageRasterPixels : null,
        avgPixelScale: pixelCount ? pixelSum / pixelCount : null,
        rasterPlayers,
      });
    })()`;
    const { result } = await cdp.send("Runtime.evaluate", { expression: summaryExpr, returnByValue: true });
    console.log(result.value);
    let summary = {};
    try { summary = JSON.parse(result.value); } catch { /* printed raw above */ }
    // Identify what actually ran so nobody mistakes SwiftShader fps for a
    // real-GPU number, or a synthetic manual-tick cadence for real display rAF.
    const mode = manual === "0" ? "real-rAF" : "manual-tick";
    const adapter = SOFTWARE ? "SOFTWARE SwiftShader (HB_SOFTWARE=1)" : "real GPU";
    let achieved = "";
    if (typeof summary?.achievedFps === "number") achieved = ` achievedFps=${summary.achievedFps.toFixed(2)}`;
    console.error(`[adaptive-probe] mode=${mode} | adapter=${adapter}${achieved}${SOFTWARE ? " | NOTE: SwiftShader fps is NOT a real-GPU number" : ""}`);
    if (pageBudget) {
      // The assertion is the point of AP_PAGE_PIXELS: a run that prints a
      // number but exits 0 regardless cannot gate anything. 10% slack covers
      // the difference between the controller's model (scene px × display
      // scale) and the engine's rounded raster size.
      const actual = summary?.pageRasterPixels;
      const limit = pageBudget * 1.1;
      const scaleText = typeof summary?.avgPixelScale === "number"
        ? summary.avgPixelScale.toFixed(3)
        : "n/a";
      if (typeof actual === "number" && Number.isFinite(actual)) {
        if (actual <= limit) {
          console.error(
            `[adaptive-probe] page budget ok: pageRasterPixels=${Math.round(actual)} <= ${Math.round(limit)} ` +
            `(AP_PAGE_PIXELS=${pageBudget}, avgPixelScale=${scaleText})`,
          );
        } else {
          console.error(
            `[adaptive-probe] FAIL: pageRasterPixels=${Math.round(actual)} > ${Math.round(limit)} ` +
            `(AP_PAGE_PIXELS=${pageBudget}, avgPixelScale=${scaleText})`,
          );
          process.exitCode = 1;
        }
      } else {
        console.error(
          `[adaptive-probe] FAIL: pageRasterPixels unavailable (rasterPlayers=${summary?.rasterPlayers ?? 0}); ` +
          `cannot verify AP_PAGE_PIXELS=${pageBudget}`,
        );
        process.exitCode = 1;
      }
    }
    if (mode === "real-rAF" && !HEADED) {
      console.error("[adaptive-probe] WARNING: real-rAF requires a headed window (HB_HEADED=1 / AP_HEADED=1); headless/backgrounded tabs deliver no rAF callbacks.");
    }
    const errs = await cdp.send("Runtime.evaluate", { expression: "JSON.stringify((window.__adaptive && window.__adaptive.errors)||[])", returnByValue: true });
    if (errs.result.value && errs.result.value !== "[]") console.error("page errors:", errs.result.value.slice(0, 1000));
  } catch (e) {
    if (sLog) console.error("--- serve ---\n" + sLog.slice(-1200));
    if (cLog) console.error("--- chromium ---\n" + cLog.slice(-1200));
    throw e;
  } finally {
    if (cdp) cdp.close();
    killGroup(chrome); killGroup(server);
    await sleep(200);
    rmSync(profile, { recursive: true, force: true });
  }
}

main().catch((e) => { console.error("adaptive-probe: FAILED:", e.message); process.exitCode = 1; });
