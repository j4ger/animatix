#!/usr/bin/env node
// Headless driver for web/dev/hybrid-vs-compute.html: loads the page in
// headless Chromium, runs the wasm benchmarks, and prints the JSON results.
//
// Dev aid only (not a gate). Software rendering means the absolute numbers are
// not representative — this exists to prove the page and both backends run
// end-to-end and to expose the measurement shape.
//
// Usage: scripts/hybrid-bench-smoke.mjs
//   HB_SCENES=light,dense HB_RES=640x360 HB_FRAMES=10 HB_WARMUP=2 HB_REPS=1
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import net from "node:net";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const CHROME = process.env.CHROME || "chromium";
const PYTHON = process.env.PYTHON || "python3";
const PAGE = "/dev/hybrid-vs-compute.html";
const SCENES = (process.env.HB_SCENES || "light").split(",");
const [W, H] = (process.env.HB_RES || "640x360").split("x").map(Number);
const FRAMES = Number(process.env.HB_FRAMES || 10);
const WARMUP = Number(process.env.HB_WARMUP || 2);
const REPS = Number(process.env.HB_REPS || 1);
const BACKENDS = (process.env.HB_BACKENDS || "compute,hybrid").split(",");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitFor(what, timeoutMs, probe) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      if (await probe()) return;
    } catch (e) {
      last = e;
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
    this.events = [];
  }
  async open() {
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
    this.ws.addEventListener("message", (event) => {
      const m = JSON.parse(event.data);
      if (m.method) this.events.push(m);
      const p = this.pending.get(m.id);
      if (!p) return;
      this.pending.delete(m.id);
      if (m.error) p.reject(new Error(JSON.stringify(m.error)));
      else p.resolve(m.result);
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
    try { this.ws.close(); } catch {}
  }
}

async function freePort() {
  return await new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const p = srv.address().port;
      srv.close(() => resolve(p));
    });
  });
}

function killGroup(child) {
  if (!child || child.exitCode !== null) return;
  try { process.kill(-child.pid, "SIGKILL"); } catch { try { child.kill("SIGKILL"); } catch {} }
}

// Software rendering is the default (works headless anywhere). Opt into the
// real GPU with HB_GPU=1 (headless, needs --use-vulkan=native) or, better,
// HB_HEADED=1 (a real window; on Linux WebGPU needs --enable-unsafe-webgpu,
// supplied by HB_UNSAFE_WEBGPU=1).
const GPU = process.env.HB_GPU === "1" || process.env.HB_HEADED === "1";
const SOFTWARE = process.env.HB_SOFTWARE === "1" || !GPU;
const HEADED = process.env.HB_HEADED === "1";

const CHROME_FLAGS = (cdpPort, profile) => {
  const flags = [
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--disable-gpu-sandbox",
    "--enable-features=Vulkan,WebGPUService",
    "--ignore-gpu-blocklist",
    "--no-first-run",
    "--no-default-browser-check",
    "--mute-audio",
    "--disable-background-networking",
    "--disable-extensions",
  ];
  if (SOFTWARE) {
    flags.push("--enable-unsafe-webgpu", "--enable-unsafe-swiftshader", "--use-gl=angle", "--use-angle=swiftshader", "--use-vulkan=swiftshader");
  } else {
    // Hardware: `--use-vulkan=native` keeps Dawn (WebGPU) on the real GPU.
    // NB: do NOT pass --enable-unsafe-webgpu here — it is what exposes the CPU
    // (SwiftShader) WebGPU adapter. Headed and headless differ in how WebGL is
    // created: headless needs ANGLE pointed at Vulkan, headed must fall back to
    // Chromium's default ANGLE choice (forcing Vulkan there yields no context).
    flags.push("--use-vulkan=native");
    if (!HEADED) flags.push("--use-gl=angle", "--use-angle=vulkan");
    if (process.env.HB_UNSAFE_WEBGPU === "1") flags.push("--enable-unsafe-webgpu");
  }
  if (process.env.HB_EXTRA_FLAGS) flags.push(...process.env.HB_EXTRA_FLAGS.split(/\s+/).filter(Boolean));
  if (!HEADED) flags.unshift("--headless=new");
  flags.push(`--remote-debugging-port=${cdpPort}`, `--user-data-dir=${profile}`, "about:blank");
  return flags;
};

async function main() {
  const pkg = join(ROOT, "web/dev/pkg-hybrid-bench/animatix_hybrid_bench.js");
  if (!existsSync(pkg)) {
    throw new Error("web/dev/pkg-hybrid-bench/ missing — run scripts/build-hybrid-bench.sh --dev first");
  }

  const sitePort = await freePort();
  const cdpPort = await freePort();
  const base = `http://127.0.0.1:${sitePort}`;
  const profile = mkdtempSync(join(tmpdir(), "amx-hb-"));

  let serverLog = "", chromeLog = "", consoleLog = "";
  const server = spawn(PYTHON, [join(ROOT, "scripts/serve-web.py"), String(sitePort)], {
    cwd: ROOT, detached: true, stdio: ["ignore", "pipe", "pipe"],
  });
  server.stdout.on("data", (c) => (serverLog += c));
  server.stderr.on("data", (c) => (serverLog += c));

  const chrome = spawn(CHROME, CHROME_FLAGS(cdpPort, profile), {
    detached: true, stdio: ["ignore", "pipe", "pipe"],
  });
  chrome.stdout.on("data", (c) => (chromeLog += c));
  chrome.stderr.on("data", (c) => (chromeLog += c));

  let cdp;
  try {
    await waitFor("site server", 20000, async () => (await fetch(`${base}/`)).ok);
    await waitFor("CDP", 30000, async () => (await fetch(`http://127.0.0.1:${cdpPort}/json/version`)).ok);

    let resp = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(`${base}${PAGE}`)}`, { method: "PUT" });
    if (!resp.ok) resp = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(`${base}${PAGE}`)}`);
    if (!resp.ok) throw new Error(`create target HTTP ${resp.status}`);
    const target = await resp.json();
    cdp = new Cdp(target.webSocketDebuggerUrl);
    cdp.onEvent = (m) => {
      if (m.method === "Runtime.consoleAPICalled") {
        consoleLog += `[${m.params.type}] ${(m.params.args || []).map((a) => a.value ?? a.description ?? "").join(" ")}\n`;
      } else if (m.method === "Runtime.exceptionThrown") {
        consoleLog += `[exception] ${m.params.exceptionDetails?.text || ""} ${m.params.exceptionDetails?.exception?.description || ""}\n`;
      }
    };
    await cdp.open();
    await cdp.send("Runtime.enable");
    await cdp.send("Page.enable");

    await waitFor("page ready", 120000, async () => {
      const { result } = await cdp.send("Runtime.evaluate", {
        expression: "!!(window.__bench && window.__bench.ready)", returnByValue: true,
      });
      return result.value === true;
    });

    const caps = (await cdp.send("Runtime.evaluate", {
      expression: `(async () => {
        const out = { webgpu: window.__bench.webgpu, webgl2: window.__bench.webgl2, adapter: null, glRenderer: null };
        try {
          if (navigator.gpu) {
            const a = await navigator.gpu.requestAdapter();
            if (a && a.info) out.adapter = { vendor: a.info.vendor, architecture: a.info.architecture, device: a.info.device, description: a.info.description, fallback: a.isFallbackAdapter };
            const hp = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
            if (hp && hp.info) out.hpAdapter = { vendor: hp.info.vendor, architecture: hp.info.architecture, device: hp.info.device, description: hp.info.description, fallback: hp.isFallbackAdapter };
          }
        } catch (e) { out.adapterError = String(e); }
        try {
          const c = document.createElement('canvas');
          const gl = c.getContext('webgl2');
          if (gl) {
            const ext = gl.getExtension('WEBGL_debug_renderer_info');
            out.glRenderer = ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
          }
        } catch (e) { out.glError = String(e); }
        return out;
      })()`,
      awaitPromise: true,
      returnByValue: true,
    })).result.value;
    console.log("caps:", JSON.stringify(caps));
    console.log("mode:", SOFTWARE ? "software(swiftshader)" : `hardware${HEADED ? " headed" : " headless"}`);

    for (const backend of BACKENDS) {
      if (backend === "compute" && !caps.webgpu) { console.log("skip compute: no WebGPU"); continue; }
      if (backend === "hybrid" && !caps.webgl2) { console.log("skip hybrid: no WebGL2"); continue; }
      for (const scene of SCENES) {
        const expr = `window.__runOne(${JSON.stringify(backend)}, ${JSON.stringify(scene)}, ${W}, ${H}, ${FRAMES}, ${WARMUP}, ${REPS})`;
        const r = await cdp.send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
        if (r.exceptionDetails) {
          console.log(`${backend}/${scene}: EXCEPTION`, JSON.stringify(r.exceptionDetails));
        }
      }
    }

    const { result } = await cdp.send("Runtime.evaluate", {
      expression: "JSON.stringify(window.__bench.results, null, 2)", returnByValue: true,
    });
    console.log("RESULTS:", result.value);
  } catch (e) {
    if (serverLog) console.error("--- serve ---\n" + serverLog.slice(-1500));
    if (consoleLog) console.error("--- console ---\n" + consoleLog.slice(-3000));
    if (chromeLog) console.error("--- chromium ---\n" + chromeLog.slice(-1500));
    throw e;
  } finally {
    if (cdp) cdp.close();
    killGroup(chrome);
    killGroup(server);
    await sleep(200);
    rmSync(profile, { recursive: true, force: true });
  }
}

main().catch((e) => { console.error("hybrid-bench-smoke: FAILED:", e.message); process.exitCode = 1; });
