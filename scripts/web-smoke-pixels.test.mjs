import test from 'node:test';
import assert from 'node:assert/strict';
import { deflateSync } from 'node:zlib';
import { inspectPng, assertPaint } from './web-smoke-pixels.mjs';

function chunk(kind, bytes = Buffer.alloc(0)) {
  const data = Buffer.concat([Buffer.from(kind), bytes]);
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  const result = Buffer.alloc(data.length + 8);
  result.writeUInt32BE(bytes.length); data.copy(result, 4);
  result.writeUInt32BE((crc ^ 0xffffffff) >>> 0, result.length - 4);
  return result;
}

function makePng({ width = 100, height = 60, channels = 4, filter = 0,
  pixel = (x, y) => x > 20 && x < 35 && y > 22 && y < 38 ? [101, 217, 232, 255] : [13, 21, 32, 255],
  depth = 8, interlace = 0 } = {}) {
  const stride = width * channels, rgba = Buffer.alloc(width * height * 4);
  const source = Buffer.alloc(stride * height), raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const color = pixel(x, y); rgba.set(color, (y * width + x) * 4);
    source.set(color.slice(0, channels), y * stride + x * channels);
  }
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = filter;
    for (let x = 0; x < stride; x++) {
      const index = y * stride + x;
      const a = x >= channels ? source[index - channels] : 0;
      const b = y ? source[index - stride] : 0;
      const c = y && x >= channels ? source[index - stride - channels] : 0;
      const distance = [a, b, c].map(value => Math.abs(a + b - c - value));
      const paeth = [a, b, c][distance.indexOf(Math.min(...distance))];
      const prediction = [0, a, b, Math.floor((a + b) / 2), paeth][filter] || 0;
      raw[y * (stride + 1) + x + 1] = (source[index] - prediction + 256) % 256;
    }
  }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(width); ihdr.writeUInt32BE(height, 4);
  ihdr[8] = depth; ihdr[9] = channels === 4 ? 6 : 2; ihdr[12] = interlace;
  return { rgba, png: Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND')]) };
}

for (const channels of [3, 4]) for (const filter of [0, 1, 2, 3, 4]) {
  test(`decodes ${channels}-channel PNG filter ${filter}`, () => {
    const { png, rgba } = makePng({ channels, filter });
    const image = inspectPng(png);
    assert.equal(image.width, 100); assert.equal(image.height, 60);
    assert.deepEqual(image.pixels, rgba);
  });
}

test('generic paint and cyan semantic centroid are measured in pixels', () => {
  const { png } = makePng();
  assert.ok(assertPaint(png).matching > 100);
  const result = assertPaint(png, { color: '#65D9E8', x: 0.28, y: 0.5 });
  assert.ok(result.coverage > 0.02);
  assert.throws(() => assertPaint(png, { color: '#65D9E8', x: 0.72 }), /centroid/);
  assert.throws(() => assertPaint(png, { color: '#F46EA0' }), /Insufficient/);
});

test('edited pink scene and antialiased colors allow bounded tolerance', () => {
  const { png } = makePng({ pixel: (x, y) => x > 65 && x < 80 && y > 22 && y < 38
    ? [240, 115, 157, 255] : [13, 21, 32, 255] });
  assertPaint(png, { color: '#F46EA0', x: 0.72, y: 0.5 });
});

test('blank, transparent, tiny noise and failed readback cannot pass', () => {
  for (const pixel of [() => [0, 0, 0, 255], () => [101, 217, 232, 0],
    (x, y) => x === 0 && y === 0 ? [255, 255, 255, 255] : [0, 0, 0, 255]]) {
    assert.throws(() => assertPaint(makePng({ pixel }).png), /visible|Insufficient/);
  }
  assert.throws(() => assertPaint(Buffer.alloc(0)), /signature/);
});

test('rejects corrupted, truncated and unsupported input rather than passing', () => {
  const { png } = makePng();
  assert.throws(() => inspectPng(png.subarray(0, -2)), /Truncated/);
  const corrupt = Buffer.from(png); corrupt[50] ^= 1;
  assert.throws(() => inspectPng(corrupt), /checksum/);
  assert.throws(() => inspectPng(makePng({ depth: 16 }).png), /Unsupported/);
  assert.throws(() => inspectPng(makePng({ interlace: 1 }).png), /Unsupported/);
  assert.throws(() => inspectPng(makePng({ filter: 5 }).png), /filter/);
});
