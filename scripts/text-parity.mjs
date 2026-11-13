#!/usr/bin/env node
import assert from 'node:assert/strict';
import { createHash, webcrypto } from 'node:crypto';
import { gunzipSync } from 'node:zlib';
import { spawnSync } from 'node:child_process';
import { readFile, mkdtemp, writeFile, rm } from 'node:fs/promises';
import { dirname, resolve, join } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { verify } from '../web/tools/sdk.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = process.argv[2]
  ? resolve(process.argv[2])
  : resolve(repo, 'web/runtime/runtime-manifest.json');
const manifestDir = dirname(manifestPath);
const manifest = verify(manifestDir, { release: true });
if (manifest.schema_version !== 1 || !manifest.build_id || !manifest.entry || !manifest.wasm) {
  throw new Error('runtime manifest is missing schema/build/entry/wasm identity');
}
const runtimeFont = async (family, file) => {
  const face = manifest.fonts?.faces?.find((candidate) => candidate.family === family && candidate.file === file);
  if (!face) throw new Error(`runtime font face not found: ${family} / ${file}`);
  const raw = gunzipSync(await readFile(resolve(manifestDir, face.url)));
  assert.equal(createHash('sha256').update(raw).digest('hex'), face.sha256, `${file} content hash`);
  assert.equal(raw.length, face.size_bytes, `${file} content length`);
  return raw;
};
const fonts = [
  await runtimeFont('Open Sans', 'OpenSans-Variable.ttf'),
  await runtimeFont('Noto Sans SC Thin', 'NotoSansSC-Common.ttf'),
  await readFile(resolve(repo, 'tools/animatix-web-build/fixtures/text-parity/fonts/NotoNaskhArabicUI.ttf')),
  await runtimeFont('Fira Math', 'FiraMath-Regular.otf'),
];
assert.equal(createHash('sha256').update(fonts[2]).digest('hex'), '4713198071a99afad0a596df8ab3c71b1b59da092ad4f3c27c439949a7118fc8', 'Arabic fixture font SHA-256');
const requestPath = resolve(repo, 'tools/animatix-web-build/fixtures/text-parity/request.json');
const request = JSON.parse(await readFile(requestPath, 'utf8'));
const scratch = await mkdtemp(join(process.env.TMPDIR ?? '/tmp', 'animatix-text-parity-'));
try {
  const requestCopy = join(scratch, 'request.json');
  const fontNames = ['OpenSans-Variable.ttf', 'NotoSansSC-Common.ttf', 'NotoNaskhArabicUI.ttf', 'FiraMath-Regular.otf'];
  const fontPaths = await Promise.all(fonts.map(async (bytes, index) => {
    const path = join(scratch, `${index}-${fontNames[index]}`);
    await writeFile(path, bytes);
    return path;
  }));
  await writeFile(requestCopy, JSON.stringify(request));

  const native = spawnSync('cargo', [
    'run', '--locked', '--quiet', '-p', 'animatix-text-parity', '--', requestCopy, ...fontPaths,
  ], { cwd: repo, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  if (native.status !== 0) throw new Error(`native text probe failed:\n${native.stderr}`);
  const nativeOutput = JSON.parse(native.stdout);

  globalThis.crypto ??= webcrypto;
  globalThis.fetch = async (input) => {
    const url = input instanceof Request ? input.url : String(input);
    const path = fileURLToPath(url);
    try {
      return new Response(await readFile(path), { status: 200 });
    } catch {
      return new Response('not found', { status: 404 });
    }
  };
  const entryPath = resolve(manifestDir, manifest.entry);
  await import(pathToFileURL(entryPath).href);
  const engine = globalThis[Symbol.for(`animatix.sdk.engine.${manifest.build_id}`)];
  if (!engine) throw new Error('SDK entry did not register its versioned engine symbol');
  const wasmModule = await WebAssembly.compile(await readFile(resolve(manifestDir, manifest.wasm)));
  engine.initSync({ module: wasmModule });
  // Grouped Equation snapshots require the rich-text split before invoking the probe.
  await engine.ensure_capabilities(1, manifest.files);
  if (typeof engine.debug_text_probe !== 'function') throw new Error('SDK is missing debug_text_probe');
  const wasmOutput = engine.debug_text_probe(request, fonts.map((bytes) => new Uint8Array(bytes)));
  compare(nativeOutput, wasmOutput, '$');
  console.log(JSON.stringify({
    status: 'passed', build_id: manifest.build_id,
    plain_cases: wasmOutput.cases.length, grouped_cases: wasmOutput.grouped.length,
    font_sha256: wasmOutput.font_sha256,
  }, null, 2));
} finally {
  await rm(scratch, { recursive: true, force: true });
}

function compare(expected, actual, path) {
  if (typeof expected === 'number' && typeof actual === 'number') {
    if (/\.(id|font_index|char_idx|word_idx|line_idx|start|end)$/.test(path)
      || /\.normalized_coords\[\d+\]$/.test(path)
      || /\.ranges\[\d+\]\[\d+\]$/.test(path)) {
      assert.equal(actual, expected, `${path} exact value`);
    } else {
      assert.ok(Math.abs(actual - expected) <= 1e-5, `${path}: ${actual} != ${expected}`);
    }
  } else if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${path} should be an array`);
    assert.equal(actual.length, expected.length, `${path} length`);
    expected.forEach((value, index) => compare(value, actual[index], `${path}[${index}]`));
  } else if (expected && typeof expected === 'object') {
    assert.ok(actual && typeof actual === 'object', `${path} should be an object`);
    assert.deepEqual(Object.keys(actual).sort(), Object.keys(expected).sort(), `${path} keys`);
    for (const key of Object.keys(expected)) compare(expected[key], actual[key], `${path}.${key}`);
  } else {
    assert.equal(actual, expected, path);
  }
}
