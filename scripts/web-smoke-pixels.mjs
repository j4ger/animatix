// CDP screenshots are decoded rather than treating compressed byte length as paint.
import { inflateSync } from 'node:zlib';

const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const MAX_PIXELS = 16_777_216;

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

export function inspectPng(input) {
  const png = Buffer.isBuffer(input) ? input : Buffer.from(input);
  if (!png.subarray(0, 8).equals(SIGNATURE)) throw new Error('Invalid PNG signature');
  let offset = 8, width, height, channels, ended = false;
  const compressed = [];
  while (offset < png.length) {
    if (offset + 12 > png.length) throw new Error('Truncated PNG chunk');
    const length = png.readUInt32BE(offset), end = offset + length + 12;
    if (end > png.length) throw new Error('Truncated PNG payload');
    const kind = png.toString('ascii', offset + 4, offset + 8);
    const data = png.subarray(offset + 8, end - 4);
    if (crc32(png.subarray(offset + 4, end - 4)) !== png.readUInt32BE(end - 4)) {
      throw new Error(`Invalid PNG ${kind} checksum`);
    }
    if (!width && kind !== 'IHDR') throw new Error('PNG must begin with IHDR');
    if (kind === 'IHDR') {
      if (width || length !== 13) throw new Error('Invalid PNG IHDR');
      width = data.readUInt32BE(0); height = data.readUInt32BE(4);
      if (!width || !height || width * height > MAX_PIXELS) throw new Error('Invalid PNG dimensions');
      if (data[8] !== 8 || ![2, 6].includes(data[9]) || data[10] || data[11] || data[12]) {
        throw new Error('Unsupported PNG: expected non-interlaced 8-bit RGB/RGBA');
      }
      channels = data[9] === 2 ? 3 : 4;
    } else if (kind === 'IDAT') {
      compressed.push(data);
    } else if (kind === 'IEND') {
      if (length || end !== png.length) throw new Error('Invalid PNG ending');
      ended = true; break;
    } else if (kind === 'PLTE') {
      // RGB/RGBA may carry a suggested palette; pixels still use full channels.
      if (!length || length % 3 || length > 768) throw new Error('Invalid PNG palette');
    } else if (kind === 'tRNS' || kind[0] === kind[0].toUpperCase()) {
      throw new Error(`Unsupported PNG chunk ${kind}`);
    }
    offset = end;
  }
  if (!ended || !compressed.length) throw new Error('Incomplete PNG');
  const stride = width * channels, expected = (stride + 1) * height;
  const raw = inflateSync(Buffer.concat(compressed), { maxOutputLength: expected });
  if (raw.length !== expected) throw new Error('Invalid PNG scanline length');
  const decoded = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    if (filter > 4) throw new Error(`Unsupported PNG filter ${filter}`);
    for (let x = 0; x < stride; x++) {
      const index = y * stride + x;
      const a = x >= channels ? decoded[index - channels] : 0;
      const b = y ? decoded[index - stride] : 0;
      const c = y && x >= channels ? decoded[index - stride - channels] : 0;
      const predictor = [0, a, b, Math.floor((a + b) / 2), paeth(a, b, c)][filter];
      decoded[index] = (raw[y * (stride + 1) + x + 1] + predictor) & 255;
    }
  }
  const pixels = Buffer.alloc(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    decoded.copy(pixels, i * 4, i * channels, i * channels + 3);
    pixels[i * 4 + 3] = channels === 4 ? decoded[i * 4 + 3] : 255;
  }
  return { width, height, pixels };
}

// Generic paint proves non-uniform visible content, not correctness of a scene.
// Callers should use color + normalized centroid for controlled scene fixtures.
export function assertPaint(input, { color, x, y, tolerance = 0.06 } = {}) {
  const { width, height, pixels } = inspectPng(input);
  const total = width * height, histogram = new Map();
  let visible = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    if (pixels[i + 3] < 128) continue;
    visible++;
    const key = `${pixels[i] >> 4},${pixels[i + 1] >> 4},${pixels[i + 2] >> 4}`;
    histogram.set(key, (histogram.get(key) || 0) + 1);
  }
  if (!visible) throw new Error('No visible pixels');
  let dominant, dominantCount = 0;
  for (const [key, count] of histogram) {
    if (count > dominantCount) { dominant = key; dominantCount = count; }
  }
  const background = dominant.split(',').map(value => Number(value) * 16 + 8);
  if (color && !/^#[0-9a-f]{6}$/i.test(color)) throw new Error('Expected #RRGGBB paint color');
  const target = color ? [1, 3, 5].map(start => parseInt(color.slice(start, start + 2), 16)) : null;
  let count = 0, sumX = 0, sumY = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    if (pixels[i + 3] < 128) continue;
    const reference = target || background;
    const delta = Math.max(Math.abs(pixels[i] - reference[0]),
      Math.abs(pixels[i + 1] - reference[1]), Math.abs(pixels[i + 2] - reference[2]));
    const matches = target ? delta <= 32 : delta > 32;
    if (!matches) continue;
    count++; sumX += (i / 4) % width + 0.5; sumY += Math.floor(i / 4 / width) + 0.5;
  }
  const minimum = Math.max(8, Math.ceil(total * 0.0005));
  if (count < minimum) throw new Error(`Insufficient ${color || 'non-background'} paint: ${count}/${total} pixels (need ${minimum})`);
  const centroid = { x: sumX / count / width, y: sumY / count / height };
  for (const [axis, expected] of Object.entries({ x, y })) {
    if (expected !== undefined && (!Number.isFinite(expected) || Math.abs(centroid[axis] - expected) > tolerance)) {
      throw new Error(`Paint centroid ${axis}=${centroid[axis].toFixed(3)}, expected ${expected} ± ${tolerance}`);
    }
  }
  return { width, height, visible, matching: count, coverage: count / total, centroid };
}
