import assert from 'node:assert/strict';
import test from 'node:test';
import { compareRgbaFrames } from './rgba-comparison.mjs';
const frame = bytes => ({ width: 2, height: 1, bytes });
const before = frame([82, 82, 82, 255, 255, 255, 255, 255]);
test('default comparison stays exact', () => {
  assert(compareRgbaFrames(before, structuredClone(before)));
  assert(!compareRgbaFrames(before, frame([83, 83, 83, 255, 255, 255, 255, 255])));
});
test('explicit one-pixel RGB rounding allowance', () => {
  assert(compareRgbaFrames(before, frame([83, 83, 83, 255, 255, 255, 255, 255]), 1, 1));
});
test('rejects larger, widespread, alpha and dimensional changes', () => {
  for (const bytes of [
    [84, 82, 82, 255, 255, 255, 255, 255],
    [83, 82, 82, 255, 254, 255, 255, 255],
    [82, 82, 82, 254, 255, 255, 255, 255],
    [0, 0, 0, 255, 0, 0, 0, 255],
    [255, 255, 255, 255, 82, 82, 82, 255],
  ]) assert(!compareRgbaFrames(before, frame(bytes), 1, 1));
  assert(!compareRgbaFrames(before, { ...before, width: 1, height: 2 }, 1, 1));
  assert(!compareRgbaFrames(before, frame([82, 82, 82, 255]), 1, 1));
});
