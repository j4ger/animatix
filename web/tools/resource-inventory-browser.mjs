// Self-contained CDP browser acceptance function. The caller supplies the real
// page APIs and a ready-player factory so this function can be serialized into
// the existing browser harness without relying on test-only URL imports.
export async function exerciseResourceInventory({
  createReadyPlayer,
  source,
  readWasmStats,
  readRuntimeStats,
  fetchOverlayFont,
  overlayFontUrls = [],
}) {
  if (typeof createReadyPlayer !== "function" || typeof readWasmStats !== "function" || typeof readRuntimeStats !== "function") {
    throw new TypeError("player factory and resource-stat readers are required");
  }
  const before = readWasmStats();
  const player = await createReadyPlayer();
  let evidence;
  if (!player || typeof player.applySource !== "function") throw new TypeError("factory must return a ready <amx-player>");
  try {
    await player.applySource(source);
    const warm = readWasmStats();
    const runtimeWarm = readRuntimeStats();
    await player.applySource(source);
    const repeated = readWasmStats();
    if (repeated.text_compile_entries !== warm.text_compile_entries) {
      throw new Error("text cache grew on identical source rebuild");
    }
    if (repeated.grouped_text_compile_entries !== warm.grouped_text_compile_entries) {
      throw new Error("grouped text cache grew on identical source rebuild");
    }
    if (repeated.font_context_pool_entries !== warm.font_context_pool_entries
      || repeated.font_context_active_entries !== warm.font_context_active_entries) {
      throw new Error("font context inventory changed on identical source rebuild");
    }

    if (overlayFontUrls.length && typeof fetchOverlayFont === "function") {
      for (const url of overlayFontUrls) await fetchOverlayFont(url, player);
    }
    const fonts = readRuntimeStats();
    if (fonts.fontCacheEntries > 64) throw new Error("decoded font cache exceeded its 64-entry cap");
    if (overlayFontUrls.length >= 65 && fonts.fontCacheEntries !== 64) {
      throw new Error("65 distinct overlay fonts did not fill the decoded font cache cap");
    }
    if (fonts.fontCachePending !== 0) throw new Error("font fetches should have settled");
    // Keep memory as observed evidence only; engines may retain pages after
    // caches stabilize, so no shrinkage assertion is meaningful here.
    evidence = { before, warm, repeated, runtimeWarm, fonts, wasmMemoryBytes: repeated.wasm_memory_bytes };
  } finally {
    player.destroy?.();
    player.remove?.();
  }
  const afterDestroy = readWasmStats();
  if (afterDestroy.font_context_active_entries > before.font_context_active_entries) {
    throw new Error("destroy retained an active font context");
  }
  const cycles = [];
  for (let index = 0; index < 5; index += 1) {
    const next = await createReadyPlayer();
    try { await next.applySource(source); }
    finally { next.destroy?.(); next.remove?.(); }
    cycles.push(readWasmStats());
  }
  const stable = cycles[0];
  for (const sample of cycles) {
    for (const field of ["text_compile_entries", "grouped_text_compile_entries", "font_context_pool_entries", "font_context_active_entries"]) {
      if (sample[field] !== stable[field]) throw new Error(`${field} did not converge after repeated destroy cycles`);
    }
    if (sample.font_context_active_entries > before.font_context_active_entries) {
      throw new Error("destroy cycles retained active font contexts");
    }
  }
  return { ...evidence, afterDestroy, cycles };
}
