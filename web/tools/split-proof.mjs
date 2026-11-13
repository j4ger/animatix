import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

const directory = pathToFileURL(`${resolve(process.argv[2])}/`);
const files = Object.fromEntries(readdirSync(directory).filter(name => name.endsWith('.wasm')).map(name => {
  const bytes = readFileSync(new URL(name, directory));
  return [name, {sha256: createHash('sha256').update(bytes).digest('hex'), size_bytes: bytes.length}];
}));
const mainBytes = readFileSync(new URL('animatix_web_bg.wasm', directory));
const coldName = Object.keys(files).find(name => name.startsWith('capability_rich_text'));
const coldBytes = readFileSync(new URL(coldName, directory));
assert.ok(coldBytes.length > 4096, `cold workload partition is too small (${coldBytes.length} bytes)`);
// The first eight table entries are a stable, distinctive little-endian
// fingerprint. Their bytes must be owned by the capability module only.
const fingerprint = Buffer.alloc(32);
let state = 0x6d2b79f5;
for (let index = 0; index < 8; index++) {
  state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0;
  fingerprint.writeUInt32LE(state, index * 4);
}
assert.notEqual(coldBytes.indexOf(fingerprint), -1, 'workload data is absent from the cold module');
assert.equal(mainBytes.indexOf(fingerprint), -1, 'workload data leaked into the main module');
const fetches = [];
let failFirst = true;
globalThis.fetch = async url => {
  fetches.push(String(url));
  if (failFirst) { failFirst = false; return new Response('', { status: 503 }); }
  return new Response(readFileSync(url), { headers: { 'Content-Type': 'application/gzip' } });
};
const engine = await import(new URL('animatix_web.js', directory));
const raw = await engine.default({ module_or_path: readFileSync(new URL('animatix_web_bg.wasm', directory)) });
assert.equal(raw.main(), 42);
assert.equal(engine.format_probe(70), 'left:70:right', 'main formatting works before any capability loads');
assert.equal(engine.nested_format_probe(70), 'prefix value70', 'nested main formatting works before capabilities');
assert.throws(() => engine.call_cold(1));
await assert.rejects(engine.ensure_capabilities(1, files), /503/);
assert.throws(() => engine.call_cold(1));
const badFiles = structuredClone(files);
const capability = Object.keys(badFiles).find(name => name.startsWith('capability_rich_text'));
badFiles[capability].sha256 = '0'.repeat(64);
await assert.rejects(engine.ensure_capabilities(1, badFiles), /SHA-256 mismatch/);
assert.throws(() => engine.call_cold(1));
await Promise.all([engine.ensure_capabilities(1, files), engine.ensure_capabilities(1, files)]);
assert.equal(fetches.length, 3, 'HTTP failure and integrity failure are retryable; success is shared');
await assert.rejects(engine.ensure_capabilities(16, files), /unsupported/);
const memory = raw.memory;
const table = new Uint32Array(2048);
let tableState = 0x6d2b79f5;
for (let step = 0; step < table.length; step++) {
  tableState = (Math.imul(tableState, 1_664_525) + 1_013_904_223) >>> 0;
  table[step] = tableState;
}
for (let index = 0; index < 100; index++) {
  const output = engine.call_cold(index);
  assert.equal(output.length, table.length);
  for (let step = 0; step < table.length; step++) {
    const input = (index + step) >>> 0;
    const expected = (Math.imul((input << 5) | (input >>> 27), 0x9e3779b9) ^ table[step] ^ (`cold:${index}`.length)) >>> 0;
    assert.equal(output[step], expected, `shared-memory write mismatch at ${index}:${step}`);
  }
}
assert.equal(raw.memory, memory);
assert.equal(engine.format_probe(70), 'left:70:right', 'main formatting survives capability initialization');
assert.equal(engine.nested_format_probe(70), 'prefix value70', 'nested main formatting survives capability initialization');
process.stdout.write(`${JSON.stringify({ passed: true, fetches, memory_bytes: raw.memory.buffer.byteLength })}\n`);
