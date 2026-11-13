// Exercise the shipped raster encoder across its real shared-memory boundary.
import assert from 'node:assert/strict';
import { readFileSync, realpathSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';
import { verify } from '../web/tools/sdk.mjs';

const directory = realpathSync(resolve(process.argv[2] || 'web/runtime'));
const manifest = verify(directory, { release: true });
const fetched = [];
globalThis.fetch = async input => {
  const url = String(input instanceof Request ? input.url : input);
  fetched.push(fileURLToPath(url));
  return new Response(readFileSync(fileURLToPath(url)));
};
await import(pathToFileURL(join(directory, manifest.entry)).href);
const engine = globalThis[Symbol.for(`animatix.sdk.engine.${manifest.build_id}`)];
assert(engine, 'versioned SDK entry did not register');
engine.initSync({ module: new WebAssembly.Module(readFileSync(join(directory, manifest.wasm))) });
await engine.ensure_capabilities(8, manifest.files);
const width = 7; const height = 5;
const rgba = Uint8Array.from({ length: width * height * 4 }, (_, index) => (index * 37 + 19) % 256);
const png = engine.encode_rgba_png(rgba, width, height);
assert.deepEqual(decodeRgbaPng(png, width, height), Buffer.from(rgba));
assert.deepEqual(engine.encode_rgba_png(rgba, width, height), png, 'repeat encoding changed the PNG');
await engine.ensure_capabilities(8, manifest.files);
const expected = manifest.capabilities.raster_encode.files.map(file => join(directory, manifest.files[file].gzip_url));
assert.deepEqual([...fetched].sort(), expected.sort(), 'encoder loaded an unused capability or downloaded a block twice');
process.stdout.write(`${JSON.stringify({ passed: true, build_id: manifest.build_id, fetched, png_bytes: png.length })}\n`);

function decodeRgbaPng(bytes, width, height) {
  bytes = Buffer.from(bytes);
  assert.deepEqual(bytes.subarray(0, 8), Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  const data = []; let offset = 8; let header;
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const kind = bytes.toString('ascii', offset + 4, offset + 8);
    const chunk = bytes.subarray(offset + 8, offset + 8 + length);
    if (kind === 'IHDR') header = chunk;
    if (kind === 'IDAT') data.push(chunk);
    offset += length + 12;
  }
  assert(header, 'PNG has no header');
  assert.equal(header.readUInt32BE(0), width); assert.equal(header.readUInt32BE(4), height);
  assert.equal(header[8], 8); assert.equal(header[9], 6); assert.equal(header[12], 0);
  const raw = inflateSync(Buffer.concat(data)); const stride = width * 4;
  assert.equal(raw.length, (stride + 1) * height);
  const output = Buffer.alloc(stride * height);
  for (let row = 0; row < height; row += 1) {
    const filter = raw[row * (stride + 1)]; assert(filter <= 4, 'unknown PNG row filter');
    for (let column = 0; column < stride; column += 1) {
      const index = row * stride + column;
      const left = column >= 4 ? output[index - 4] : 0;
      const above = row ? output[index - stride] : 0;
      const upperLeft = row && column >= 4 ? output[index - stride - 4] : 0;
      const predicted = [0, left, above, Math.floor((left + above) / 2), paeth(left, above, upperLeft)][filter];
      output[index] = (raw[row * (stride + 1) + column + 1] + predicted) & 255;
    }
  }
  return output;
}

function paeth(left, above, upperLeft) {
  const prediction = left + above - upperLeft;
  const a = Math.abs(prediction - left); const b = Math.abs(prediction - above); const c = Math.abs(prediction - upperLeft);
  return a <= b && a <= c ? left : b <= c ? above : upperLeft;
}
