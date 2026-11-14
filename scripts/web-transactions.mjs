#!/usr/bin/env node
// Browser acceptance against a complete SDK/site, including subpath hosting.
// SwiftShader proves function here; it is deliberately not a performance gate.
import assert from 'node:assert/strict';
import { Cdp } from './cdp-client.mjs';
import { compareRgbaFrames } from './rgba-comparison.mjs';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdtempSync, readFileSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join, resolve, extname, relative } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { deflateSync, gunzipSync, gzipSync, inflateSync } from 'node:zlib';
import { verify } from '../web/tools/sdk.mjs';
import { exerciseResourceInventory } from '../web/tools/resource-inventory-browser.mjs';

const ROOT = fileURLToPath(new URL('../', import.meta.url));
const SITE = resolve(process.env.ANIMATIX_WEB_ROOT || join(ROOT, 'web'));
const EVIDENCE = resolve(process.env.ANIMATIX_WEB_EVIDENCE || join(ROOT, 'target/perf/distribution-refactor/web-transactions'));
const manifest = verify(join(SITE, 'runtime'), { release: true });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const timeout = Number(process.env.ANIMATIX_WEB_TEST_TIMEOUT_MS || 600_000);
const hardwareGpu = process.env.ANIMATIX_WEB_GPU === 'hardware';
const CRC_TABLE = Array.from({ length: 256 }, (_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) value = (value & 1) ? (0xedb88320 ^ (value >>> 1)) : (value >>> 1);
  return value >>> 0;
});
function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) value = CRC_TABLE[(value ^ byte) & 0xff] ^ (value >>> 8);
  return (value ^ 0xffffffff) >>> 0;
}
function pngChunk(type, data) {
  const name = Buffer.from(type); const content = Buffer.concat([name, data]);
  const chunk = Buffer.allocUnsafe(12 + data.length);
  chunk.writeUInt32BE(data.length, 0); content.copy(chunk, 4);
  chunk.writeUInt32BE(crc32(content), 8 + data.length);
  return chunk;
}
function encodeFramePng(frame) {
  const rgba = Buffer.from(frame.rgba, 'base64');
  assert.equal(rgba.length, frame.width * frame.height * 4, 'GPU readback byte count is inconsistent');
  let hash = 2166136261;
  for (const byte of rgba) hash = Math.imul(hash ^ byte, 16777619) >>> 0;
  assert.equal(hash, frame.hash, 'GPU readback bytes do not match their recorded FNV hash');
  const stride = frame.width * 4; const filtered = Buffer.alloc(frame.height * (stride + 1));
  for (let row = 0; row < frame.height; row += 1) rgba.copy(filtered, row * (stride + 1) + 1, row * stride, (row + 1) * stride);
  const header = Buffer.alloc(13);
  header.writeUInt32BE(frame.width, 0); header.writeUInt32BE(frame.height, 4);
  header[8] = 8; header[9] = 6;
  const png = Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk('IHDR', header), pngChunk('IDAT', deflateSync(filtered)), pngChunk('IEND', Buffer.alloc(0)),
  ]);
  const idat = []; let cursor = 8;
  while (cursor < png.length) {
    const length = png.readUInt32BE(cursor); const type = png.toString('ascii', cursor + 4, cursor + 8);
    if (type === 'IDAT') idat.push(png.subarray(cursor + 8, cursor + 8 + length));
    cursor += length + 12;
  }
  const decoded = inflateSync(Buffer.concat(idat));
  assert.equal(decoded.length, filtered.length, 'encoded PNG scanline length changed');
  let decodedHash = 2166136261;
  for (let row = 0; row < frame.height; row += 1) {
    const line = row * (stride + 1);
    assert.equal(decoded[line], 0, 'PNG encoder emitted an unexpected row filter');
    const pixels = decoded.subarray(line + 1, line + stride + 1);
    assert.deepEqual(pixels, rgba.subarray(row * stride, (row + 1) * stride), 'encoded PNG pixels differ from GPU readback');
    for (const byte of pixels) decodedHash = Math.imul(decodedHash ^ byte, 16777619) >>> 0;
  }
  assert.equal(decodedHash, frame.hash, 'decoded PNG pixels do not match the GPU readback hash');
  return png;
}
const config = 'config { resolution: (128, 72), duration: 2 }\n';
const graphics = `${config}let side = 40\nlet tall = 24\nbox: Rect, size: (side, tall), color: (0.9, 0.1, 0.1, 1), at: (64, 36), opacity: 1\n`;
const solid = color => `${config}box: Rect, size: (80, 48), color: ${color}, at: (64, 36), opacity: 1\n`;
const text = value => `${config}label: Text, text: ${JSON.stringify(value)}, font_size: 14, text_max_width: 112, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`;
const svg = color => `<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect width="40" height="40" fill="${color}"/></svg>`;
const rasterPixels = Buffer.from([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
let rasterHash = 2166136261;
for (const byte of rasterPixels) rasterHash = Math.imul(rasterHash ^ byte, 16777619) >>> 0;
const rasterFixture = encodeFramePng({ width: 2, height: 2, rgba: rasterPixels.toString('base64'), hash: rasterHash });
const fixtures = new Map([
  ['base.amx', graphics],
  ['palette.amx', 'import "./nested/value.amx" as value\npub let tint = value.tint\n'],
  ['nested/value.amx', 'pub let tint = (0.1, 0.8, 0.2, 1)\n'],
  ['slow.amx', 'pub let tint = (0.2, 0.3, 0.9, 1)\n'],
  ['left/module.amx', 'pub component Left { icon: Svg, url: "icon.data", size: (40, 40), opacity: 1 }\n'],
  ['right/module.amx', 'pub component Right { icon: Svg, url: "icon.data", size: (40, 40), opacity: 1 }\n'],
  ['subtitles/caption.amx', 'pub component ChineseCaptions { subs: Captions, transcript: "valid.vtt", at: (64, 52), caption_mode: "line", color: (1, 1, 1, 1), font_size: 14, backdrop: false, opacity: 1 }\n'],
  ['subtitles/missing-caption.amx', 'pub component MissingCaptions { subs: Captions, transcript: "missing.vtt", at: (64, 52), caption_mode: "line", color: (1, 1, 1, 1), font_size: 14, backdrop: false, opacity: 1 }\n'],
  ['subtitles/malformed-caption.amx', 'pub component MalformedCaptions { subs: Captions, transcript: "malformed.vtt", at: (64, 52), caption_mode: "line", color: (1, 1, 1, 1), font_size: 14, backdrop: false, opacity: 1 }\n'],
  ['subtitles/valid.vtt', 'WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n你好，世界字幕\n'],
  ['subtitles/malformed.vtt', 'this is not a valid transcript\n'],
  ['left/icon.data', svg('red')], ['right/icon.data', svg('blue')],
  ['shared.data', svg('green')],
  ['raster.data', rasterFixture], ['broken-image.data', Buffer.from('invalid image payload')],
]);
const regular = manifest.fonts.faces.find(face => /OpenSans-Regular/.test(face.file || '')) || manifest.fonts.faces[0];
const overlay = regular && gunzipSync(readFileSync(join(SITE, 'runtime', regular.url)));
function widerFontMetrics(font, factor = 1.6) {
  const bytes = Buffer.from(font);
  const tables = new Map();
  const count = bytes.readUInt16BE(4);
  for (let index = 0; index < count; index += 1) {
    const record = 12 + index * 16;
    tables.set(bytes.toString('ascii', record, record + 4), {
      record, offset: bytes.readUInt32BE(record + 8), length: bytes.readUInt32BE(record + 12),
    });
  }
  const hhea = tables.get('hhea'); const hmtx = tables.get('hmtx'); const head = tables.get('head');
  assert.ok(hhea && hmtx && head, 'Open Sans fixture has horizontal metrics tables');
  const metrics = bytes.readUInt16BE(hhea.offset + 34);
  let maxAdvance = 0;
  for (let index = 0; index < metrics; index += 1) {
    const offset = hmtx.offset + index * 4;
    const advance = Math.min(65_535, Math.round(bytes.readUInt16BE(offset) * factor));
    bytes.writeUInt16BE(advance, offset); maxAdvance = Math.max(maxAdvance, advance);
  }
  bytes.writeUInt16BE(maxAdvance, hhea.offset + 10);
  const checksum = (offset, length) => {
    let sum = 0;
    for (let index = 0; index < length; index += 4) {
      const word = (bytes[offset + index] << 24)
        | ((bytes[offset + index + 1] || 0) << 16)
        | ((bytes[offset + index + 2] || 0) << 8)
        | (bytes[offset + index + 3] || 0);
      sum = (sum + (word >>> 0)) >>> 0;
    }
    return sum;
  };
  for (const table of [hhea, hmtx]) bytes.writeUInt32BE(checksum(table.offset, table.length), table.record + 4);
  bytes.writeUInt32BE(0, head.offset + 8);
  bytes.writeUInt32BE((0xB1B0AFBA - checksum(0, bytes.length)) >>> 0, head.offset + 8);
  return bytes;
}
const overlayWide = overlay && widerFontMetrics(overlay);
const requests = [];
const scenarioRequestCounts = new Map();
let phase = 'bootstrap';
let invalidOverlayRepaired = false;

function wasmWithChangedCustomName(bytes) {
  const mutated = Buffer.from(bytes);
  const readUleb = start => {
    let value = 0; let shift = 0; let offset = start;
    while (offset < mutated.length) {
      const byte = mutated[offset++]; value |= (byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) return { value: value >>> 0, offset };
      shift += 7;
    }
    throw new Error('truncated wasm section length');
  };
  let offset = 8;
  while (offset < mutated.length) {
    const id = mutated[offset++]; const section = readUleb(offset); offset = section.offset;
    const end = offset + section.value;
    if (end > mutated.length) throw new Error('invalid wasm section bounds');
    if (id === 0) {
      const name = readUleb(offset); const byte = name.offset;
      if (name.value && mutated[byte] < 0x7e) { mutated[byte] += 1; return mutated; }
    }
    offset = end;
  }
  throw new Error('capability fixture has no mutable custom section');
}

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (url.pathname === '/__phase') {
      phase = url.searchParams.get('name'); response.end('ok'); return;
    }
    if (url.pathname === '/__repair-invalid-overlay') {
      invalidOverlayRepaired = true; response.end('ok'); return;
    }
    if (url.pathname === '/preview/__acceptance__/index.html') {
      response.setHeader('Content-Type', 'text/html');
      response.end('<!doctype html><meta charset="utf-8"><title>SDK transaction acceptance</title><style>amx-player{display:block;width:256px;height:144px}</style><body></body>'); return;
    }
    const scenarioMatch = url.pathname.match(/^\/preview\/scenario\/([^/]+)\/runtime\/(.+)$/);
    if (scenarioMatch) {
      const [, scenario, file] = scenarioMatch;
      const entry = { phase, path: url.pathname, status: 200 }; requests.push(entry);
      response.setHeader('Cache-Control', 'no-store');
      const runtimeRoot = resolve(SITE, 'runtime');
      const path = resolve(runtimeRoot, decodeURIComponent(file));
      if (relative(runtimeRoot, path).startsWith('..') || !existsSync(path)) {
        entry.status = 404; response.writeHead(404); response.end('scenario artifact missing'); return;
      }
      const baseManifest = JSON.parse(readFileSync(join(runtimeRoot, 'runtime-manifest.json')));
      if (file === 'runtime-manifest.json') {
        const manifest = structuredClone(baseManifest);
        if (scenario === 'schema-mismatch') manifest.schema_version = 999;
        if (scenario === 'build-mismatch') manifest.build_id = 'f'.repeat(64);
        if (scenario === 'build-mismatch') {
          const entryBytes = readFileSync(join(runtimeRoot, baseManifest.entry), 'utf8');
          const oldRegistration = `animatix.sdk.engine.${baseManifest.build_id}`;
          const newRegistration = `animatix.sdk.engine.${manifest.build_id}`;
          if (!entryBytes.includes(oldRegistration)) throw new Error('entry registration token is missing');
          const modified = Buffer.from(entryBytes.replace(oldRegistration, newRegistration));
          manifest.files[manifest.entry].sha256 = createHash('sha256').update(modified).digest('hex');
          manifest.files[manifest.entry].size_bytes = modified.length;
        }
        response.setHeader('Content-Type', 'application/json'); response.end(JSON.stringify(manifest)); return;
      }
      if (scenario === 'sri-entry' && file === baseManifest.entry) {
        const corrupt = Buffer.from(readFileSync(path)); corrupt[corrupt.length - 1] ^= 1;
        response.setHeader('Content-Type', 'text/javascript'); response.end(corrupt); return;
      }
      if (scenario === 'build-mismatch' && file === baseManifest.entry) {
        const source = readFileSync(path, 'utf8');
        const modified = Buffer.from(source.replace(`animatix.sdk.engine.${baseManifest.build_id}`, `animatix.sdk.engine.${'f'.repeat(64)}`));
        response.setHeader('Content-Type', 'text/javascript'); response.end(modified); return;
      }
      if (scenario === 'capability-retry') {
        const mediaFile = baseManifest.capabilities.media.files[0];
        if (file === `${mediaFile}.gz`) {
          const count = scenarioRequestCounts.get(file) || 0; scenarioRequestCounts.set(file, count + 1);
          if (count === 0) {
            entry.corrupted = true;
            const corrupted = wasmWithChangedCustomName(readFileSync(join(runtimeRoot, mediaFile)));
            response.setHeader('Content-Type', 'application/gzip'); response.end(gzipSync(corrupted, { level: 9 })); return;
          }
        }
      }
      response.setHeader('Content-Type', ({ '.js': 'text/javascript', '.json': 'application/json', '.wasm': 'application/wasm', '.gz': 'application/gzip' })[extname(path)] || 'application/octet-stream');
      response.end(readFileSync(path)); return;
    }
    const entry = { phase, path: url.pathname, status: 200 };
    requests.push(entry);
    const fixture = url.pathname.replace('/preview/__acceptance__/', '');
    if (fixture === 'slow.amx' || fixture === 'shared.data' || fixture === 'slowfont.ttf' || fixture === 'retryfont.ttf') await sleep(500);
    if (fixture === 'retryfont.ttf' && !requests.some(item => item.path.endsWith('/retryfont.ttf') && item.status === 503)) {
      entry.status = 503; response.writeHead(503); response.end('temporary font failure'); return;
    }
    if (fixture === 'invalidfont.ttf') {
      response.setHeader('Content-Type', 'font/ttf');
      response.end(invalidOverlayRepaired ? overlay : Buffer.from('<!doctype html><html>not a font</html>'));
      return;
    }
    if ((fixture === 'slowfont.ttf' || fixture === 'retryfont.ttf') && overlay) {
      response.setHeader('Content-Type', 'font/ttf'); response.end(overlay); return;
    }
    if (fixture === 'overlay-a.ttf' || fixture === 'overlay-b.ttf') {
      response.setHeader('Content-Type', 'font/ttf');
      response.end(fixture === 'overlay-a.ttf' ? overlay : overlayWide); return;
    }
    if (fixtures.has(fixture)) {
      response.setHeader('Content-Type', Buffer.isBuffer(fixtures.get(fixture)) ? 'application/octet-stream' : fixture.endsWith('.data') ? 'image/svg+xml' : 'text/plain');
      response.end(fixtures.get(fixture)); return;
    }
    const path = resolve(SITE, '.' + decodeURIComponent(url.pathname.replace(/^\/preview/, '')));
    if (!url.pathname.startsWith('/preview/') || relative(SITE, path).startsWith('..') || !existsSync(path)) {
      entry.status = 404; response.writeHead(404); response.end('fixture or artifact file not found'); return;
    }
    response.setHeader('Content-Type', ({ '.js': 'text/javascript', '.json': 'application/json', '.wasm': 'application/wasm', '.gz': 'application/gzip' })[extname(path)] || 'application/octet-stream');
    response.end(readFileSync(path));
  } catch (error) { response.writeHead(500); response.end(error.message); }
});


async function waitFor(description, probe, limit = 30_000) {
  const end = Date.now() + limit;
  while (Date.now() < end) {
    try { const value = await probe(); if (value) return value; } catch { /* process/server may still be starting */ }
    await sleep(100);
  }
  throw new Error(`timed out: ${description}`);
}

// The only browser-side access to private state is observability: installed
// player/source identity, build count and GPU readback. Operations use the SDK.
async function browserTests(sources, exerciseInventory, compareFrames) {
  const results = [];
  const frames = {};
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const check = (condition, message) => { if (!condition) throw new Error(message); };
  const phase = name => fetch(`/__phase?name=${encodeURIComponent(name)}`);
  const within = (promise, message) => Promise.race([promise, new Promise((_, reject) => setTimeout(() => reject(new Error(message)), sources.timeout))]);
  async function until(probe, message) {
    const end = performance.now() + sources.timeout;
    while (performance.now() < end) { if (await probe()) return; await sleep(25); }
    throw new Error(message);
  }
  async function test(name, run) {
    await phase(name); window.__activeTransaction = name;
    try { await run(); }
    catch (error) {
      window.__transactionFailure = { name, message: error.message, stack: error.stack };
      throw error;
    }
    results.push(name); window.__transactions = [...results];
    console.log(`transaction passed: ${name}`);
  }
  async function create(extra = {}) {
    const player = document.createElement('amx-player');
    player.setAttribute('src', './base.amx'); player.setAttribute('eager', ''); player.setAttribute('sealed', '');
    for (const [key, value] of Object.entries(extra)) player.setAttribute(key, value);
    player._acceptanceReady = 0;
    const ready = new Promise((resolve, reject) => {
      player.addEventListener('amxready', () => { player._acceptanceReady += 1; resolve(player); });
      player.addEventListener('amxerror', event => reject(new Error(event.detail?.message || String(event.detail?.error))));
    });
    document.body.append(player);
    return { player, ready: within(ready, 'player did not become ready') };
  }
  async function isolatedPlayer(scenario) {
    const frame = document.createElement('iframe');
    frame.src = '/preview/__acceptance__/index.html'; document.body.append(frame);
    await within(new Promise(resolve => frame.addEventListener('load', resolve, { once: true })), 'scenario iframe did not load');
    const child = frame.contentDocument;
    const script = child.createElement('script'); script.type = 'module';
    script.src = `/preview/scenario/${scenario}/runtime/amx-player.js`;
    await within(new Promise((resolve, reject) => {
      script.addEventListener('load', resolve, { once: true });
      script.addEventListener('error', () => reject(new Error(`scenario SDK entry failed: ${scenario}`)), { once: true });
      child.head.append(script);
    }), 'scenario player module did not load');
    const player = child.createElement('amx-player');
    player.setAttribute('src', './base.amx'); player.setAttribute('eager', ''); player.setAttribute('sealed', '');
    let readyCount = 0;
    const ready = new Promise(resolve => player.addEventListener('amxready', () => { readyCount += 1; resolve(player); }));
    const error = new Promise(resolve => player.addEventListener('amxerror', resolve, { once: true }));
    child.body.append(player);
    return { frame, player, ready, error, readyCount: () => readyCount };
  }
  async function pixels(player) {
    return within(new Promise((resolve, reject) => {
      player._player.debug_readback_rgba(player.time, value => {
        if (value.error) { reject(new Error(value.error)); return; }
        let hash = 2166136261;
        for (const byte of value.bytes) hash = Math.imul(hash ^ byte, 16777619) >>> 0;
        resolve({ hash, width: value.width, height: value.height, bytes: Array.from(value.bytes) });
      });
    }), 'GPU readback did not finish');
  }
  function snapshot(name, frame) {
    let binary = '';
    for (let offset = 0; offset < frame.bytes.length; offset += 0x8000) {
      binary += String.fromCharCode(...frame.bytes.slice(offset, offset + 0x8000));
    }
    frames[name] = { width: frame.width, height: frame.height, rgba: btoa(binary), hash: frame.hash };
  }
  let p;
  let resourceInventory;
  window.__deviceRequests = 0;
  const createShader = GPUDevice.prototype.createShaderModule;
  GPUDevice.prototype.createShaderModule = function (descriptor) {
    if (String(descriptor.label || '').includes('Fullscreen Blit')) {
      const code = String(descriptor.code || '');
      console.log('fullscreen shader diagnostics', JSON.stringify({ length: code.length, fragment: code.includes('fn fs_main'), vertex: code.includes('fn main') }));
    }
    return createShader.call(this, descriptor);
  };
  check(typeof GPUAdapter !== 'undefined', 'WebGPU adapter interface unavailable');
  const adapter = await navigator.gpu.requestAdapter();
  check(adapter, 'WebGPU adapter unavailable');
  const adapterInfo = { vendor: adapter.info?.vendor, architecture: adapter.info?.architecture, device: adapter.info?.device, description: adapter.info?.description, isFallbackAdapter: adapter.isFallbackAdapter };
  const originalDevice = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = function (...args) { window.__deviceRequests += 1; return originalDevice.apply(this, args); };
  const sdk = await import('/preview/runtime/amx-player.js');
  await test('graphics-no-fonts-or-capabilities', async () => {
    const created = await create(); p = await created.ready;
    check(p._state === 'ready' && p._player.has_document(), 'ready fired before a document was installed');
    check(p.time === p.duration, 'paused initial document did not show its completion frame');
    window.__graphics = await pixels(p);
    const rgba = window.__graphics.bytes;
    const colors = new Set();
    for (let i = 0; i < rgba.length; i += 4) colors.add(`${rgba[i]},${rgba[i+1]},${rgba[i+2]},${rgba[i+3]}`);
    check(colors.size > 1, `graphics fixture rendered a uniform frame (${[...colors].join(';')})`);
    snapshot('graphics', window.__graphics);
  });
  await test('literal-english-without-chinese-or-rich-text', async () => {
    const diagnostics = await p.applySource(sources.english);
    check(Array.isArray(diagnostics), 'applySource must resolve diagnostics');
    const frame = await pixels(p);
    check(frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128), 'English text rendered no visible glyph pixels');
    snapshot('english-literal', frame);
  });
  await test('english-semibold-selects-variable-font-and-renders-distinctly', async () => {
    await p.applySource(sources.semiboldRegular);
    const regularFrame = await pixels(p);
    await p.applySource(sources.semibold);
    const semiboldFrame = await pixels(p);
    check(semiboldFrame.bytes.some((value, index) => index % 4 === 0 && value > 128 && semiboldFrame.bytes[index + 1] > 128 && semiboldFrame.bytes[index + 2] > 128), 'semibold English text rendered no visible glyph pixels');
    check(semiboldFrame.hash !== regularFrame.hash, 'semibold Text rendered identically to regular weight');
    snapshot('english-semibold', semiboldFrame);
  });
  await test('chinese-counter-prefix-suffix-without-rich-text', async () => {
    await p.applySource(sources.chineseCounter);
    const frame = await pixels(p);
    check(frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128), 'Chinese Counter prefix/suffix rendered no visible glyph pixels');
    snapshot('chinese-counter-prefix-suffix', frame);
  });
  await test('ordinary-chinese-without-rich-text', async () => {
    await p.applySource(sources.chinese);
    const frame = await pixels(p);
    check(frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128), 'Chinese text rendered no visible glyph pixels');
    check(frame.hash !== frames['english-literal'].hash, 'Chinese and English fixtures rendered the same frame');
    snapshot('chinese', frame);
  });
  await test('external-chinese-captions-and-resource-rollback', async () => {
    await p.applySource(sources.transcript);
    p.seek(0.5);
    const validFrame = await pixels(p);
    check(validFrame.bytes.some((value, index) => index % 4 === 0 && value > 128 && validFrame.bytes[index + 1] > 128 && validFrame.bytes[index + 2] > 128), 'external Chinese transcript rendered no visible caption glyphs');
    check(p._loadedBundle.analysis.fonts.text.includes('你好，世界字幕'), 'prepared font analysis omitted the external transcript text');
    const fontHashes = await Promise.all(p._loadedBundle.fontBytes.map(async bytes => {
      const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
      return [...digest].map(byte => byte.toString(16).padStart(2, '0')).join('');
    }));
    check(fontHashes.includes(sources.cjkFontSha256), 'prepared font closure omitted the Noto Sans SC raw face');
    const installed = p._loadedSource; const player = p._player;
    snapshot('external-chinese-captions', validFrame);
    for (const [name, source] of [['missing', sources.missingTranscript], ['malformed', sources.malformedTranscript]]) {
      let failure;
      try { await p.applySource(source); } catch (error) { failure = error; }
      check(failure?.kind === 'resource', `${name} transcript did not fail as a resource error`);
      check(p._loadedSource === installed && p._player === player, `${name} transcript replaced the committed document`);
      check((await pixels(p)).hash === validFrame.hash, `${name} transcript changed the committed caption frame`);
    }
  });
  await test('unicode-math-text-loads-fira-fallback-without-rich-text', async () => {
    await p.applySource(sources.unicodeMath);
    const frame = await pixels(p);
    check(frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128), 'Unicode math Text rendered no visible glyph pixels');
    check(p._loadedBundle.analysis.fonts.text.includes('𝑥 + 1'), 'font analysis omitted the Unicode math literal');
    const fontHashes = await Promise.all(p._loadedBundle.fontBytes.map(async bytes => {
      const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
      return [...digest].map(byte => byte.toString(16).padStart(2, '0')).join('');
    }));
    check(fontHashes.includes(sources.mathFontSha256), 'Unicode math fallback did not include the raw Fira Math face');
    snapshot('unicode-math-text', frame);
  });
  await test('failed-source-and-asset-retain-last-successful-frame', async () => {
    const before = await pixels(p); const installed = p._loadedSource; const player = p._player;
    let failure;
    try { await p.applySource('config { definitely broken syntax !!!'); } catch (error) { failure = error; }
    check(failure && Array.isArray(failure.diagnostics) && failure.diagnostics.length, 'syntax error lacks diagnostics');
    check(p._state === 'ready' && p._player === player && p._loadedSource === installed, 'failed source replaced installed document');
    check((await pixels(p)).hash === before.hash, 'failed source changed the rendered frame');
    failure = null;
    try { await p.applySource(sources.missingAsset); } catch (error) { failure = error; }
    check(failure && /404|asset/.test(failure.message), 'missing asset did not reject with a resource diagnostic');
    check(p._loadedSource === installed && (await pixels(p)).hash === before.hash, 'failed resource preparation replaced the frame');
    snapshot('retained-after-errors', await pixels(p));
  });
  await test('new-transitive-imports-and-transaction-install', async () => {
    await p.applySource(sources.imported);
    check(p._loadedBundle.modules.size === 2, 'new transitive imports did not close');
    check(p._loadedSource === sources.imported && p.time === p.duration, 'paused source install position is incorrect');
  });
  await test('batched-vars-merge-and-preserve-normalized-position', async () => {
    await p.applySource(sources.graphics); p.seek(p.duration * 0.35);
    const before = await pixels(p); let builds = 0; let settled = 0;
    const original = p._buildPrepared;
    p._buildPrepared = function (...args) { builds += 1; return original.apply(this, args); };
    try {
      const a = p.setVars({ side: 70 }).then(value => { settled += 1; return value; });
      const b = p.setVars({ tall: 40 }).then(value => { settled += 1; return value; });
      check(settled === 0, 'setVars settled before its transaction');
      const values = await Promise.all([a, b]);
      check(values.every(Array.isArray) && settled === 2 && builds === 1, 'variable batch did not share one final build/result');
      check(p.vars.side === 70 && p.vars.tall === 40, 'setVars lost a merged variable');
      check(Math.abs(p.time / p.duration - 0.35) < 0.001, 'variable rebuild lost normalized position');
      check((await pixels(p)).hash !== before.hash, 'variable batch did not change the rendered result');
      p.setAttribute('data-vars', JSON.stringify({ side: 55 }));
      await until(() => builds === 2 && !p._varsTimer && !p._operationController, 'data-vars replacement did not finish');
      check(p.vars.side === 55 && !('tall' in p.vars), 'data-vars must replace the merged set');
    } finally { p._buildPrepared = original; }
  });
  await test('failed-src-reload-settles-pending-variable-callers', async () => {
    const src = p.getAttribute('src');
    for (const invalid of ['./missing-scene.amx', null]) {
      const before = await pixels(p);
      const installed = p._loadedSource;
      const player = p._player;
      const committed = JSON.stringify(p.vars);
      const failedEvent = new Promise(resolve => p.addEventListener('amxerror', resolve, { once: true }));
      const pending = p.setVars({ side: 33 }).then(() => null, error => error);
      if (invalid === null) p.removeAttribute('src'); else p.setAttribute('src', invalid);
      const [failure, event] = await within(Promise.all([pending, failedEvent]), 'src failure left setVars pending');
      check(failure?.kind === 'resource' && event.detail?.kind === 'resource', 'src failure did not settle variable callers with the resource error');
      check(p._player === player && p._loadedSource === installed, 'failed src reload replaced the installed document');
      check(JSON.stringify(p.vars) === committed, 'failed src reload retained uncommitted variables');
      check((await pixels(p)).hash === before.hash, 'failed src reload changed the last successful frame');
      const recovered = new Promise(resolve => p.addEventListener('amxready', resolve, { once: true }));
      p.setAttribute('src', src);
      await within(recovered, 'valid src did not recover after a failed reload');
    }
  });
  await test('quality-reuses-prepared-program', async () => {
    const prepared = p._loadedBundle.prepared; let preparations = 0; let builds = 0;
    const originalPrepare = p._prepareDocument; const originalBuild = p._buildPrepared;
    p._prepareDocument = function (...args) { preparations += 1; return originalPrepare.apply(this, args); };
    p._buildPrepared = function (...args) { builds += 1; return originalBuild.apply(this, args); };
    try {
      p.setAttribute('quality', 'preview');
      await until(() => builds === 1 && !p._operationController, 'quality rebuild did not finish');
      check(preparations === 0 && p._loadedBundle.prepared === prepared, 'quality rebuilt the language analysis');
    } finally { p._prepareDocument = originalPrepare; p._buildPrepared = originalBuild; }
  });
  await test('superseded-import-cannot-install-or-signal-ready', async () => {
    const ready = p._acceptanceReady;
    const first = p.applySource(sources.slow).then(() => 'installed', error => error.name);
    await sleep(40);
    await p.applySource(sources.latest);
    check(await first === 'AbortError', 'superseded operation did not reject as cancelled');
    await sleep(550);
    check(p._loadedSource === sources.latest && p._acceptanceReady <= ready + 1, 'stale operation installed or sent a ready event');
  });
  await test('detach-and-remount-preserve-player-and-frame', async () => {
    const player = p._player; const before = await pixels(p); const ready = p._acceptanceReady;
    p.remove(); document.body.append(p); await sleep(50);
    check(p._player === player && !p._destroyed && p._acceptanceReady === ready, 'DOM reparenting recreated or destroyed the player');
    check((await pixels(p)).hash === before.hash, 'DOM reparenting lost the frame');
  });
  await test('media-actor-kind-and-relative-module-resource-isolation', async () => {
    await p.applySource(sources.relativeAssets);
    const assets = p._loadedBundle.analysis.assets;
    check(assets.length === 2 && new Set(assets.map(asset => asset.id)).size === 2, 'same-name module assets collided');
    check(assets.every(asset => /\/left\/icon.data$|\/right\/icon.data$/.test(asset.url)), 'assets lost their declaring module base');
    const frame = await pixels(p);
    const sample = (x, y) => frame.bytes.slice((y * frame.width + x) * 4, (y * frame.width + x) * 4 + 3);
    const left = sample(32, 36); const right = sample(96, 36);
    check(left[0] > left[2] + 100 && right[2] > right[0] + 100, `relative SVG assets rendered incorrectly: ${left}, ${right}`);
    snapshot('relative-assets', frame);
  });
  await test('raster-image-decode-and-invalid-payload-retention', async () => {
    await p.applySource(sources.image);
    const before = await pixels(p);
    const colors = before.bytes;
    check(colors.some((value, index) => index % 4 === 0 && value > 240 && colors[index + 1] < 10 && colors[index + 2] < 10), 'raster fixture has no red quadrant');
    check(colors.some((value, index) => index % 4 === 2 && value > 240 && colors[index - 1] < 10 && colors[index - 2] < 10), 'raster fixture has no blue quadrant');
    snapshot('raster-image', before);
    const failure = await p.applySource(sources.brokenImage).then(() => null, error => error);
    check(failure?.kind === 'build', 'invalid image payload did not fail during prepared build');
    check(p._loadedSource === sources.image && (await pixels(p)).hash === before.hash, 'invalid image payload replaced the last successful image');
  });
  await test('concurrent-same-family-overlay-fonts-stay-isolated', async () => {
    check(sources.overlayAvailable, 'Open Sans regular overlay fixture is available');
    const first = await create({ 'data-fonts': './overlay-a.ttf' });
    const second = await create({ 'data-fonts': './overlay-b.ttf' });
    const [a, b] = await Promise.all([first.ready, second.ready]);
    const textSource = sources.overlayText;
    await Promise.all([a.applySource(textSource), b.applySource(textSource)]);
    const firstFrame = await pixels(a); const secondFrame = await pixels(b);
    check(firstFrame.hash !== secondFrame.hash, 'different same-family font metrics rendered identical text pixels');
    await a.applySource(textSource);
    const firstAgain = await pixels(a);
    check(firstAgain.hash === firstFrame.hash, 'loading the second font changed the first player\'s cached text result');
    snapshot('overlay-font-a', firstAgain); snapshot('overlay-font-b', secondFrame);
    a.destroy(); b.destroy(); a.remove(); b.remove();
  });
  await test('rich-text-loads-on-demand', async () => {
    await p.applySource(sources.rich);
    check(!p._loadedBundle.analysis.diagnostics.some(diagnostic => diagnostic.code === 'unknown-property'), 'rich-text fixture used an unsupported property');
    const frame = await pixels(p);
    check(frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128), 'rich text rendered no visible glyph pixels');
    snapshot('rich-text', frame);
  });
  await test('typst-generated-chinese-selects-dynamic-cjk-fallback', async () => {
    await p.applySource(sources.generatedChineseTypst);
    const analysis = p._loadedBundle.analysis;
    check(analysis.fonts.dynamic === true, 'Typst generated text did not request dynamic font coverage');
    check(!analysis.fonts.text.includes('中'), 'generated Chinese unexpectedly appeared in raw source font text');
    const fontHashes = await Promise.all(p._loadedBundle.fontBytes.map(async bytes => {
      const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
      return [...digest].map(byte => byte.toString(16).padStart(2, '0')).join('');
    }));
    check(fontHashes.includes(sources.cjkFontSha256), 'dynamic Typst font closure omitted the raw Noto Sans SC face');
    const frame = await pixels(p);
    check(frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128), 'Typst-generated Chinese rendered no visible glyph pixels');
    snapshot('typst-generated-chinese', frame);
    const installed = p._loadedSource; const player = p._player;
    const previewResourceCount = () => performance.getEntriesByType('resource').filter(entry => {
      const url = new URL(entry.name, location.href);
      return url.origin === location.origin && url.pathname.startsWith('/preview/');
    }).length;
    const resourceCount = previewResourceCount();
    const unsupported = await p.applySource(sources.unsupportedTypst).then(() => null, error => error);
    const unsupportedDiagnostics = (unsupported?.diagnostics ?? []).map(diagnostic => diagnostic.message).join('\n');
    check(unsupported?.kind === 'build' && /U\+10ffff/i.test(`${unsupported.message}\n${unsupportedDiagnostics}`) && /coverage|unsupported|missing glyph/i.test(`${unsupported.message}\n${unsupportedDiagnostics}`), 'unsupported generated U+10FFFF did not fail with a font coverage diagnostic');
    check(p._loadedSource === installed && p._player === player, 'unsupported generated glyph replaced the installed document');
    check((await pixels(p)).hash === frame.hash, 'unsupported generated glyph changed the installed frame');
    check(previewResourceCount() === resourceCount, 'unsupported generated glyph triggered a network request during rendering');
  });
  await test('overlay-failure-retains-frame-and-retry-recovers', async () => {
    await p.applySource(sources.english);
    const before = await pixels(p); const installed = p._loadedSource; const player = p._player;
    const state = () => ({ time: p.time, playing: p._playing, renderScale: p._renderScale,
      revision: p._renderRevision, canvas: [p._canvas.width, p._canvas.height],
      vars: p.vars, state: p._state, generation: p._operationGeneration });
    const beforeState = state();
    const failed = new Promise(resolve => p.addEventListener('amxerror', resolve, { once: true }));
    p.setAttribute('data-fonts', './retryfont.ttf');
    const failure = await within(failed, 'failed overlay request did not report an error');
    await until(() => !p._operationController, 'failed overlay transaction did not settle');
    check(failure.detail?.kind === 'resource', 'failed overlay did not report a resource error');
    check(p._state === 'ready' && p._player === player && p._loadedSource === installed, 'failed overlay replaced the installed player or source');
    const after = await pixels(p);
    const hasGlyphs = frame => frame.bytes.some((value, index) => index % 4 === 0 && value > 128 && frame.bytes[index + 1] > 128 && frame.bytes[index + 2] > 128);
    // A no-operation control reproduces one RGB edge pixel changing 82 to 83.
    // Bound that raster rounding explicitly; alpha and every other pixel stay exact.
    const sameFrame = compareFrames(before, after, 1, 1);
    if (!sameFrame || !hasGlyphs(before) || !hasGlyphs(after)) {
      snapshot('failed-overlay-before', before); snapshot('failed-overlay-after', after);
      window.__failureState = { before: beforeState, after: state(),
        frames: [before, after].map(({ width, height, hash }) => ({ width, height, hash })) };
      try {
        const repeated = await pixels(p); snapshot('failed-overlay-repeated', repeated);
        window.__failureState.frames.push({ width: repeated.width, height: repeated.height, hash: repeated.hash });
      } catch (error) { window.__failureState.repeatError = error.message; }
      window.__failureFrames = Object.fromEntries(Object.entries(frames).filter(([name]) => name.startsWith('failed-overlay-')));
    }
    check(hasGlyphs(before), 'overlay baseline rendered no visible glyph pixels');
    check(hasGlyphs(after), 'failed overlay lost visible glyph pixels');
    check(sameFrame, 'failed overlay changed the rendered frame beyond one RGB rounding pixel');
    const recovered = new Promise((resolve, reject) => {
      p.addEventListener('amxready', () => resolve(p), { once: true });
      p.addEventListener('amxerror', event => reject(new Error(event.detail?.message || 'overlay retry failed')), { once: true });
    });
    p.setAttribute('data-fonts', './retryfont.ttf?retry=1');
    await within(recovered, 'overlay retry did not report ready');
    await until(() => !p._operationController, 'overlay retry transaction did not settle');
    check(p._state === 'ready' && p._player === player && p._loadedSource === installed, 'successful overlay retry replaced the scene identity');
    snapshot('overlay-recovered', await pixels(p));
  });
  await test('http-200-invalid-overlay-retains-frame-and-retries-same-url', async () => {
    check(sources.overlayAvailable, 'Open Sans overlay fixture is available');
    await p.applySource(sources.overlayText);
    const before = await pixels(p); const installed = p._loadedSource; const player = p._player;
    const failed = new Promise(resolve => p.addEventListener('amxerror', resolve, { once: true }));
    p.setAttribute('data-fonts', './invalidfont.ttf');
    const failure = await within(failed, 'invalid HTTP 200 overlay font did not report an error');
    await until(() => !p._operationController, 'invalid overlay transaction did not settle');
    check(failure.detail?.kind === 'resource', 'invalid HTTP 200 overlay did not report a resource error');
    check(p._state === 'ready' && p._player === player && p._loadedSource === installed, 'invalid overlay replaced the installed player or source');
    check((await pixels(p)).hash === before.hash, 'invalid overlay changed the rendered frame');

    await fetch('/__repair-invalid-overlay');
    await p.applySource(sources.overlayText);
    await until(() => !p._operationController, 'repaired same-URL overlay transaction did not settle');
    check(p._state === 'ready' && p._player === player && p._loadedSource === sources.overlayText, 'repaired overlay did not install successfully');
    // The server-side request log is asserted in verifyLoading() after this
    // browser function returns; it is not in this serialized page scope.
    snapshot('invalid-overlay-recovered', await pixels(p));
  });
  await test('shared-overlay-download-survives-one-consumer-destroy', async () => {
    const first = (await create()).player;
    const second = (await create()).player;
    await until(() => first._state === 'ready' && second._state === 'ready', 'players did not become ready');
    const survivorReady = new Promise((resolve, reject) => {
      second.addEventListener('amxready', () => resolve(second), { once: true });
      second.addEventListener('amxerror', event => reject(new Error(String(event.detail?.error))), { once: true });
    });
    first.setAttribute('data-fonts', './slowfont.ttf');
    second.setAttribute('data-fonts', './slowfont.ttf');
    await until(() => first._operationController && second._operationController, 'players did not begin loading');
    await sleep(60); first.destroy();
    const survivor = await within(survivorReady, 'surviving player did not reload');
    check(first._player === null && first._loadedBundle === null, 'destroy retained a live document');
    check(survivor._runtime === p._runtime, 'mixed players created different base runtimes');
    check(window.__deviceRequests === 1, `page created ${window.__deviceRequests} WebGPU devices`);
    survivor.destroy(); first.remove(); survivor.remove();
  });
  await test('destroy-is-idempotent-and-rejects-pending-vars', async () => {
    const pending = p.setVars({ side: 20 }).then(() => 'resolved', error => error.name);
    p.destroy(); p.destroy();
    check(await pending === 'AbortError', 'destroy did not cancel pending variable callers');
    check(p._player === null && p._loadedBundle === null, 'destroy retained player or preparation');
    check(await p.setVars({ side: 30 }).then(() => false, error => error.name === 'AbortError'), 'destroyed player accepted a new variable update');
    p.remove(); document.body.append(p); await sleep(30);
    check(p._player === null && p._destroyed, 'remount revived a destroyed player');
    p.remove();
  });
  await test('repeated-create-destroy-keeps-shared-runtime', async () => {
    const runtime = window[Symbol.for('animatix.sdk.runtime')].promise;
    for (let index = 0; index < 3; index += 1) {
      const created = await create(); const player = await created.ready;
      player.destroy(); check(player._player === null && player._loadedBundle === null, 'cycle retained active resources'); player.remove();
    }
    check(window[Symbol.for('animatix.sdk.runtime')].promise === runtime && window.__deviceRequests === 1, 'cycles duplicated base runtime/device');
    check(document.querySelectorAll('amx-player').length === 0, 'destroy cycles retained elements');
  });
  await test('resource-inventory-converges-after-rebuild-and-destroy', async () => {
    const runtime = await window[Symbol.for('animatix.sdk.runtime')].promise;
    check(typeof runtime.module.debug_resource_stats === 'function' && typeof sdk.debugRuntimeStats === 'function', 'resource inventory API is missing');
    resourceInventory = await exerciseInventory({
      createReadyPlayer: async () => { const created = await create(); return created.ready; },
      source: sources.english,
      readWasmStats: () => runtime.module.debug_resource_stats(),
      readRuntimeStats: () => sdk.debugRuntimeStats(),
      overlayFontUrls: Array.from({ length: 65 }, (_, index) => new URL(`./overlay-a.ttf?inventory=${index}`, document.baseURI).href),
      fetchOverlayFont: async (url, player) => {
        await within(new Promise((resolve, reject) => {
          const cleanup = () => { player.removeEventListener('amxready', ready); player.removeEventListener('amxerror', failed); };
          const ready = () => { cleanup(); resolve(); };
          const failed = event => { cleanup(); reject(new Error(event.detail?.message || 'overlay inventory failed')); };
          player.addEventListener('amxready', ready); player.addEventListener('amxerror', failed);
          player.setAttribute('data-fonts', url);
        }), 'overlay inventory transaction did not finish');
      },
    });
    check(window.__deviceRequests === 1, 'inventory cycles created another GPU device');
  });
  await test('browser-sri-rejects-corrupt-runtime-entry-before-wasm', async () => {
    const isolated = await isolatedPlayer('sri-entry');
    const failure = await within(isolated.error, 'corrupt SRI entry did not emit amxerror');
    await sleep(50);
    check(failure.detail?.kind === 'resource', `corrupt entry error kind was ${failure.detail?.kind}`);
    check(/integrity|SRI/i.test(failure.detail?.message || ''), 'browser SRI failure was not reported explicitly');
    check(isolated.readyCount() === 0, 'corrupt entry emitted amxready');
    check(!isolated.player._player, 'corrupt entry created a GPU player');
    isolated.frame.remove();
  });
  await test('browser-rejects-manifest-schema-and-runtime-build-mismatches', async () => {
    for (const scenario of ['schema-mismatch', 'build-mismatch']) {
      const isolated = await isolatedPlayer(scenario);
      const failure = await within(isolated.error, `${scenario} did not emit amxerror`);
      await sleep(50);
      check(failure.detail?.kind === 'version', `${scenario} error kind was ${failure.detail?.kind}`);
      check(scenario === 'schema-mismatch'
        ? /schema/i.test(failure.detail?.message || '')
        : /runtime build mismatch/i.test(failure.detail?.message || ''), `${scenario} did not report its explicit mismatch`);
      check(isolated.readyCount() === 0, `${scenario} emitted amxready`);
      check(!isolated.player._player, `${scenario} created a GPU player`);
      isolated.frame.remove();
    }
  });
  await test('browser-capability-integrity-failure-retains-and-recovers', async () => {
    const isolated = await isolatedPlayer('capability-retry');
    const player = await within(isolated.ready, 'isolated graphics player did not become ready');
    const before = await pixels(player); const installed = player._loadedSource; const engine = player._player;
    const mediaSource = sources.media;
    const first = await within(player.applySource(mediaSource).then(() => null, error => error), 'corrupt capability did not reject');
    check(first && first.kind === 'capability', `corrupt capability error kind was ${first?.kind}`);
    check(/SHA-256|size mismatch/i.test(first.message || ''), 'corrupt capability integrity failure was not explicit');
    check(player._player === engine && player._loadedSource === installed, 'corrupt capability replaced the installed graphic document');
    check((await pixels(player)).hash === before.hash, 'corrupt capability changed the prior graphic frame');
    await player.applySource(mediaSource);
    check(player._loadedSource === mediaSource && player._player === engine, 'valid capability retry did not install the new scene on the same player');
    check((await pixels(player)).hash !== before.hash, 'valid capability retry did not render the media scene');
    isolated.frame.remove();
  });
  let fixed = false;
  try { sdk.configureRuntime({ manifestUrl: '/different/runtime-manifest.json' }); } catch { fixed = true; }
  check(fixed, 'runtime location changed after first initialization');
  return { passed: results, frames, resource_inventory: resourceInventory, device_requests: window.__deviceRequests, adapter: adapterInfo };
}

function hasRequest(files, selected = requests) {
  return selected.some(request => files.some(file => request.path === `/preview/runtime/${file}` || request.path === `/preview/runtime/${manifest.files[file]?.gzip_url}`));
}
function verifyLoading() {
  const sriPhase = requests.filter(request => request.phase === 'browser-sri-rejects-corrupt-runtime-entry-before-wasm');
  assert(sriPhase.some(request => request.path.endsWith(`/${manifest.entry}`)), 'browser did not request the SRI-protected entry');
  assert(!sriPhase.some(request => /\.wasm(?:\.gz)?$/.test(request.path)), 'browser fetched Wasm after SRI rejected the entry');
  const capabilityPhase = requests.filter(request => request.phase === 'browser-capability-integrity-failure-retains-and-recovers');
  const corrupted = capabilityPhase.filter(request => request.corrupted);
  assert.equal(corrupted.length, 1, 'capability integrity fixture did not corrupt exactly one response');
  assert.equal(capabilityPhase.filter(request => request.path === corrupted[0].path).length, 2, 'capability retry did not refetch the corrupt gzip payload');
  for (const file of manifest.capabilities.media.files) {
    const path = `/preview/scenario/capability-retry/runtime/${manifest.files[file].gzip_url}`;
    assert.equal(capabilityPhase.filter(request => request.path === path).length,
      path === corrupted[0].path ? 2 : 1, `capability retry failed to reuse the verified payload ${file}`);
  }
  const graphicsRequests = requests.filter(request => request.phase === 'graphics-no-fonts-or-capabilities');
  assert(!graphicsRequests.some(request => request.path.includes('/runtime/fonts/')), 'graphics fetched a font');
  const capabilityFiles = Object.values(manifest.capabilities).flatMap(capability => capability.files);
  assert(!hasRequest(capabilityFiles, graphicsRequests), 'graphics fetched a capability chunk');
  const englishRequests = requests.filter(request => request.phase === 'literal-english-without-chinese-or-rich-text');
  const chineseFonts = manifest.fonts.faces.filter(face => /NotoSansSC|Noto Sans SC/i.test(`${face.file} ${face.family}`)).map(face => face.url);
  assert(!hasRequest(chineseFonts, englishRequests), 'English fetched Chinese fonts');
  const semiboldRequests = requests.filter(request => request.phase === 'english-semibold-selects-variable-font-and-renders-distinctly');
  const variableFont = manifest.fonts.faces.find(face => face.family === 'Open Sans' && face.axes?.some(axis => axis.tag === 'wght'));
  assert(variableFont, 'Open Sans variable face is missing from the SDK manifest');
  assert(hasRequest([variableFont.url], semiboldRequests), 'semibold English text did not fetch the Open Sans variable face');
  assert(!hasRequest(chineseFonts, semiboldRequests), 'semibold English text fetched a CJK face');
  const richFiles = manifest.capabilities.rich_text.files;
  assert(!hasRequest(richFiles, semiboldRequests), 'semibold English text fetched the rich-text capability');
  const chineseRequests = requests.filter(request => request.phase === 'ordinary-chinese-without-rich-text');
  assert(!hasRequest(richFiles, chineseRequests), 'ordinary Chinese text fetched the rich-text capability');
  const transcriptRequests = requests.filter(request => request.phase === 'external-chinese-captions-and-resource-rollback');
  assert(!hasRequest(manifest.capabilities.rich_text.files, transcriptRequests), 'plain Captions transcript fetched the rich-text capability');
  for (const [file, status] of [['valid.vtt', 200], ['missing.vtt', 404], ['malformed.vtt', 200]]) {
    const request = transcriptRequests.find(item => item.path === `/preview/__acceptance__/subtitles/${file}`);
    assert.equal(request?.status, status, `external transcript ${file} was not fetched from its declaring module directory`);
  }
  const unicodeMathRequests = requests.filter(request => request.phase === 'unicode-math-text-loads-fira-fallback-without-rich-text');
  const mathFace = manifest.fonts.faces.find(face => /FiraMath-Regular/i.test(face.file || ''));
  assert(mathFace, 'Fira Math regular face is missing from the SDK manifest');
  assert(hasRequest([mathFace.url], unicodeMathRequests), 'Unicode math Text did not fetch the Fira Math fallback face');
  assert(!hasRequest(manifest.capabilities.rich_text.files, unicodeMathRequests), 'Unicode math Text fetched a rich-text capability');
  const richStart = requests.findIndex(request => request.phase === 'rich-text-loads-on-demand');
  const preRich = requests.slice(0, richStart < 0 ? requests.length : richStart);
  const media = new Set(manifest.capabilities.media.files);
  const richOnly = manifest.capabilities.rich_text.files.filter(file => !media.has(file));
  const chineseCounterRequests = requests.filter(request => request.phase === 'chinese-counter-prefix-suffix-without-rich-text');
  assert(hasRequest(chineseFonts, chineseCounterRequests), 'Chinese Counter prefix/suffix did not fetch a CJK font face');
  assert(!hasRequest(manifest.capabilities.rich_text.files, chineseCounterRequests), 'Chinese Counter prefix/suffix fetched the rich-text capability');
  assert(!hasRequest(richOnly, preRich), 'ordinary text fetched a rich-text chunk');
  assert(hasRequest(manifest.capabilities.rich_text.files, requests.filter(request => request.phase === 'rich-text-loads-on-demand')), 'rich text never fetched its capability');
  const mediaRequests = requests.filter(request => manifest.capabilities.media.files.some(file => request.path === `/preview/runtime/${file}` || request.path === `/preview/runtime/${manifest.files[file]?.gzip_url}`));
  for (const file of manifest.capabilities.media.files) {
    const fileRequests = mediaRequests.filter(request => request.path === `/preview/runtime/${file}` || request.path === `/preview/runtime/${manifest.files[file]?.gzip_url}`);
    assert.equal(fileRequests.length, 1, `media capability payload ${file} was not fetched exactly once`);
    assert(['failed-source-and-asset-retain-last-successful-frame', 'media-actor-kind-and-relative-module-resource-isolation'].includes(fileRequests[0].phase), `media capability payload ${file} loaded before a media-requiring source`);
  }
  const used = new Set([...manifest.capabilities.rich_text.files, ...manifest.capabilities.media.files]);
  assert(!hasRequest(manifest.capabilities.raster_encode.files.filter(file => !used.has(file))), 'unused raster encoder was fetched');
  const baseFiles = [manifest.wasm, manifest.files[manifest.wasm].gzip_url];
  assert.equal(requests.filter(request => baseFiles.some(file => request.path === `/preview/runtime/${file}`)).length, 1, 'page fetched more than one base engine');
  assert.equal(requests.filter(request => request.path.endsWith('/slowfont.ttf')).length, 1, 'overlay download was not shared');
  const retryRequests = requests.filter(request => request.path.endsWith('/retryfont.ttf'));
  assert.deepEqual(retryRequests.map(request => request.status), [503, 200], 'overlay failure/retry did not make one failed then one successful request');
  const invalidFontRequests = requests.filter(request => request.path.endsWith('/invalidfont.ttf'));
  assert.deepEqual(invalidFontRequests.map(request => request.status), [200, 200], 'invalid font retry did not refetch the repaired same URL');
}

async function main() {
  mkdirSync(EVIDENCE, { recursive: true });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const profile = mkdtempSync(join(tmpdir(), 'amx-transactions-'));
  const chrome = spawn(process.env.CHROME || 'chromium', [
    ...(hardwareGpu ? [] : ['--headless=new']), '--no-sandbox', '--disable-dev-shm-usage', '--disable-gpu-sandbox', '--enable-unsafe-webgpu',
    ...(hardwareGpu ? ['--use-vulkan=native', '--enable-features=Vulkan,WebGPUService', '--ignore-gpu-blocklist'] : ['--enable-unsafe-swiftshader', '--disable-vulkan-surface', '--use-angle=vulkan', '--use-vulkan=swiftshader', '--enable-features=Vulkan']),
    '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-extensions',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
  ], { detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let chromeLog = ''; const consoleEvents = []; let cdp; let report;
  chrome.on('error', error => { chromeLog += `${error.stack}\n`; });
  chrome.stdout.on('data', chunk => { chromeLog += chunk; }); chrome.stderr.on('data', chunk => { chromeLog += chunk; });
  try {
    const port = await waitFor('Chromium DevTools', () => existsSync(join(profile, 'DevToolsActivePort')) && Number(readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]));
    const response = await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent('about:blank')}`, { method: 'PUT' });
    const target = await response.json(); cdp = new Cdp(target.webSocketDebuggerUrl); await cdp.open();
    cdp.onEvent = event => { if (event.method.startsWith('Runtime.')) consoleEvents.push(event); };
    await cdp.send('Page.enable'); await cdp.send('Runtime.enable');
    await cdp.send('Page.navigate', { url: `${origin}/preview/__acceptance__/index.html` });
    await waitFor('fixture page', async () => await cdp.evaluate('document.readyState') === 'complete');
    const sources = {
      timeout, graphics, overlayAvailable: Boolean(overlay && overlayWide), english: text('*literal* #markup'), chinese: text('中文换行与文本'),
      cjkFontSha256: manifest.fonts.faces.find(face => /NotoSansSC|Noto Sans SC/i.test(`${face.file} ${face.family}`))?.sha256,
      chineseCounter: `${config}label: Counter, value: 123, prefix: "约", suffix: " 位用户", font_size: 14, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      transcript: `${config}import "./subtitles/caption.amx"\n#0s\ncaption: ChineseCaptions, at: (0, 0), opacity: 1\n`,
      missingTranscript: `${config}import "./subtitles/missing-caption.amx"\n#0s\ncaption: MissingCaptions, at: (0, 0), opacity: 1\n`,
      malformedTranscript: `${config}import "./subtitles/malformed-caption.amx"\n#0s\ncaption: MalformedCaptions, at: (0, 0), opacity: 1\n`,
      unicodeMath: `${config}label: Text, text: "𝑥 + 1", font_size: 22, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      mathFontSha256: manifest.fonts.faces.find(face => /FiraMath-Regular/i.test(face.file || ''))?.sha256,
      generatedChineseTypst: `${config}label: Typst, text: "#str.from-unicode(20013)", font_size: 18, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      unsupportedTypst: `${config}label: Typst, text: "#str.from-unicode(1114111)", font_size: 18, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      semiboldRegular: `${config}label: Text, text: "Variable weight sample", font_family: "Open Sans", font_size: 32, font_weight: 400, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      semibold: `${config}label: Text, text: "Variable weight sample", font_family: "Open Sans", font_size: 32, font_weight: "semibold", color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      imported: `${config}import "./palette.amx" as palette\nbox: Rect, size: (60, 40), at: (64, 36), color: palette.tint, opacity: 1\n`,
      slow: `${config}import "./slow.amx" as slow\nbox: Rect, size: (60, 40), at: (64, 36), color: slow.tint, opacity: 1\n`,
      latest: solid('(0.1, 0.8, 0.2, 1)'),
      missingAsset: `${config}pic: Svg, url: "missing.data", size: (40, 40), at: (64, 36), opacity: 1\n`,
      relativeAssets: `${config}import "./left/module.amx"\nimport "./right/module.amx"\nleft: Left, at: (32, 36), opacity: 1\nright: Right, at: (96, 36), opacity: 1\n`,
      rich: `${config}label: Typst, text: "Hello *rich*", font_size: 18, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
      media: `${config}pic: Svg, url: "shared.data", size: (40, 40), at: (64, 36), opacity: 1\n`,
      image: `${config}pic: Image, url: "raster.data", size: (40, 40), at: (64, 36), opacity: 1\n`,
      brokenImage: `${config}pic: Image, url: "broken-image.data", size: (40, 40), at: (64, 36), opacity: 1\n`,
      overlayText: `${config}label: Text, text: "MMMMWWiiii", font_family: "Open Sans", font_size: 40, font_weight: 400, color: (1, 1, 1, 1), at: (64, 36), opacity: 1\n`,
    };
    let suiteTimer;
    try {
      report = await Promise.race([cdp.evaluate(`(${browserTests.toString()})(${JSON.stringify(sources)}, (${exerciseResourceInventory.toString()}), (${compareRgbaFrames.toString()}))`, timeout * 2), new Promise((_, reject) => { suiteTimer = setTimeout(() => reject(new Error('browser transaction suite timed out')), timeout * 2); })]);
    } finally { clearTimeout(suiteTimer); }
    if (hardwareGpu) assert(!/swiftshader|llvmpipe|software/i.test(JSON.stringify(report.adapter)), `hardware GPU required: ${JSON.stringify(report.adapter)}`);
    verifyLoading();
    assert(!consoleEvents.some(event => event.method === 'Runtime.exceptionThrown'), 'browser reported an uncaught exception');
    assert(!/wgpu uncaptured error|wgpu device lost|Error while parsing WGSL/i.test(JSON.stringify(consoleEvents)), 'GPU validation failed');
    for (const [name, frame] of Object.entries(report.frames)) {
      writeFileSync(join(EVIDENCE, `${name}.png`), encodeFramePng(frame)); delete frame.rgba;
    }
    const pageScreenshot = { status: 'captured' };
    try {
      const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, 15_000);
      writeFileSync(join(EVIDENCE, 'browser.png'), Buffer.from(shot.data, 'base64'));
    } catch (error) {
      pageScreenshot.status = 'unavailable'; pageScreenshot.reason = error.message;
    }
    writeFileSync(join(EVIDENCE, 'report.json'), JSON.stringify({ ...report, page_screenshot: pageScreenshot, build_id: manifest.build_id, site: SITE, limitations: hardwareGpu ? ['Performance comparison is recorded separately.'] : ['Software GPU functional evidence only; hardware visual/performance acceptance remains separate.'] }, null, 2));
    process.stdout.write(`web-transactions: ${report.passed.length} checks passed; ${EVIDENCE}\n`);
  } catch (error) {
    const browserState = cdp
      ? await cdp.evaluate(`({ passed: window.__transactions || [], active: window.__activeTransaction || null, failure: window.__transactionFailure || null, frameState: window.__failureState || null, frames: window.__failureFrames || {} })`).catch(() => null)
      : null;
    for (const [name, frame] of Object.entries(browserState?.frames || {})) {
      writeFileSync(join(EVIDENCE, `${name}.png`), encodeFramePng(frame));
    }
    writeFileSync(join(EVIDENCE, 'report.json'), JSON.stringify({
      frame_state: browserState?.frameState,
      passed: report?.passed ?? browserState?.passed ?? [],
      active: browserState?.active ?? null,
      failure: browserState?.failure ?? { message: error.message, stack: error.stack },
      build_id: manifest.build_id, site: SITE,
    }, null, 2));
    throw error;
  } finally {
    writeFileSync(join(EVIDENCE, 'requests.json'), JSON.stringify(requests, null, 2));
    writeFileSync(join(EVIDENCE, 'console.json'), JSON.stringify(consoleEvents, null, 2));
    writeFileSync(join(EVIDENCE, 'chromium.log'), chromeLog);
    cdp?.socket.close();
    try { process.kill(-chrome.pid, 'SIGKILL'); } catch { chrome.kill('SIGKILL'); }
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    rmSync(profile, { recursive: true, force: true });
  }
}
await main();
