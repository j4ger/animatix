// Shared loader for the single Animatix web SDK. The runtime module and its
// capability chunks are page scoped; player instances remain element scoped.
import { playerError } from "./errors.js";

const state = (globalThis[Symbol.for("animatix.sdk.runtime")] ??= {
  manifestUrl: null, started: false, promise: null,
});
// These defaults are added rather than replacing the page-global object: a
// page may have loaded an older embed bundle before a newer one is evaluated.
state.backendPreference ??= "webgpu";
state.backendConfigured ??= false;
state.selectedBackend ??= null;
state.backendGeneration ??= 0;
state.backendSelection ??= null;
state.webgpuInitPromise ??= null;
state.webgpuInitialized ??= false;
state.backendLost ??= false;
state.backendLossCount ??= 0;
state.players ??= new Set();
state.lossBridgeInstalled ??= false;
const manifestPromises = new Map();
const FONT_BYTES = new Map();
const BACKENDS = new Set(["auto", "webgpu", "webgl2"]);

/** Read-only inventory of this page's bounded, decoded font cache. */
export function debugRuntimeStats() {
  const entries = [...FONT_BYTES.values()];
  return {
    fontCacheEntries: entries.length,
    fontCacheRetainedBytes: entries.reduce((total, entry) => total + (entry.byteLength ?? 0), 0),
    fontCachePending: entries.filter(entry => !entry.settled).length,
    fontCacheConsumers: entries.reduce((total, entry) => total + entry.consumers, 0),
  };
}

function checkedFetch(url, kind = "bytes", signal) {
  // Cache decoded font promises and the runtime instance, never an unread
  // Response: Response.clone() would retain an unconsumed tee for every file.
  return fetch(url, { signal }).then(response => {
    if (!response.ok) throw playerError(new Error(`${kind} HTTP ${response.status}: ${url}`), "resource");
    return response;
  }).catch(error => { throw playerError(error, "resource"); });
}

export function configureRuntime(options = {}) {
  if (!options || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("configureRuntime expects an options object");
  }
  const hasManifest = Object.hasOwn(options, "manifestUrl");
  const hasBackend = Object.hasOwn(options, "backend");
  if (!hasManifest && !hasBackend) {
    throw new TypeError("configureRuntime expects { manifestUrl } and/or { backend }");
  }
  let url = state.manifestUrl;
  if (hasManifest) {
    if (typeof options.manifestUrl !== "string") throw new TypeError("manifestUrl must be a string");
    url = new URL(options.manifestUrl, globalThis.document?.baseURI ?? import.meta.url).href;
  }
  let backend = state.backendPreference;
  if (hasBackend) {
    if (typeof options.backend !== "string" || !BACKENDS.has(options.backend)) {
      throw new TypeError("backend must be 'auto', 'webgpu' or 'webgl2'");
    }
    backend = options.backend;
  }
  if (state.started && ((hasManifest && state.manifestUrl !== url) || (hasBackend && backend !== state.backendPreference))) {
    throw new Error("runtime configuration is fixed after loading begins");
  }
  if (hasManifest) state.manifestUrl = url;
  if (hasBackend) {
    state.backendPreference = backend;
    state.backendConfigured = true;
  }
}

/** The backend selected for this page after the first player was created. */
export function runtimeBackend() {
  return state.selectedBackend;
}

/** Shared device generation, useful for diagnosing a loss and recovery. */
export function runtimeBackendGeneration() {
  return state.backendGeneration;
}

function backendCandidates(preference, availableBackends) {
  const requested = preference === "auto" ? ["webgpu", "webgl2"] : [preference];
  return availableBackends ? requested.filter((backend) => availableBackends.includes(backend)) : requested;
}

function playerFactory(module, backend) {
  if (typeof module.create_player_for_backend === "function") {
    return (canvas) => module.create_player_for_backend(canvas, backend);
  }
  if (backend === "webgpu" && typeof module.create_player === "function") {
    return (canvas) => module.create_player(canvas);
  }
  // Compatibility with the first opt-in WebGL2 trial. New SDKs use the single
  // create_player_for_backend export so backend policy has one wasm boundary.
  if (backend === "webgl2" && typeof module.create_webgl_player === "function") {
    return (canvas) => module.create_webgl_player(canvas);
  }
  throw new Error(`This SDK does not include the ${backend} renderer`);
}

async function initializeBackend(module, backend) {
  if (backend === "webgpu") {
    if (typeof module.init_engine === "function") await module.init_engine();
    else if (typeof module.create_player !== "function" && typeof module.create_player_for_backend !== "function") {
      throw new Error("This SDK does not expose WebGPU initialization");
    }
  } else if (typeof module.create_player_for_backend !== "function" && typeof module.create_webgl_player !== "function") {
    throw new Error("This SDK was built without the WebGL2 renderer");
  }
}

function backendError(error, backend) {
  const wrapped = error instanceof Error ? error : new Error(String(error));
  wrapped.kind ??= "capability";
  wrapped.backend ??= backend;
  return wrapped;
}

function announceBackendLoss(detail) {
  const eventGeneration = Number.isSafeInteger(detail?.generation) ? detail.generation : null;
  if (eventGeneration !== null && eventGeneration < state.backendGeneration) return;
  if (eventGeneration !== null) state.backendGeneration = eventGeneration;
  if (detail?.backend && state.selectedBackend && detail.backend !== state.selectedBackend) return;
  if (state.backendLost) return;
  state.backendLost = true;
  state.backendLossCount += 1;
  const lost = {
    backend: detail?.backend ?? state.selectedBackend,
    generation: state.backendGeneration,
    reason: detail?.reason ?? "device-lost",
    message: detail?.message ?? "The graphics device was lost",
  };
  // WebGPU is a shared page device: every embed must rebuild together. GL
  // devices are per canvas, so their context-lost event is routed only to its
  // owning player by the element's canvas listener.
  if (lost.backend === "webgpu") {
    state.selectedBackend = null;
    state.backendSelection = null;
    state.webgpuInitialized = false;
    state.webgpuInitPromise = null;
  }
  for (const player of [...state.players]) {
    try { player._handleBackendLost?.(lost); }
    catch (error) { console.error("amx-player: backend loss handler failed", error); }
  }
}

function installBackendLossBridge() {
  if (state.lossBridgeInstalled || typeof globalThis.addEventListener !== "function") return;
  state.lossBridgeInstalled = true;
  globalThis.addEventListener("animatixbackendlost", (event) => announceBackendLoss(event.detail ?? {}));
}

async function initializeBackendOnce(module, backend, pageState = state) {
  if (backend !== "webgpu") return initializeBackend(module, backend);
  if (pageState.webgpuInitialized) return;
  if (!pageState.webgpuInitPromise) {
    pageState.webgpuInitPromise = initializeBackend(module, backend)
      .then(() => { pageState.webgpuInitialized = true; })
      .finally(() => { pageState.webgpuInitPromise = null; });
  }
  return pageState.webgpuInitPromise;
}

async function createPlayerForMode(module, backend, canvas, pageState = state) {
  // WebGPU initialization is page shared and single-flight; WebGL2 creation is
  // canvas-specific, so its wasm factory initializes the per-canvas device.
  await initializeBackendOnce(module, backend, pageState);
  return playerFactory(module, backend)(canvas);
}

async function chooseBackend(module, canvas, replaceCanvas, preference, pageState, availableBackends) {
  const candidates = backendCandidates(preference, availableBackends);
  if (candidates.length === 0) {
    throw new Error(`This SDK was built without the requested ${preference} renderer`);
  }
  let activeCanvas = canvas;
  let lastError;
  for (let index = 0; index < candidates.length; index += 1) {
    const backend = candidates[index];
    try {
      const lossCount = pageState.backendLossCount ?? 0;
      const player = await createPlayerForMode(module, backend, activeCanvas, pageState);
      if (backend === "webgpu" && (pageState.backendLossCount ?? 0) !== lossCount) {
        player?.free?.();
        throw new Error("WebGPU device was lost during initialization");
      }
      pageState.selectedBackend = backend;
      pageState.backendLost = false;
      const wasmGeneration = Number(player?.backend_generation?.());
      if (backend === "webgpu" && Number.isSafeInteger(wasmGeneration) && wasmGeneration > 0) {
        pageState.backendGeneration = wasmGeneration;
      } else {
        pageState.backendGeneration += 1;
      }
      return { player, canvas: activeCanvas, backend, generation: pageState.backendGeneration, owner: replaceCanvas };
    } catch (error) {
      lastError = backendError(error, backend);
      if (index + 1 < candidates.length && typeof replaceCanvas === "function") {
        // A failed WebGPU surface creation can already have claimed the
        // HTMLCanvasElement's context type. A fresh canvas is required before
        // asking wgpu for a WebGL2 surface.
        activeCanvas = replaceCanvas();
      }
    }
  }
  throw lastError ?? new Error("No supported graphics backend is available");
}

export async function createRuntimePlayer(
  module,
  canvas,
  replaceCanvas,
  preference = state.backendPreference,
  pageState = state,
  availableBackends = null,
) {
  installBackendLossBridge();
  if (pageState.selectedBackend && !pageState.backendLost) {
    try {
      const lossCount = pageState.backendLossCount ?? 0;
      const player = await createPlayerForMode(module, pageState.selectedBackend, canvas, pageState);
      if (pageState.selectedBackend === "webgpu" && (pageState.backendLossCount ?? 0) !== lossCount) {
        player?.free?.();
        throw new Error("WebGPU device was lost during initialization");
      }
      return { player, backend: pageState.selectedBackend, generation: pageState.backendGeneration };
    } catch (error) {
      throw backendError(error, pageState.selectedBackend);
    }
  }

  if (!pageState.backendSelection) {
    pageState.backendSelection = chooseBackend(
      module, canvas, replaceCanvas, preference, pageState, availableBackends,
    )
      .finally(() => { pageState.backendSelection = null; });
  }
  const selected = await pageState.backendSelection;
  if (selected.canvas === canvas || selected.owner === replaceCanvas) return selected;
  const player = await createPlayerForMode(module, selected.backend, canvas, pageState);
  return { player, backend: selected.backend, generation: selected.generation };
}

function registerRuntimePlayer(player) {
  state.players.add(player);
  const canvas = player._canvas;
  const onContextLost = (event) => {
    event.preventDefault();
    player._handleBackendLost?.({
      backend: "webgl2",
      generation: state.backendGeneration,
      reason: "webglcontextlost",
      message: "The WebGL2 context was lost",
      local: true,
    });
  };
  if (state.selectedBackend === "webgl2") canvas?.addEventListener?.("webglcontextlost", onContextLost);
  return () => {
    state.players.delete(player);
    canvas?.removeEventListener?.("webglcontextlost", onContextLost);
  };
}

function validateManifest(manifest) {
  if (!manifest || manifest.schema_version !== 1) {
    throw playerError(new Error(`unsupported Animatix runtime manifest schema ${manifest?.schema_version}`), "version");
  }
  if (typeof manifest.build_id !== "string" || !manifest.build_id) {
    throw playerError(new Error("runtime manifest has no build_id"), "version");
  }
  if (!manifest.entry || typeof manifest.entry !== "string") {
    throw playerError(new Error("runtime manifest has no entry module"), "version");
  }
  if (!manifest.files || typeof manifest.files !== "object") {
    throw playerError(new Error("runtime manifest has no files table"), "version");
  }
  if (manifest.render_backends !== undefined) {
    const backends = JSON.stringify(manifest.render_backends);
    if (!['["webgpu"]', '["webgpu","webgl2"]'].includes(backends)) {
      throw playerError(new Error("runtime manifest has an invalid render backend inventory"), "version");
    }
  }
  return manifest;
}

export async function loadRuntime() {
  if (!state.backendConfigured && typeof globalThis.location?.search === "string") {
    // Preserve the opt-in WebGL2 query used by the original trial while the
    // supported page-level configuration is rolled out.
    if (new URLSearchParams(globalThis.location.search).get("amx-backend") === "webgl2") {
      state.backendPreference = "webgl2";
    }
  }
  state.manifestUrl ??= new URL("./runtime-manifest.json", import.meta.url).href;
  state.started = true;
  const manifestUrl = state.manifestUrl;
  const root = new URL("./", manifestUrl).href;
  let pending = state.promise;
  if (!pending) {
    pending = (async () => {
      let manifestPromise = manifestPromises.get(manifestUrl);
      if (!manifestPromise) {
        manifestPromise = checkedFetch(manifestUrl, "runtime manifest")
          .then((response) => response.json())
          .then(validateManifest)
          .catch((error) => {
            manifestPromises.delete(manifestUrl);
            throw error;
          });
        manifestPromises.set(manifestUrl, manifestPromise);
      }
      const manifest = await manifestPromise;
      const entry = new URL(manifest.entry, manifestUrl).href;
      const entryInfo = manifest.files[manifest.entry];
      if (!entryInfo || !/^[0-9a-f]{64}$/.test(entryInfo.sha256)) {
        throw playerError(new Error("runtime manifest has no valid entry integrity"), "version");
      }
      let module;
      try {
        module = await loadIntegrityModule(entry, entryInfo.sha256, manifest.build_id);
      } catch (error) { throw playerError(error, "resource"); }
      const wasm = await loadWasmResponse(manifest, manifestUrl);
      const verification = verifyWasm(wasm.clone(), manifest.files[manifest.wasm]);
      const [compiled] = await Promise.all([WebAssembly.compileStreaming(wasm), verification]);
      await module.default({ module_or_path: compiled });
      const runtimeBuildId = module.runtime_build_id?.();
      if (runtimeBuildId !== undefined && String(runtimeBuildId) !== manifest.build_id) {
        throw playerError(new Error(
          `runtime build mismatch (manifest ${manifest.build_id}, wasm ${runtimeBuildId})`,
        ), "version");
      }
      installBackendLossBridge();
      return {
        root,
        manifest,
        module,
        backendPreference: () => state.backendPreference,
        backend: runtimeBackend,
        backendGeneration: runtimeBackendGeneration,
        createPlayer: async (canvas, replaceCanvas) => {
          const selected = await createRuntimePlayer(
            module, canvas, replaceCanvas, state.backendPreference, state, manifest.render_backends ?? null,
          );
          return selected.player;
        },
        registerPlayer: registerRuntimePlayer,
        recoverBackend: async () => {
          if (!state.backendLost) return state.selectedBackend;
          // The next createPlayer call negotiates once and shares its result
          // among every embed recovering from this loss.
          return null;
        },
        ensureCapabilities: async (required) => {
          try {
            if (typeof module.ensure_capabilities !== "function") {
              if (required) throw new Error("runtime entry does not expose ensure_capabilities");
              return;
            }
            await module.ensure_capabilities(required, manifest.files);
          } catch (error) {
            throw playerError(error, "capability");
          }
        },
        initEngine: async () => {
          try {
            await initializeBackendOnce(module, "webgpu", state);
          } catch (error) {
            throw playerError(error, "capability");
          }
        },
      };
    })().catch((error) => {
      state.promise = null;
      throw playerError(error, "version");
    });
    state.promise = pending;
  }
  return pending;
}

// A module script with SRI both verifies and executes the exact bytes once.
// Cross-origin SDK hosts must send Access-Control-Allow-Origin for anonymous
// CORS; their CSP must permit the SDK host in script-src.
export function loadIntegrityModule(url, sha256, buildId, documentRef = globalThis.document) {
  if (!/^[0-9a-f]{64}$/.test(sha256)) return Promise.reject(new Error("invalid runtime entry SHA-256"));
  if (!documentRef?.createElement) return Promise.reject(new Error("module script loading requires a document"));
  const integrity = `sha256-${globalThis.btoa(String.fromCharCode(...Uint8Array.from(sha256.match(/../g), byte => parseInt(byte, 16))))}`;
  const script = documentRef.createElement("script");
  script.type = "module";
  script.src = url;
  script.integrity = integrity;
  script.crossOrigin = "anonymous";
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      script.onload = null;
      script.onerror = null;
      script.remove?.();
    };
    script.onload = () => {
      cleanup();
      const module = globalThis[Symbol.for(`animatix.sdk.engine.${buildId}`)];
      if (!module || typeof module !== "object") reject(new Error("integrity-checked runtime did not register its API"));
      else resolve(module);
    };
    script.onerror = () => {
      cleanup();
      reject(new Error("integrity-checked runtime module failed to load (check SRI, CSP and CORS)"));
    };
    (documentRef.head || documentRef.documentElement).append(script);
  });
}

async function verifyWasm(response, descriptor) {
  if (!descriptor || !/^[0-9a-f]{64}$/.test(descriptor.sha256) || !Number.isSafeInteger(descriptor.size_bytes)) throw playerError(new Error("invalid wasm integrity descriptor"), "version");
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength !== descriptor.size_bytes) throw playerError(new Error("wasm size mismatch"), "version");
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const hash = [...digest].map(byte => byte.toString(16).padStart(2, "0")).join("");
  if (hash !== descriptor.sha256) throw playerError(new Error("wasm SHA-256 mismatch"), "version");
}

async function loadWasmResponse(manifest, manifestUrl) {
  const descriptor = manifest.files?.[manifest.wasm];
  if (!descriptor?.url) throw playerError(new Error(`runtime manifest has no file '${manifest.wasm}'`), "version");
  const primaryUrl = new URL(descriptor.url, manifestUrl).href;
  const url = descriptor.gzip_url ? new URL(descriptor.gzip_url, manifestUrl).href : primaryUrl;
  const response = await checkedFetch(url, "runtime wasm");
  return decodedWasmResponse(response);
}

export async function decodedWasmResponse(response) {
  const reader = response.body.getReader();
  let {value: prefix = new Uint8Array()} = await reader.read();
  while (prefix.length < 2) {
    const next = await reader.read(); if (next.done) break;
    const merged = new Uint8Array(prefix.length + next.value.length);
    merged.set(prefix); merged.set(next.value, prefix.length); prefix = merged;
  }
  const stream = new ReadableStream({
    start(controller) { if (prefix.length) controller.enqueue(prefix); },
    async pull(controller) { const next = await reader.read(); if(next.done) controller.close(); else controller.enqueue(next.value); },
    cancel(reason) { return reader.cancel(reason); },
  });
  let body = stream;
  if (prefix[0] === 0x1f && prefix[1] === 0x8b) {
    if (typeof DecompressionStream !== "function") {
      throw playerError(new Error("gzip Wasm decoding is unavailable"), "capability");
    }
    body = stream.pipeThrough(new DecompressionStream("gzip"));
  }
  return new Response(body, {headers: {"Content-Type": "application/wasm"}});
}

function sharedFont(key, signal, load) {
  if (signal?.aborted) return Promise.reject(new DOMException("Operation cancelled", "AbortError"));
  let entry = FONT_BYTES.get(key);
  if (!entry) {
    const controller = new AbortController();
    entry = { controller, consumers: 0, settled: false, promise: null };
    const current = entry;
    entry.promise = load(controller.signal).then(bytes => {
      current.settled = true;
      current.byteLength = bytes?.byteLength ?? 0;
      return bytes;
    }, error => {
      current.settled = true;
      if (FONT_BYTES.get(key) === current) FONT_BYTES.delete(key);
      throw error;
    });
    cacheFont(key, entry);
  }
  const current = entry;
  current.consumers += 1;
  return new Promise((resolve, reject) => {
    let done = false;
    function finish(error, bytes) {
      if (done) return;
      done = true;
      signal?.removeEventListener("abort", cancel);
      current.consumers -= 1;
      if (!current.settled && current.consumers === 0) {
        if (FONT_BYTES.get(key) === current) FONT_BYTES.delete(key);
        current.controller.abort();
      }
      if (error) reject(error); else resolve(bytes);
    }
    function cancel() { finish(new DOMException("Operation cancelled", "AbortError")); }
    signal?.addEventListener("abort", cancel, { once: true });
    current.promise.then(bytes => finish(null, bytes), error => finish(error));
    if (signal?.aborted) cancel();
  });
}

export async function fetchFontFace(runtime, face, signal) {
  const id = face.id;
  if (!id || typeof face.url !== "string") throw playerError(new Error("invalid font face in runtime manifest"), "version");
  const manifestUrl = state.manifestUrl || new URL("runtime-manifest.json", runtime.root);
  const descriptor = runtime.manifest.files?.[face.url];
  let url;
  try { url = new URL(descriptor?.url ?? face.url, manifestUrl).href; } catch (error) { throw playerError(error, "version"); }
  return sharedFont(`${runtime.manifest.build_id}:${id}`, signal, async sharedSignal => {
    const response = await checkedFetch(url, `font '${id}'`, sharedSignal);
    let buffer;
    try { buffer = await response.arrayBuffer(); } catch (error) { throw playerError(error, "resource"); }
    let bytes;
    if (new Uint8Array(buffer)[0] !== 0x1f || new Uint8Array(buffer)[1] !== 0x8b) {
      bytes = new Uint8Array(buffer);
    } else {
      if (typeof DecompressionStream !== "function") throw playerError(new Error("gzip font decoding is unavailable"), "capability");
      const stream = new Blob([buffer]).stream().pipeThrough(new DecompressionStream("gzip"));
      bytes = new Uint8Array(await new Response(stream).arrayBuffer());
    }
    if (face.size_bytes !== undefined && bytes.byteLength !== face.size_bytes) throw playerError(new Error(`font '${id}' size mismatch: expected ${face.size_bytes}, got ${bytes.byteLength}`), "version");
    if (face.sha256) {
      const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
      const hash = [...digest].map(byte => byte.toString(16).padStart(2, "0")).join("");
      if (hash.toLowerCase() !== String(face.sha256).toLowerCase()) throw playerError(new Error(`font '${id}' SHA-256 mismatch`), "version");
    }
    return bytes;
  });
}

export async function fetchOverlayFont(runtime, url, signal) {
  const validate = runtime?.module?.validate_overlay_font;
  if (typeof validate !== "function") {
    throw playerError(new Error("runtime does not support custom font validation"), "capability");
  }
  return sharedFont(`overlay:${url}`, signal, async sharedSignal => {
    const response = await checkedFetch(url, "custom font", sharedSignal);
    let bytes;
    try {
      bytes = new Uint8Array(await response.arrayBuffer());
    } catch (error) {
      throw playerError(error, "resource");
    }
    try {
      validate.call(runtime.module, bytes);
    } catch (error) {
      throw playerError(new Error(`custom font validation failed: ${error?.message ?? error}`), "resource");
    }
    return bytes;
  });
}

export async function fetchPreparedAssets(runtime, assets, signal) {
  return Promise.all((assets ?? []).map(async (asset) => {
    if (!asset?.id || typeof asset.url !== "string" || typeof asset.kind !== "string") {
      throw playerError(new Error("invalid prepared asset descriptor"), "resource");
    }
    let url;
    try { url = new URL(asset.url, runtime.root).href; } catch (error) { throw playerError(error, "resource"); }
    let response;
    try {
      response = await fetch(url, { signal });
    } catch (error) {
      throw playerError(error, "resource");
    }
    if (!response.ok) throw playerError(new Error(`asset '${asset.id}' HTTP ${response.status}: ${url}`), "resource");
    let payload;
    try {
      payload = asset.kind === "text" || asset.kind === "svg"
        ? await response.text()
        : new Uint8Array(await response.arrayBuffer());
    } catch (error) {
      throw playerError(error, "resource");
    }
    return { id: asset.id, payload };
  }));
}

export async function fetchSource(url, signal) {
  let response;
  try { response = await fetch(url, { signal }); } catch (error) { throw playerError(error, "resource"); }
  if (!response.ok) throw playerError(new Error(`HTTP ${response.status}: ${url}`), "resource");
  try { return await response.text(); } catch (error) { throw playerError(error, "resource"); }
}

export async function fetchModule(url, signal) {
  let response;
  try { response = await fetch(url, { signal }); } catch (error) { throw playerError(error, "resource"); }
  if (!response.ok) throw playerError(new Error(`HTTP ${response.status}: ${url}`), "resource");
  try { return await response.text(); } catch (error) { throw playerError(error, "resource"); }
}


/** Fetch only missing explicit scene modules. There is no implicit SDK library.
 * The caller retains source-map keys; URLs resolve against the authored scene.
 * Commit the batch only after it succeeds and still owns its cancellation token.
 */
export async function fetchMissingModules(paths, sourceUrl, modules, signal) {
  const cancelled = () => {
    if (signal?.aborted) throw new DOMException("Operation cancelled", "AbortError");
  };
  cancelled();
  const pending = [...new Set(paths)].filter(path => !modules.has(path));
  const entries = await Promise.all(pending.map(async path => [
    path, await fetchModule(new URL(path, sourceUrl).href, signal),
  ]));
  cancelled();
  for (const [path, source] of entries) modules.set(path, source);
}


function cacheFont(key, promise) {
  if (FONT_BYTES.size >= 64) FONT_BYTES.delete(FONT_BYTES.keys().next().value);
  FONT_BYTES.set(key, promise);
}
