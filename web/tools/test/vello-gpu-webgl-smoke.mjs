#!/usr/bin/env node
// Opt-in candidate check: WebGL2 on mobile-sized Chromium, with WebGPU removed.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { Cdp, instrumentation, navigationCommitted, until } from './web-smoke-browser.mjs';
import { inspectPng } from './web-smoke-pixels.mjs';

const base = process.env.WEBGL_CANDIDATE_URL ?? 'http://127.0.0.1:8124';
const mode = process.env.WEBGL_CANDIDATE_MODE ?? 'auto';
assert(['auto', 'webgl2'].includes(mode), 'expected automatic or forced WebGL2 selection');
const url = new URL('/dev/vello-gpu-candidate.html', base);
url.searchParams.set('backend', mode);
const sdkRoot = resolve(process.env.ANIMATIX_WEB_ROOT ?? new URL('../../../web', import.meta.url).pathname);
const manifestPath = join(sdkRoot, 'runtime', 'runtime-manifest.json');
assert(existsSync(manifestPath), `SDK missing in ${sdkRoot}; build/restore the candidate SDK first`);
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
assert(manifest.render_backends?.includes('webgl2'), 'runtime manifest does not include the opt-in WebGL2 backend');
const profile = mkdtempSync(join(tmpdir(), 'amx-vello-gpu-webgl-'));
const chrome = spawn(process.env.CHROME ?? 'chromium', [
  '--headless', '--disable-dev-shm-usage', '--no-sandbox', '--disable-crashpad-for-testing',
  '--disable-background-networking', '--disable-webgpu', '--use-gl=angle', '--use-angle=swiftshader',
  '--enable-unsafe-swiftshader', '--enable-features=WebAssemblyUnlimitedSyncCompilation',
  '--remote-debugging-port=0',
  ...(process.env.WEBGL_CHROME_SINGLE_PROCESS === '1' ? ['--single-process'] : []),
  `--user-data-dir=${profile}`, 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let log = '';
chrome.stderr.setEncoding('utf8').on('data', chunk => { log = (log + chunk).slice(-10000); });
const browser = (cdp, fn, ...args) => cdp.evaluate(`(${fn.toString()})(...${JSON.stringify(args)})`);

async function connect() {
  const portFile = join(profile, 'DevToolsActivePort');
  let tabs;
  await until('Chromium CDP endpoint', async () => {
    if (chrome.exitCode !== null) throw Error(`Chromium exited: ${log}`);
    if (!existsSync(portFile)) return false;
    const [port] = readFileSync(portFile, 'utf8').split('\n');
    try { tabs = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json(); }
    catch { return false; }
    return tabs.some(tab => tab.type === 'page');
  }, 15000);
  const cdp = new Cdp(tabs.find(tab => tab.type === 'page').webSocketDebuggerUrl);
  await cdp.open();
  return cdp;
}

async function frame(cdp, time) {
  const rect = await browser(cdp, async t => {
    const player = document.querySelector('amx-player');
    player.pause();
    player.seek(t);
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const bounds = player.shadowRoot.querySelector('canvas').getBoundingClientRect();
    return { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height, dpr: devicePixelRatio };
  }, time);
  const shot = await cdp.send('Page.captureScreenshot', { format: 'png' });
  const png = inspectPng(Buffer.from(shot.data, 'base64'));
  return (x, y) => {
    const px = Math.floor((rect.x + rect.width * x) * rect.dpr);
    const py = Math.floor((rect.y + rect.height * y) * rect.dpr);
    const index = (py * png.width + px) * 4;
    return Array.from(png.pixels.subarray(index, index + 3));
  };
}

function near(actual, expected, tolerance = 3) {
  assert(actual.length === expected.length && actual.every((channel, i) => Math.abs(channel - expected[i]) <= tolerance),
    JSON.stringify({ actual, expected }));
}

const browserErrors = [];
let cdp;
try {
  cdp = await connect();
  cdp.onEvent = event => {
    if (event.method === 'Runtime.consoleAPICalled' && event.params.type === 'error') {
      browserErrors.push(event.params.args.map(arg => arg.value ?? arg.description).join(' '));
    }
  };
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width: 393, height: 851, deviceScaleFactor: 2, mobile: true,
  });
  await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 });
  await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
    source: instrumentation + `
      delete Navigator.prototype.gpu;
      window.webglReadbacks = 0;
      for (const name of ['readPixels', 'getBufferSubData']) {
        const method = WebGL2RenderingContext.prototype[name];
        WebGL2RenderingContext.prototype[name] = function (...args) {
          window.webglReadbacks++;
          return Reflect.apply(method, this, args);
        };
      }
    `,
  });
  await cdp.send('Page.navigate', { url: url.href });
  await navigationCommitted(cdp, url.href);
  await until('candidate first frame', async () => {
    const status = await cdp.evaluate(`({ ready: __smoke.events.length, errors: __smoke.errors, backend: document.querySelector('amx-player')?.backend })`);
    assert.deepEqual(status.errors, []);
    return status.ready === 1 && status.backend === 'webgl2';
  });
  assert.equal(await cdp.evaluate(`'gpu' in navigator`), false);
  const left = await frame(cdp, 0);
  const right = await frame(cdp, 2.5);
  near(left(360 / 1280, 0.5), [101, 217, 232]);
  near(right(920 / 1280, 0.5), [101, 217, 232]);
  near(right(0.05, 0.05), [13, 21, 32]);
  assert.equal(await browser(cdp, async () => {
    try { await document.querySelector('amx-player').applySource('rect:::'); }
    catch { return true; }
    return false;
  }), true);
  near((await frame(cdp, 2.5))(920 / 1280, 0.5), [101, 217, 232]);

  await browser(cdp, async () => {
    const source = await (await fetch('/dev/vello-gpu-glass.amx')).text();
    await document.querySelector('amx-player').applySource(source);
  });
  const frosted = await frame(cdp, 0.4);
  const blend = await frame(cdp, 1.5);
  const after = await frame(cdp, 2.6);
  near(frosted(0.05, 0.05), [16, 24, 39]);
  near(frosted(0.26, 0.22), [252, 120, 108]);
  near(frosted(0.245, 0.22), [252, 120, 108]);
  assert(frosted(0.245, 0.4)[0] < 245, 'Glass did not soften the bar behind the panel');
  near(frosted(0.5, 0.5), [255, 255, 255]);
  near(blend(0.05, 0.05), [34, 29, 56]);
  near(after(0.05, 0.05), [51, 34, 72]);

  await browser(cdp, async () => {
    const source = await (await fetch('/dev/vello-gpu-inline.amx')).text();
    await document.querySelector('amx-player').applySource(source);
  });
  const inline = await frame(cdp, 1);
  near(inline(0.5, 0.5), [252, 120, 108]);
  const blurredEdge = inline(0.385, 0.5);
  assert(blurredEdge[1] > 55 && blurredEdge[1] < 215, `inline Blur did not soften the edge: ${blurredEdge}`);

  const lostCanvas = await browser(cdp, async () => {
    const canvas = document.querySelector('amx-player').shadowRoot.querySelector('canvas');
    const gl = canvas.getContext('webgl2');
    const extension = gl?.getExtension('WEBGL_lose_context');
    if (!extension) return false;
    window.__contextLoss = { canvas, extension };
    extension.loseContext();
    return true;
  });
  assert.equal(lostCanvas, true, 'SwiftShader did not expose WEBGL_lose_context');
  await until('WebGL2 context recovery', async () => cdp.evaluate(`
    __candidate.events.some(event => event.type === 'amxbackendrecovered') &&
    document.querySelector('amx-player')?.shadowRoot.querySelector('canvas') !== __contextLoss.canvas
  `));
  assert.equal(await cdp.evaluate(`document.querySelector('amx-player').backend`), 'webgl2');
  assert.equal(await cdp.evaluate(`document.querySelector('amx-player').time`), 1);
  near((await frame(cdp, 1))(0.5, 0.5), [252, 120, 108]);
  assert.equal(await cdp.evaluate('webglReadbacks'), 0, 'player rendering used a WebGL readback');
  const readback = await browser(cdp, () => new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(Error('WebGL2 readback callback timed out')), 7500);
    document.querySelector('amx-player')._player.debug_readback_rgba(1, result => {
      clearTimeout(timeout);
      if (typeof result === 'string') { reject(Error(result)); return; }
      const { width, height, bytes } = result;
      const center = (Math.floor(height / 2) * width + Math.floor(width / 2)) * 4;
      resolve({ width, height, length: bytes.length, center: Array.from(bytes.slice(center, center + 4)) });
    });
  }));
  assert.deepEqual([readback.width, readback.height, readback.length], [640, 360, 640 * 360 * 4]);
  near(readback.center, [252, 120, 108, 255]);
  assert.deepEqual(await cdp.evaluate('__smoke.errors'), []);
  assert.deepEqual(browserErrors, []);
  console.log('Vello GPU WebGL2 candidate passed: playback, edit rollback, Glass, transition, Filter, context recovery, RGBA readback');
} catch (error) {
  console.error(browserErrors.join('\n'));
  throw error;
} finally {
  cdp?.close();
  await new Promise(resolve => {
    if (chrome.exitCode !== null) return resolve();
    chrome.on('exit', resolve);
    chrome.kill();
  });
  rmSync(profile, { recursive: true, force: true });
}
