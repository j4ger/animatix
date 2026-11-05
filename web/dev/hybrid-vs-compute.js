import init, * as bench from "./pkg-hybrid-bench/animatix_hybrid_bench.js";

const $ = (id) => document.getElementById(id);
const statusEl = $("status");
const tbody = document.querySelector("#out tbody");
const results = [];
window.__bench = { results, webgpu: false, webgl2: false, ready: false, error: null };

function setStatus(msg, isErr) {
  statusEl.textContent = msg || "";
  statusEl.className = "status" + (isErr ? " err" : "");
}

function webgl2Available() {
  // Probe on a throwaway canvas so we never create a context on the real one
  // with the wrong attributes (WebGlRenderer needs antialias:false).
  try {
    const c = document.createElement("canvas");
    return !!(c.getContext("webgl2"));
  } catch (_) {
    return false;
  }
}

async function detect() {
  let gpu = false;
  try {
    if (navigator.gpu && (await navigator.gpu.requestAdapter())) gpu = true;
  } catch (_) {}
  window.__bench.webgpu = gpu;
  window.__bench.webgl2 = webgl2Available();
  $("run-compute").disabled = !gpu;
  $("run-hybrid").disabled = !window.__bench.webgl2;
  if ($("run-hybrid-wgpu")) {
    // Only present in the `--features wgpu` build of the wasm module.
    const hasWgpu = typeof bench.run_hybrid_wgpu === "function";
    $("run-hybrid-wgpu").disabled = !(gpu && hasWgpu);
  }
  if (!gpu) setStatus("WebGPU unavailable here — compute disabled (hybrid still runs).");
}

function fmt(n) {
  return Number.isFinite(n) ? n.toFixed(2) : "—";
}

function addRow(r) {
  results.push(r);
  const tr = document.createElement("tr");
  const fps = r.per_frame_ms > 0 ? (1000 / r.per_frame_ms).toFixed(0) : "—";
  const cells = [
    r.backend, r.scene, `${r.width}×${r.height}`,
    fmt(r.per_frame_ms), fmt(r.submit_ms), fmt(r.gpu_ms), fmt(r.clear_only_ms), fps,
  ];
  for (const v of cells) {
    const td = document.createElement("td");
    td.textContent = v;
    tr.appendChild(td);
  }
  tbody.appendChild(tr);
}

function params(over) {
  over = over || {};
  const resStr = over.width && over.height ? `${over.width}x${over.height}` : $("res").value;
  const [w, h] = resStr.split("x").map(Number);
  return {
    scene: over.scene ?? $("scene").value,
    width: w,
    height: h,
    frames: over.frames ?? Math.max(1, Number($("frames").value) || 90),
    warmup: over.warmup ?? Math.max(0, Number($("warmup").value) || 0),
    reps: over.reps ?? Math.max(1, Number($("reps").value) || 1),
  };
}

async function runOne(backend, over) {
  const p = params(over);
  const canvas =
    backend === "compute" ? $("c-compute")
    : backend === "hybrid_wgpu" ? $("c-hwgpu")
    : $("c-hybrid");
  setStatus(`running ${backend} · ${p.scene} · ${p.width}×${p.height} …`);
  // Yield so the status paints before the synchronous wasm work starts.
  await new Promise((r) => requestAnimationFrame(() => r()));
  try {
    let r;
    if (backend === "compute") {
      r = await bench.run_compute(canvas, p.scene, p.width, p.height, p.frames, p.warmup, p.reps);
    } else if (backend === "hybrid_wgpu") {
      r = await bench.run_hybrid_wgpu(canvas, p.scene, p.width, p.height, p.frames, p.warmup, p.reps);
    } else {
      r = bench.run_hybrid(canvas, p.scene, p.width, p.height, p.frames, p.warmup, p.reps);
    }
    addRow(r);
    setStatus(`${backend} done: ${fmt(r.per_frame_ms)} ms/frame (${(1000 / r.per_frame_ms).toFixed(0)} fps)`);
  } catch (err) {
    const msg = (err && (err.message || err)) + "";
    addRow({
      backend, scene: p.scene, width: p.width, height: p.height,
      per_frame_ms: NaN, submit_ms: NaN, gpu_ms: NaN, clear_only_ms: NaN, error: msg,
    });
    setStatus(`${backend} failed: ${msg}`, true);
  }
}

async function runAll() {
  const scenes = ["light", "dense"];
  const resList = [[1280, 720], [640, 360]];
  const backends = [];
  if (!($("run-compute").disabled)) backends.push("compute");
  if (!($("run-hybrid").disabled)) backends.push("hybrid");
  for (const backend of backends) {
    for (const scene of scenes) {
      for (const [width, height] of resList) {
        $("scene").value = scene;
        $("res").value = `${width}x${height}`;
        await runOne(backend);
      }
    }
  }
}

$("run-compute").addEventListener("click", () => runOne("compute"));
$("run-hybrid").addEventListener("click", () => runOne("hybrid"));
$("run-all").addEventListener("click", runAll);

// Automation hooks (used by scripts/hybrid-bench-smoke.mjs).
window.__runOne = (backend, scene, width, height, frames, warmup, reps) =>
  runOne(backend, { scene, width, height, frames, warmup, reps });
window.__runAll = runAll;

(async () => {
  try {
    await init();
    bench.init_panic_hook();
    await detect();
    window.__bench.ready = true;
    setStatus(
      `ready — WebGPU: ${window.__bench.webgpu ? "yes" : "no"}, WebGL2: ${window.__bench.webgl2 ? "yes" : "no"}`,
    );
  } catch (err) {
    window.__bench.error = String(err);
    setStatus("wasm init failed: " + err, true);
  }
})();
