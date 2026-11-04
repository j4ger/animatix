#!/usr/bin/env node
// Drives web/dev/adaptive-probe.html in Chromium and prints the adaptive-quality
// summary it publishes on `window.__adaptive`.
//
// Env:
//   AP_BUNDLE=new|legacy  AP_N=<count>  AP_W=<cssW>  AP_H=<cssH>  AP_SCENE=<url>
//   HB_HEADED=1           real window on a real GPU (Linux WebGPU needs
//                         HB_UNSAFE_WEBGPU=1; run with the NVIDIA ICD env)
//   HB_SOFTWARE=1         force SwiftShader (headless, reproducible anywhere)
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
const HEADED = process.env.HB_HEADED === "1";
const SOFTWARE = process.env.HB_SOFTWARE === "1";
const UNSAFE = process.env.HB_UNSAFE_WEBGPU === "1";

const bundle = process.env.AP_BUNDLE || "new";
const n = process.env.AP_N || "12";
const w = process.env.AP_W || "320";
const h = process.env.AP_H || "180";
const scene = process.env.AP_SCENE || "./adaptive_probe.amx";
const manual = process.env.AP_MANUAL === "0" ? "0" : "1";
const PAGE = `/dev/adaptive-probe.html?bundle=${encodeURIComponent(bundle)}&n=${n}&w=${w}&h=${h}&scene=${encodeURIComponent(scene)}&manual=${manual}&amxmanualtick=1`;

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
    let resp = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(`${base}${PAGE}`)}`, { method: "PUT" });
    if (!resp.ok) resp = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(`${base}${PAGE}`)}`);
    const target = await resp.json();
    cdp = new Cdp(target.webSocketDebuggerUrl);
    await cdp.open();
    await cdp.send("Runtime.enable");
    await waitFor("__adaptive", 120000, async () => {
      const { result } = await cdp.send("Runtime.evaluate", { expression: "!!window.__adaptive", returnByValue: true });
      return result.value === true;
    });
    const { result } = await cdp.send("Runtime.evaluate", { expression: "JSON.stringify(window.__adaptive.summary)", returnByValue: true });
    console.log(result.value);
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
