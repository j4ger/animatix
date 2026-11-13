import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { bundleEngine } from "./bundle-engine.mjs";

import { decodedWasmResponse, fetchOverlayFont, fetchPreparedAssets, loadIntegrityModule } from "../embed/src/player/runtime.js";
import { cancellationError, dispatchPlayerError, playerError } from "../embed/src/player/errors.js";
import { selectFontFaces } from "../embed/src/player/font-selection.js";

const acceptsTestFontBytes = (bytes) => {
  if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0) throw new Error("font data contains no supported faces");
};
const testFontRuntime = { module: { validate_overlay_font: acceptsTestFontBytes } };

test("font selection keeps primary faces, adds only needed glyph fallbacks, and resolves axes", () => {
  const faces = [
    { id: "open-regular", family: "Open Sans", weight: 400, style: "normal", coverage: [[32, 126]] },
    { id: "open-bold", family: "Open Sans", weight: 700, style: "normal", coverage: [[32, 126]] },
    { id: "open-variable", family: "Open Sans", weight: 400, style: "normal", axes: [{ tag: "wght", min: 300, max: 900 }], coverage: [[32, 126]] },
    { id: "open-italic", family: "Open Sans", weight: 400, style: "italic", coverage: [[32, 126]] },
    { id: "noto-cjk", family: "Noto Sans SC", weight: 400, style: "normal", coverage: [[0x4e00, 0x9fff]] },
    { id: "serif-450", family: "Serif", weight: 450, style: "normal", coverage: [[32, 126]] },
    { id: "fira-math", family: "Fira Math", weight: 400, style: "normal", coverage: [[0x20, 0x22ff]] },
  ];
  const runtime = { manifest: { fonts: { faces, fallbacks: ["noto-cjk"] } } };
  const ids = (requirements) => selectFontFaces(runtime, requirements).map(({ id }) => id).sort();

  assert.deepEqual(ids({ families: ["Open Sans"], weights: [400], styles: ["normal"], text: "你好" }), ["noto-cjk", "open-regular"]);
  assert.deepEqual(ids({ families: ["Open Sans"], weights: [400], styles: ["normal"], text: "Hello" }), ["open-regular"]);
  assert.deepEqual(ids({ families: ["Open Sans"], weights: [537], styles: ["normal"], text: "Hello" }), ["open-variable"]);
  assert.deepEqual(ids({ families: ["Open Sans"], weights: [400], styles: ["italic"], text: "Hello" }), ["open-italic"]);
  assert.deepEqual(ids({ families: ["Serif"], weights: [400], styles: ["normal"], text: "Hello" }), ["serif-450"]);
  assert.deepEqual(ids({ families: ["Serif"], weights: [400], styles: ["italic"], text: "Hello" }), ["serif-450"]);
  assert.deepEqual(ids({}), []);
  assert.deepEqual(ids({ families: ["Open Sans"], dynamic: true }), ["noto-cjk", "open-bold", "open-italic", "open-regular", "open-variable"]);
  assert.deepEqual(ids({ families: ["Open Sans"], weights: [400], styles: ["normal"], text: "**bold**", rich_text: true }), [
    "fira-math", "open-bold", "open-italic", "open-regular", "open-variable",
  ]);
});

test("player errors have a stable kind and amxerror detail shape", () => {
  const target = new EventTarget();
  let received;
  target.addEventListener("amxerror", (event) => { received = event.detail; });
  const diagnostics = [{ severity: "error", code: "parse", message: "bad scene" }];
  const error = new Error("bad scene");
  error.kind = "build";
  error.diagnostics = diagnostics;
  dispatchPlayerError(target, "./scene.amx", error, "resource");
  assert.deepEqual(Object.keys(received).sort(), ["diagnostics", "error", "kind", "message", "src"]);
  assert.equal(received.src, "./scene.amx");
  assert.equal(received.kind, "build");
  assert.equal(received.message, "bad scene");
  assert.equal(received.error, error);
  assert.equal(received.diagnostics, diagnostics);
});

test("cancellation is recognizable on promise errors and never emits amxerror", () => {
  const target = new EventTarget();
  let events = 0;
  target.addEventListener("amxerror", () => { events += 1; });
  const error = cancellationError("superseded");
  assert.equal(playerError(error, "build").kind, "cancel");
  assert.equal(dispatchPlayerError(target, "./scene.amx", error), null);
  assert.equal(events, 0);
});

test("runtime resource failures carry the resource kind", async () => {
  const previousFetch = globalThis.fetch;
  globalThis.fetch = async () => new Response("missing", { status: 404 });
  try {
    await assert.rejects(
      fetchPreparedAssets({ root: "https://sdk.example/runtime/" }, [
        { id: "picture", url: "https://scene.example/picture.png", kind: "image" },
      ]),
      (error) => error.kind === "resource" && error.message.includes("HTTP 404"),
    );
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("resource body failures keep the resource kind", async () => {
  const previousFetch = globalThis.fetch;
  globalThis.fetch = async () => ({
    ok: true,
    arrayBuffer: async () => { throw new TypeError("connection reset while reading body"); },
  });
  try {
    await assert.rejects(
      fetchPreparedAssets({ root: "https://sdk.example/runtime/" }, [
        { id: "picture", url: "https://scene.example/picture.bin", kind: "image" },
      ]),
      (error) => error.kind === "resource" && error.message.includes("connection reset"),
    );
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("invalid resource URLs keep the resource kind", async () => {
  await assert.rejects(
    fetchPreparedAssets({ root: "https://sdk.example/runtime/" }, [
      { id: "picture", url: "http://[invalid", kind: "image" },
    ]),
    (error) => error.kind === "resource",
  );
});

test("gzip Wasm without DecompressionStream is a capability failure", async () => {
  const previousDecompressionStream = globalThis.DecompressionStream;
  globalThis.DecompressionStream = undefined;
  try {
    await assert.rejects(
      decodedWasmResponse(new Response(new Uint8Array([0x1f, 0x8b, 0x00]))),
      (error) => error.kind === "capability" && error.message.includes("gzip Wasm decoding"),
    );
  } finally {
    globalThis.DecompressionStream = previousDecompressionStream;
  }
});

test("prepared asset payload type follows the Rust resource kind, not its URL suffix", async () => {
  const previousFetch = globalThis.fetch;
  const requested = [];
  globalThis.fetch = async (url) => {
    requested.push(String(url));
    if (String(url).includes("as-svg.png")) {
      return new Response("<svg/>", { status: 200 });
    }
    return new Response(new Uint8Array([1, 2, 3]), { status: 200 });
  };
  try {
    const assets = await fetchPreparedAssets(
      { root: "https://sdk.example/runtime/" },
      [
        { id: "vector-resource", url: "https://scene.example/as-svg.png", kind: "svg" },
        { id: "raster-resource", url: "https://scene.example/as-image.svg?rev=3", kind: "image" },
      ],
    );
    assert.equal(assets[0].id, "vector-resource");
    assert.equal(assets[0].payload, "<svg/>");
    assert.equal(assets[1].id, "raster-resource");
    assert.deepEqual([...assets[1].payload], [1, 2, 3]);
    assert.deepEqual(requested, [
      "https://scene.example/as-svg.png",
      "https://scene.example/as-image.svg?rev=3",
    ]);
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("shared font fetches return independent readable response bodies", async () => {
  const previousFetch = globalThis.fetch;
  let count = 0;
  globalThis.fetch = async () => {
    count += 1;
    return new Response(new Uint8Array([9, 8, 7]), { status: 200 });
  };
  try {
    const url = `https://fonts.example/${crypto.randomUUID()}.ttf`;
    const [first, second] = await Promise.all([
      fetchOverlayFont(testFontRuntime, url), fetchOverlayFont(testFontRuntime, url),
    ]);
    assert.equal(count, 1);
    assert.deepEqual([...first], [9, 8, 7]);
    assert.deepEqual([...second], [9, 8, 7]);
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("HTTP 200 invalid overlay font is rejected, evicted, and retried from the same URL", async () => {
  const previousFetch = globalThis.fetch;
  const valid = new Uint8Array([9, 8, 7]);
  let requests = 0;
  const runtime = {
    module: {
      validate_overlay_font(bytes) {
        if (bytes.length !== valid.length || bytes.some((byte, index) => byte !== valid[index])) {
          throw new Error("font data contains no supported faces");
        }
      },
    },
  };
  globalThis.fetch = async () => {
    requests += 1;
    return new Response(requests === 1 ? "<html>not a font</html>" : valid, { status: 200 });
  };
  try {
    const url = `https://fonts.example/${crypto.randomUUID()}.ttf`;
    await assert.rejects(
      fetchOverlayFont(runtime, url),
      (error) => error.kind === "resource" && error.message.includes("no supported faces"),
    );
    assert.deepEqual([...await fetchOverlayFont(runtime, url)], [...valid]);
    assert.equal(requests, 2, "failed validation remained cached instead of fetching the repaired same URL");
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("cancelling one font consumer preserves a shared download, while the last cancellation aborts it", async () => {
  const previousFetch = globalThis.fetch;
  let requests = 0;
  let aborted = 0;
  const pending = [];
  globalThis.fetch = (_url, { signal }) => {
    requests += 1;
    return new Promise((resolve, reject) => {
      pending.push(() => resolve(new Response(new Uint8Array([4, 5, 6]))));
      signal.addEventListener("abort", () => { aborted += 1; reject(new DOMException("aborted", "AbortError")); }, { once: true });
    });
  };
  try {
    const url = `https://fonts.example/${crypto.randomUUID()}.ttf`;
    const first = new AbortController();
    const second = new AbortController();
    const a = fetchOverlayFont(testFontRuntime, url, first.signal);
    const b = fetchOverlayFont(testFontRuntime, url, second.signal);
    first.abort();
    await assert.rejects(a, { name: "AbortError" });
    assert.equal(aborted, 0);
    pending[0]();
    assert.deepEqual([...await b], [4, 5, 6]);
    assert.equal(requests, 1);
    const last = new AbortController();
    const c = fetchOverlayFont(testFontRuntime, `${url}?other`, last.signal);
    last.abort();
    await assert.rejects(c, { name: "AbortError" });
    assert.equal(aborted, 1);
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("runtime inventory reports pending and bounded decoded font bytes", async () => {
  const runtime = await import("../embed/src/player/runtime.js?inventory-isolated");
  const previousFetch = globalThis.fetch;
  let resolvePending;
  globalThis.fetch = async (_url, { signal }) => {
    if (String(_url).endsWith("pending.ttf")) {
      return new Promise((resolve, reject) => {
        resolvePending = () => resolve(new Response(new Uint8Array([1, 2, 3, 4])));
        signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")), { once: true });
      });
    }
    return new Response(new Uint8Array([7, 8, 9, 10]), { status: 200 });
  };
  try {
    const pending = runtime.fetchOverlayFont(testFontRuntime, "https://fonts.example/pending.ttf");
    assert.deepEqual(runtime.debugRuntimeStats(), {
      fontCacheEntries: 1,
      fontCacheRetainedBytes: 0,
      fontCachePending: 1,
      fontCacheConsumers: 1,
    });
    resolvePending();
    await pending;
    for (let index = 0; index < 70; index += 1) {
      await runtime.fetchOverlayFont(testFontRuntime, `https://fonts.example/unique-${index}.ttf`);
    }
    assert.deepEqual(runtime.debugRuntimeStats(), {
      fontCacheEntries: 64,
      fontCacheRetainedBytes: 256,
      fontCachePending: 0,
      fontCacheConsumers: 0,
    });
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("integrity module loader sets SRI and resolves the API registered by the executed module", async () => {
  const buildId = `test-${crypto.randomUUID()}`;
  const api = { ensure_capabilities() {} };
  let script;
  const documentRef = { createElement: () => (script = {}), head: { append(element) {
    assert.equal(element.type, "module");
    assert.equal(element.src, "https://cdn.example/sdk/entry.js");
    assert.equal(element.crossOrigin, "anonymous");
    assert.match(element.integrity, /^sha256-[A-Za-z0-9+/]+=*$/);
    globalThis[Symbol.for(`animatix.sdk.engine.${buildId}`)] = api;
    element.onload();
  } } };
  try {
    assert.equal(await loadIntegrityModule("https://cdn.example/sdk/entry.js", "ab".repeat(32), buildId, documentRef), api);
    assert.equal(script.onload, null);
    assert.equal(script.onerror, null);
  } finally {
    delete globalThis[Symbol.for(`animatix.sdk.engine.${buildId}`)];
  }
});

test("integrity module loader rejects failed SRI/CSP/CORS script loads", async () => {
  const documentRef = { createElement: () => ({}), head: { append(element) { element.onerror(); } } };
  await assert.rejects(
    loadIntegrityModule("https://cdn.example/sdk/entry.js", "cd".repeat(32), crypto.randomUUID(), documentRef),
    /SRI, CSP and CORS/,
  );
});

test("SDK entry bundling removes external JS imports and preserves import.meta.url", async (t) => {
  const dir = mkdtempSync(join(tmpdir(), "animatix-sdk-bundle-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, "animatix_web.js"), `import { helper } from './__wasm_split.js';\nexport const value = helper();\nexport const wasmUrl = new URL('animatix_web_bg.wasm', import.meta.url).href;\n`);
  writeFileSync(join(dir, "__wasm_split.js"), `export const helper = () => 'ready';\n`);
  const buildId = "f".repeat(64);
  const other = mkdtempSync(join(tmpdir(), "animatix-sdk-another-stage-"));
  t.after(() => rmSync(other, { recursive: true, force: true }));
  for (const name of ["animatix_web.js", "__wasm_split.js"]) {
    writeFileSync(join(other, name), readFileSync(join(dir, name)));
  }
  await bundleEngine(dir, buildId);
  await bundleEngine(other, buildId);
  const bundle = readFileSync(join(dir, "animatix_web.js"), "utf8");
  assert.equal(readFileSync(join(other, "animatix_web.js"), "utf8"), bundle,
    "same SDK identity must produce the same SRI entry across staging directories");
  assert.doesNotMatch(bundle, /^\s*import\s/m);
  assert.match(bundle, /import\.meta\.url/);
  assert.ok(bundle.includes(`animatix.sdk.engine.${buildId}`));
  assert.equal(readFileSync(join(dir, "__wasm_split.js"), "utf8").includes("helper"), true);
});
