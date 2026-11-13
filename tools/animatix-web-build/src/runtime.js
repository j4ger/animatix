import { initSync, activate_capabilities } from './animatix_web.js';

const manifest = __SPLIT_MANIFEST__;
const chunks = new Map(manifest.chunks.map(chunk => [chunk.id, chunk]));
const pending = new Map();
const shared = {};
let files;

async function wasmResponse(file) {
  const response = await fetch(new URL(`${file}.gz`, import.meta.url));
  if (!response.ok) throw new Error(`capability HTTP ${response.status}: ${file}`);
  const reader = response.body.getReader();
  const first = await reader.read();
  let prefix = first.value || new Uint8Array();
  while (prefix.length < 2 && !first.done) {
    const next = await reader.read();
    if (next.done) break;
    const merged = new Uint8Array(prefix.length + next.value.length);
    merged.set(prefix); merged.set(next.value, prefix.length); prefix = merged;
  }
  const stream = new ReadableStream({
    start(controller) { if (prefix.length) controller.enqueue(prefix); },
    async pull(controller) {
      const next = await reader.read();
      if (next.done) controller.close(); else controller.enqueue(next.value);
    },
    cancel(reason) { return reader.cancel(reason); },
  });
  const body = prefix[0] === 0x1f && prefix[1] === 0x8b
    ? stream.pipeThrough(new DecompressionStream('gzip')) : stream;
  return new Response(body, { headers: { 'Content-Type': 'application/wasm' } });
}

function load(id) {
  if (!pending.has(id)) {
    const chunk = chunks.get(id);
    if (!chunk) return Promise.reject(new Error(`unknown capability chunk: ${id}`));
    pending.set(id, (async () => {
      await Promise.all(chunk.dependencies.map(load));
      const base = initSync();
      const imports = { env: { memory: base.memory }, __wasm_split: { ...base, ...shared } };
      const descriptor = files?.[chunk.file];
      if (!descriptor || !/^[0-9a-f]{64}$/.test(descriptor.sha256) || !Number.isSafeInteger(descriptor.size_bytes)) throw new Error(`missing capability integrity: ${chunk.file}`);
      const response = await wasmResponse(chunk.file);
      const integrity = response.clone().arrayBuffer().then(async bytes => {
        if (bytes.byteLength !== descriptor.size_bytes) throw new Error(`capability size mismatch: ${chunk.file}`);
        const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(byte => byte.toString(16).padStart(2, '0')).join('');
        if (hash !== descriptor.sha256) throw new Error(`capability SHA-256 mismatch: ${chunk.file}`);
      });
      const [compiled] = await Promise.all([WebAssembly.compileStreaming(response), integrity]);
      const instance = await WebAssembly.instantiate(compiled, imports);
      Object.assign(shared, instance.exports);
    })().catch(error => { pending.delete(id); throw error; }));
  }
  return pending.get(id);
}

function callbackLoader(id) {
  return async (index, data) => {
    const base = initSync();
    try {
      await load(id);
      if (index !== undefined) base.__indirect_function_table.get(index)(data, true);
    } catch (error) {
      if (index !== undefined) base.__indirect_function_table.get(index)(data, false);
      throw error;
    }
  };
}

export async function ensure_capabilities(required, manifestFiles) {
  if (manifestFiles) files = manifestFiles;
  if (!Number.isInteger(required) || required < 0 || (required & ~15) !== 0) {
    throw new Error(`unsupported capability mask: ${required}`);
  }
  const needed = Object.entries(manifest.capabilities)
    .filter(([, cap]) => (required & cap.mask) !== 0)
    .map(([id]) => load(id));
  // Never start a Rust once-cell loader until every network request succeeded;
  // a transient HTTP failure therefore remains retryable on the same page.
  await Promise.all(needed);
  await activate_capabilities(required);
}
