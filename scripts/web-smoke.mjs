#!/usr/bin/env node
// web-smoke — load the real site in headless Chromium and assert that every
// <amx-player> reaches `ready`.
//
// Why this exists: every static gate can pass while the player is broken. The
// embed's module split once left a bare `qualityStep` reference that esbuild
// happily bundled; `site-scenes` built the scenes natively, `embed-drift` built
// the bundle, and `web-build` only checked that two wasm files existed — all
// green while every page showed "Failed to load scene". This gate closes that
// gap: it serves web/, drives Chromium through CDP, injects amxready/amxerror
// listeners before the document scripts run, and fails on the first error,
// timeout, or missing canvas.
//
// Chromium's SwiftShader adapter supplies WebGPU without a host GPU, so the
// `.#web-smoke` dev shell (chromium + node + python3) is enough.
//
// No third-party dependencies: Node 22+ has fetch and WebSocket.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import net from "node:net";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const CHROME = process.env.CHROME || "chromium";
const PYTHON = process.env.PYTHON || "python3";

// Pages that exercise the three load shapes: the full-profile hero grid, a
// single walkthrough player, and a slim-profile demo with several players.
const PAGES = process.env.SMOKE_PAGES
  ? process.env.SMOKE_PAGES.split(",")
  : ["/", "/demos/transformer/index.html", "/demos/matrix/index.html"];

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

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

// Runs inside the page. The page-side deadline is generous: the first load
// compiles a multi-megabyte wasm module and builds every scene on the page.
function pageProbe() {
  return (async () => {
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    if (!navigator.gpu) return { error: "navigator.gpu is undefined (no WebGPU)" };
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) return { error: "no WebGPU adapter" };

    const players = [...document.querySelectorAll("amx-player")];
    if (!players.length) return { error: "no <amx-player> element on the page" };

    // Players below the fold load on intersection, and the loader deliberately
    // drops a queued player that has already been scrolled past. Walk them one
    // at a time and wait for each: that is what a reader does, and it makes the
    // gate deterministic instead of a race with the scroll position.
    const results = [];
    const globalDeadline = Date.now() + 180000;
    for (const player of players) {
      const src = player.getAttribute("src");
      const deadline = Math.min(globalDeadline, Date.now() + 45000);
      player.scrollIntoView({ block: "center", behavior: "instant" });
      while (player._state !== "ready") {
        const smoke = window.__amxSmoke || { errors: [] };
        if (smoke.errors.length) return { error: `amxerror on ${src}: ${smoke.errors[0]}` };
        if (player._state === "error") return { error: `player error on ${src}` };
        if (Date.now() > deadline) {
          return {
            error: `timed out waiting for ${src} to be ready (state: ${player._state})`,
            states: players.map((p) => p._state),
          };
        }
        await sleep(100);
      }
      // The first frame follows ready; give it a beat while still on screen,
      // then record whether the canvas actually painted (a blank canvas after
      // ready is its own regression).
      await sleep(300);
      const canvas = player.shadowRoot && player.shadowRoot.querySelector("canvas");
      if (!canvas) return { error: `${src}: no canvas in the player's shadow root` };
      let bytes;
      try {
        bytes = canvas.toDataURL("image/png").length;
      } catch {
        bytes = -1; // WebGPU canvases may refuse readback; not a paint failure
      }
      results.push({ src, bytes });
    }
    return {
      players: players.length,
      canvases: results.map((result) => result.bytes),
      sources: results.map((result) => result.src),
    };
  })();
}

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

async function main() {
  if (!existsSync(join(ROOT, "web/pkg/animatix_web_bg.wasm"))) {
    throw new Error("web/pkg/animatix_web_bg.wasm is missing — run scripts/ci.sh gate web-build first");
  }

  const sitePort = await freePort();
  const cdpPort = await freePort();
  const base = `http://127.0.0.1:${sitePort}`;
  const profile = mkdtempSync(join(tmpdir(), "amx-smoke-"));

  let serverLog = "";
  let chromeLog = "";
  const server = spawn(PYTHON, [join(ROOT, "scripts/serve-web.py"), String(sitePort)], {
    cwd: ROOT,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  server.stdout.on("data", (chunk) => (serverLog += chunk));
  server.stderr.on("data", (chunk) => (serverLog += chunk));

  const chrome = spawn(
    CHROME,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--disable-gpu-sandbox",
      "--enable-unsafe-webgpu",
      // Dawn still initializes Vulkan with no host GPU; disabling the surface
      // is what lets that work headless on the lavapipe ICD in .#web-smoke.
      "--disable-vulkan-surface",
      "--use-angle=vulkan",
      "--enable-features=Vulkan",
      "--no-first-run",
      "--no-default-browser-check",
      "--mute-audio",
      "--disable-background-networking",
      "--disable-extensions",
      `--remote-debugging-port=${cdpPort}`,
      `--user-data-dir=${profile}`,
      "about:blank",
    ],
    { detached: true, stdio: ["ignore", "pipe", "pipe"] },
  );
  chrome.stdout.on("data", (chunk) => (chromeLog += chunk));
  chrome.stderr.on("data", (chunk) => (chromeLog += chunk));

  let cdp;
  let consoleLog = "";
  try {
    await waitFor("the local site server", 20000, async () => {
      const response = await fetch(`${base}/`);
      return response.ok;
    });
    await waitFor("Chromium's CDP endpoint", 30000, async () => {
      const response = await fetch(`http://127.0.0.1:${cdpPort}/json/version`);
      return response.ok;
    });

    let response = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(`${base}${PAGES[0]}`)}`, {
      method: "PUT",
    });
    if (!response.ok) {
      response = await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(`${base}${PAGES[0]}`)}`);
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
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `
        window.__amxSmoke = { errors: [], ready: 0 };
        addEventListener("amxerror", (event) => {
          const detail = event.detail || {};
          let text = "amxerror";
          if (detail.message) text = String(detail.message);
          else if (detail.error) text = detail.error.message || String(detail.error);
          else if (detail.diagnostics) {
            try { text = JSON.stringify(detail.diagnostics).slice(0, 800); } catch { /* keep default */ }
          } else {
            try { text = JSON.stringify(detail).slice(0, 800); } catch { /* keep default */ }
          }
          window.__amxSmoke.errors.push(text);
        }, true);
        addEventListener("amxready", () => { window.__amxSmoke.ready += 1; }, true);
      `,
    });

    for (const path of PAGES) {
      const url = `${base}${path}`;
      process.stdout.write(`web-smoke: ${path} `);
      await cdp.send("Page.navigate", { url });
      await waitFor(`load of ${path}`, 45000, async () => {
        const { result } = await cdp.send("Runtime.evaluate", {
          expression: "document.readyState",
          returnByValue: true,
        });
        return result.value === "complete";
      });

      const { result } = await cdp.send("Runtime.evaluate", {
        expression: `(${pageProbe.toString()})()`,
        awaitPromise: true,
        returnByValue: true,
      });
      const outcome = result.value || {};
      if (outcome.error) {
        throw new Error(`${path}: ${outcome.error}${outcome.states ? ` (states: ${outcome.states.join(",")})` : ""}${outcome.info ? `\n${JSON.stringify(outcome.info, null, 2)}` : ""}`);
      }
      const canvases = outcome.canvases || [];
      if (canvases.length !== outcome.players || canvases.some((size) => size === 0)) {
        throw new Error(`${path}: a player has no canvas (${canvases.join(",")})`);
      }
      const painted = canvases.filter((size) => size > 4000).length;
      if (painted === 0 && canvases.some((size) => size > 0)) {
        throw new Error(`${path}: every canvas is blank after ready (${canvases.join(",")})`);
      }
      console.log(`ok (${outcome.players} players, ${painted} painted, canvas bytes ${canvases.join(",")})`);
    }
    console.log("web-smoke: ok");
  } catch (error) {
    if (serverLog) console.error(`--- serve-web.py ---\n${serverLog.slice(-2000)}`);
    if (consoleLog) console.error(`--- page console ---\n${consoleLog.slice(-4000)}`);
    if (chromeLog) console.error(`--- chromium ---\n${chromeLog.slice(-2000)}`);
    throw error;
  } finally {
    if (cdp) cdp.close();
    killGroup(chrome);
    killGroup(server);
    await sleep(200);
    rmSync(profile, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`web-smoke: FAILED: ${error.message}`);
  process.exitCode = 1;
});
