// Tiny, explicit allowance for the one edge-coverage rounding pixel observed
// when the same static text is read back before/after presentation on WebGPU.
// This is not a perceptual image metric: alpha, dimensions and all other pixels
// must remain exact. Callers opt in per fixture; defaults remain byte-exact.
export function compareRgbaFrames(before, after, maxRgbDelta = 0, maxChangedPixels = 0) {
  if (before.width !== after.width || before.height !== after.height
      || before.bytes.length !== after.bytes.length
      || before.bytes.length !== before.width * before.height * 4) return false;
  let changedPixels = 0;
  for (let i = 0; i < before.bytes.length; i += 4) {
    if (before.bytes[i + 3] !== after.bytes[i + 3]) return false;
    let changed = false;
    for (let channel = 0; channel < 3; channel += 1) {
      const delta = Math.abs(before.bytes[i + channel] - after.bytes[i + channel]);
      if (delta > maxRgbDelta) return false;
      changed ||= delta !== 0;
    }
    changedPixels += Number(changed);
    if (changedPixels > maxChangedPixels) return false;
  }
  return true;
}
