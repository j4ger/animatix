#!/usr/bin/env node
// Alternating real-WebGPU A/B benchmark for the legacy embed loader and the
// single-SDK <amx-player>. It always runs the retained legacy Wasm package
// from the original refactor baseline and requires a separately built final
// SDK under web/runtime (or WPR_NEW_RUNTIME). No generated artifacts are
// checked into source control.
//
// Usage (inside `nix develop`, headed against the NVIDIA Vulkan ICD):
//   DISPLAY=:0 VK_ICD_FILENAMES=/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json \
//     WPR_GPU=1 node scripts/web-runtime-perf.mjs
//
// WPR_SCENE=testfixtures/web-runtime-perf/cjk.amx WPR_REPS=6 WPR_WARMUPS=1 \
//   WPR_NEW_RUNTIME=web/runtime WPR_OUT=target/perf/web-runtime.json
// The harness reports raw paired samples, 95% intervals, and provisional
// relative-plus-absolute materiality floors for primary user-impact metrics.
// No performance measurements are checked in by this script change. Run only
// after the final SDK passes its current-input verification and the GPU host is idle.

import { execFileSync, spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';
import { gzipSync } from 'node:zlib';
import { verify as verifySdk } from '../web/tools/sdk.mjs';
import { combinePrimaryVerdict, comparePaired } from '../web/tools/perf-statistics.mjs';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const boundedInt = (value, fallback, min, max) => Math.min(max, Math.max(min, Number.isFinite(Number(value)) ? Number(value) : fallback));
function configuredNumber(name, fallback, minimum) {
  const raw = process.env[name];
  if (raw === undefined || raw === '') return fallback;
  const value = Number(raw);
  if (!Number.isFinite(value) || value < minimum) fail(`${name} must be a finite number >= ${minimum}`);
  return value;
}
const BASELINE = join(ROOT, 'target/perf/distribution-refactor/baseline/web-runtime/old');
const NEW_RUNTIME = resolve(ROOT, process.env.WPR_NEW_RUNTIME || 'web/runtime');
const OLD_COMMIT = 'ad08d647179691a55efd8f0811f11ecbc89ea18b';
const REPS = boundedInt(process.env.WPR_REPS,6,3,12);
const WARMUPS = boundedInt(process.env.WPR_WARMUPS,1,0,4);
const WARM_REPS = boundedInt(process.env.WPR_WARM_REPS,3,2,8);
const FRAME_COUNT = boundedInt(process.env.WPR_FRAMES,90,20,2000);
const DRAIN_FRAMES = boundedInt(process.env.WPR_DRAIN_FRAMES,30,5,300);
const SCALE = Number(process.env.WPR_SCALE || 0.85);
const QUALITY = process.env.WPR_QUALITY || 'production';
const WIDTH = 960;
const HEIGHT = 540;
const FRAME_BUDGET_MS = 1000 / 60;
const RELATIVE_MARGIN = configuredNumber('WPR_RELATIVE_MARGIN', 1.05, 1);
const PRIMARY_METRIC_POLICY = Object.freeze({
  cold_startup_ms: { absoluteThreshold: configuredNumber('WPR_COLD_READY_FLOOR_MS', 3 * FRAME_BUDGET_MS, 0), rationale: 'three reference 60 Hz frame intervals (~50 ms)' },
  warm_startup_ms: { absoluteThreshold: configuredNumber('WPR_WARM_READY_FLOOR_MS', FRAME_BUDGET_MS, 0), rationale: 'one reference 60 Hz frame interval (~16.7 ms)' },
  queue_depth_one_frame_p95_ms: { absoluteThreshold: configuredNumber('WPR_QUEUE_DEPTH_ONE_P95_FLOOR_MS', FRAME_BUDGET_MS * 0.01, 0), rationale: 'one percent of a reference 60 Hz frame interval (~0.167 ms)' },
});
const PERFORMANCE_POLICY = {
  relative_margin: RELATIVE_MARGIN,
  confidence_level: 0.95,
  configuration_variables: {
    relative_margin: 'WPR_RELATIVE_MARGIN',
    cold_startup_floor_ms: 'WPR_COLD_READY_FLOOR_MS',
    warm_startup_floor_ms: 'WPR_WARM_READY_FLOOR_MS',
    queue_depth_one_p95_floor_ms: 'WPR_QUEUE_DEPTH_ONE_P95_FLOOR_MS',
  },
  primary_metrics: Object.fromEntries(Object.entries(PRIMARY_METRIC_POLICY).map(([name, policy]) => [name, {
    absolute_floor_ms: policy.absoluteThreshold,
    rationale: policy.rationale,
  }])),
  rule: 'Block only when the paired 95% lower bounds exceed both the relative margin and the metric absolute floor; no material regression when either upper bound stays within its floor; otherwise inconclusive.',
  floor_status: 'provisional engineering materiality floors, not validated product SLOs',
};
const DEFAULT_SCENES = ['testfixtures/web-runtime-perf/english.amx','testfixtures/web-runtime-perf/cjk.amx','testfixtures/web-runtime-perf/rich-media.amx'];
export function parseScenes(value = undefined) {
  const input = value === undefined ? DEFAULT_SCENES.join(',') : value;
  const scenes = input.split(',').map(scene => scene.trim()).filter(Boolean);
  if (scenes.length === 0) throw new Error('WPR_SCENE must select at least one scene; empty performance workloads are invalid');
  if (scenes.length > 10) throw new Error('WPR_SCENE accepts at most 10 scenes per run');
  return scenes;
}
export function contentType(path) {
  if (path.endsWith('.html')) return 'text/html; charset=utf-8';
  if (path.endsWith('.wasm')) return 'application/wasm';
  if (path.endsWith('.js') || path.endsWith('.mjs')) return 'text/javascript';
  if (path.endsWith('.json')) return 'application/json';
  if (path.endsWith('.amx')) return 'text/plain';
  if (path.endsWith('.png')) return 'image/png';
  return 'application/octet-stream';
}
const SCENES = parseScenes(process.env.WPR_SCENE);
const OUT = resolve(ROOT, process.env.WPR_OUT || 'target/perf/web-runtime.json');
const CHROME = process.env.CHROME || 'chromium';
const GPU = process.env.WPR_GPU === '1';
const PROFILE = process.env.WPR_HEADED === '0' ? false : true;
const sleep = ms => new Promise(resolveSleep => setTimeout(resolveSleep, ms));
const log = message => process.stderr.write(`web-runtime-perf: ${message}\n`);

function fail(message) { throw new Error(message); }
function run(command, args, options = {}) {
  return execFileSync(command, args, { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options }).trim();
}
function requireFile(path, label) { if (!existsSync(path)) fail(`${label} missing: ${path}`); }
function freePort() {
  return new Promise((resolvePort, reject) => {
    const server = net.createServer(); server.unref(); server.once('error', reject);
    server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolvePort(port)); });
  });
}
async function waitFor(label, ms, probe) {
  const until = Date.now() + ms; let last;
  while (Date.now() < until) {
    try { const value = await probe(); if (value) return value; } catch (error) { last = error; }
    await sleep(150);
  }
  fail(`timeout waiting for ${label}${last ? `: ${last.message}` : ''}`);
}

// CDP uses bounded requests; an unresponsive browser cannot strand this task.
class Cdp {
  constructor(url) { this.ws = new WebSocket(url); this.nextId = 0; this.pending = new Map(); }
  async open() {
    await Promise.race([
      new Promise((ok, no) => { this.ws.addEventListener('open', ok, { once: true }); this.ws.addEventListener('error', no, { once: true }); }),
      new Promise((_, no) => setTimeout(() => no(new Error('CDP socket open timeout')), 8000)),
    ]);
    this.ws.addEventListener('message', event => {
      const message = JSON.parse(event.data); const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id); clearTimeout(pending.timer);
      if (message.error) pending.reject(new Error(JSON.stringify(message.error)));
      else pending.resolve(message.result);
    });
  }
  send(method, params = {}, sessionId) {
    const id = ++this.nextId;
    return new Promise((resolveSend, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP ${method} timeout`)); }, 15000);
      this.pending.set(id, { resolve: resolveSend, reject, timer });
      this.ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }
  close() { try { this.ws.close(); } catch {} }
}
function killGroup(child) {
  if (!child || child.exitCode !== null) return;
  try { process.kill(-child.pid, 'SIGKILL'); } catch { try { child.kill('SIGKILL'); } catch {} }
}

function prepareSite(staging) {
  requireFile(join(BASELINE, 'pkg/animatix_web.js'), 'pinned old Wasm glue');
  requireFile(join(BASELINE, 'pkg-slim/animatix_web.js'), 'pinned old slim Wasm glue');
  requireFile(join(BASELINE, 'pkg-assets/animatix_web.js'), 'pinned old assets Wasm glue');
  requireFile(join(BASELINE, 'manifest.json'), 'pinned old font manifest');
  requireFile(join(NEW_RUNTIME, 'runtime-manifest.json'), 'final SDK runtime manifest');
  requireFile(join(NEW_RUNTIME, 'amx-player.js'), 'final SDK bundled entry');
  const sdk = verifySdk(NEW_RUNTIME, { current: true, commit: run('git', ['rev-parse', 'HEAD']) });
  if (sdk.build.profile !== 'release') fail(`new SDK is not a release build: ${sdk.build.profile}`);
  for (const scene of SCENES) sceneFile(scene);
  const stage = join(staging, 'site'); mkdirSync(stage, { recursive: true });

  const old = join(stage, 'old');
  mkdirSync(join(old, 'embed'), { recursive: true });
  mkdirSync(join(old, 'fonts'), { recursive: true });
  for (const profile of ['pkg', 'pkg-slim', 'pkg-assets']) cpSync(join(BASELINE, profile), join(old, profile), { recursive: true });
  copyFileSync(join(BASELINE, 'manifest.json'), join(old, 'fonts/manifest.json'));
  // The retained legacy packages used these same raw font files. Re-gzip them
  // from the source assets so old and new see byte-identical faces.
  const rawFonts = join(ROOT, 'crates/animatix-text/assets/fonts');
  const legacyManifest = JSON.parse(readFileSync(join(BASELINE, 'manifest.json'), 'utf8'));
  for (const name of new Set(Object.values(legacyManifest.profiles).flat())) {
    const rawName = name.replace(/\.gz$/, '');
    requireFile(join(rawFonts, rawName), `original raw font ${rawName}`);
    writeFileSync(join(old, 'fonts', name), gzipSync(readFileSync(join(rawFonts, rawName)), { level: 9 }));
  }

  // Rebuild only the HEAD-pinned embed JavaScript. The Wasm packages above
  // remain byte-for-byte artifacts from the original baseline.
  const sourceRoot = join(staging, 'old-source'); mkdirSync(join(sourceRoot, 'player'), { recursive: true });
  const files = run('git', ['ls-tree', '-r', '--name-only', OLD_COMMIT, 'web/embed/src']).split('\n').filter(Boolean);
  for (const file of files) {
    const relative = file.slice('web/embed/src/'.length);
    const destination = join(sourceRoot, relative); mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, run('git', ['show', `${OLD_COMMIT}:${file}`]));
  }
  const require = createRequire(join(ROOT, 'web/tools/package.json'));
  const esbuild = require('esbuild');
  esbuild.buildSync({ entryPoints: [join(sourceRoot, 'amx-player.js')], outfile: join(old, 'embed/amx-player.js'), bundle: true, format: 'esm', target: 'es2022', minify: true });

  const fresh = join(stage, 'fresh'); mkdirSync(fresh, { recursive: true });
  cpSync(NEW_RUNTIME, join(fresh, 'runtime'), { recursive: true });
  writeFileSync(join(stage, 'old-page.html'), page('old'));
  writeFileSync(join(stage, 'fresh-page.html'), page('fresh'));
  return stage;
}

function sceneFile(scene) {
  const path=resolve(ROOT,scene);
  if(!path.startsWith(ROOT+sep)) fail(`scene must be inside the workspace: ${scene}`);
  requireFile(path,`performance scene ${scene}`);
  return {path, url:`/repo/${relative(ROOT,path).split(sep).map(encodeURIComponent).join('/')}`};
}

export function page(kind) {
  const old = kind === 'old';
  const setup = old
    ? `const moduleScript = document.createElement('script'); moduleScript.type='module'; moduleScript.src='/old/embed/amx-player.js'; moduleScript.dataset.runtimeBase='/old'; moduleScript.dataset.fontBase='/old/fonts'; document.head.append(moduleScript); await new Promise((resolve,reject)=>{moduleScript.onload=resolve; moduleScript.onerror=()=>reject(Error('legacy loader failed'));});`
    : `const {configureRuntime,debugRuntimeStats} = await import('/fresh/runtime/amx-player.js'); window.__debugRuntimeStats=debugRuntimeStats; configureRuntime({manifestUrl: location.origin + '/fresh/runtime/runtime-manifest.json'});`;
  return `<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:#111} amx-player{display:block;width:${WIDTH}px;height:${HEIGHT}px}</style><main></main><script type="module">
    const quality=${JSON.stringify(QUALITY)}, scale=${SCALE}, frames=${FRAME_COUNT}, drainFrames=${DRAIN_FRAMES};
    const phases={prepare_ms:null,build_ms:null};
    const wasmMemories=new Set();
    window.__deviceAdapterInfo=[];
    const isNvidiaInfo=info=>/^(?:0x)?10de$/i.test(String(info?.vendor||'').trim())||/\\bnvidia\\b/i.test(String(info?.vendor||'')+' '+String(info?.device||'')+' '+String(info?.description||''));
    const assertNvidiaDeviceRequests=()=>{if(!window.__deviceAdapterInfo.length)throw Error('engine created no WebGPU device; selected adapter is unverified');if(window.__deviceAdapterInfo.some(info=>!info.nvidia_verified))throw Error('engine used a WebGPU adapter that is not verified as NVIDIA: '+JSON.stringify(window.__deviceAdapterInfo))};
    const recordWasmInstance=value=>{const instance=value?.instance??value;try{for(const exported of Object.values(instance?.exports??{}))if(exported instanceof WebAssembly.Memory)wasmMemories.add(exported)}catch{}};
    const wrapWasmFactory=name=>{const original=WebAssembly[name];if(typeof original!=='function')return;WebAssembly[name]=function(...args){return Reflect.apply(original,this,args).then(result=>{recordWasmInstance(result);return result})}};
    wrapWasmFactory('instantiate'); wrapWasmFactory('instantiateStreaming');
    const quantile=(xs,p)=>{const sorted=[...xs].sort((a,b)=>a-b);return sorted[Math.min(sorted.length-1,Math.floor((sorted.length-1)*p))]};
    const inkCount=(bytes,width,height)=>{const points=[[0,0],[width-1,0],[0,height-1],[width-1,height-1]],samples=points.map(([x,y])=>[0,1,2,3].map(c=>bytes[(y*width+x)*4+c])),background=[0,1,2,3].map(c=>samples.map(p=>p[c]).sort((a,b)=>a-b)[2]);let pixels=0;for(let i=0;i<bytes.length;i+=4){const rgb=Math.abs(bytes[i]-background[0])+Math.abs(bytes[i+1]-background[1])+Math.abs(bytes[i+2]-background[2]);if(rgb>36||Math.abs(bytes[i+3]-background[3])>16)pixels++}return{pixels,background}};
    const errorText=e=>String(e?.stack||e?.message||e);
    window.__result=null; window.__failure=null;
    try {
      if(typeof GPUAdapter==='undefined')throw Error('WebGPU adapter interface unavailable before runtime import');
      const originalRequestDevice=GPUAdapter.prototype.requestDevice;
      GPUAdapter.prototype.requestDevice=function(...args){const info=this.info||{};window.__deviceAdapterInfo.push({vendor:info.vendor||'',architecture:info.architecture||'',device:info.device||'',description:info.description||'',nvidia_verified:isNvidiaInfo(info)});return originalRequestDevice.apply(this,args)};
      const start=performance.now();
      ${setup}
      const sceneUrl=new URLSearchParams(location.search).get('scene'); if(!sceneUrl)throw Error('missing scene URL');
      const el=document.createElement('amx-player'); el.setAttribute('src',sceneUrl); el.setAttribute('profile','auto'); el.setAttribute('quality',quality); el.setAttribute('aspect','16:9'); el.setAttribute('autoplay','false');
      ${old
        ? `const oldStartLoad=el._startLoad.bind(el), oldLoadScene=el._loadScene.bind(el), oldWithEngine=el._playerWithEngine.bind(el); el._startLoad=async(...args)=>{const t=performance.now();try{return await oldStartLoad(...args)}finally{phases.document_load_ms=performance.now()-t}}; el._loadScene=async(...args)=>{const t=performance.now();try{return await oldLoadScene(...args)}finally{phases.legacy_load_scene_ms=(phases.legacy_load_scene_ms||0)+(performance.now()-t)}}; el._playerWithEngine=async(...args)=>{const loaded=await oldWithEngine(...args);window.__engine=loaded.module;window.__engineProfile=args[3];return loaded};`
        : `const oldStartLoad=el._startLoad.bind(el), oldPrepare=el._prepareDocument.bind(el), oldBuild=el._buildPrepared.bind(el); el._startLoad=async(...args)=>{const t=performance.now();try{return await oldStartLoad(...args)}finally{phases.document_load_ms=performance.now()-t}}; el._prepareDocument=async(...args)=>{const t=performance.now();try{return await oldPrepare(...args)}finally{phases.prepare_ms=(phases.prepare_ms||0)+(performance.now()-t)}}; el._buildPrepared=(...args)=>{const t=performance.now();try{return oldBuild(...args)}finally{phases.build_ms=(phases.build_ms||0)+(performance.now()-t)}};`}
      document.querySelector('main').append(el);
      el._maybeStartLoading?.();
      const ready=()=>new Promise((resolve,reject)=>{const deadline=Date.now()+90000; const poll=()=>{if(el._state==='ready')return resolve();if(el._state==='error')return reject(Error('player error'));if(Date.now()>deadline)return reject(Error('player ready timeout'));setTimeout(poll,50)};poll()});
      await ready();
      const startupMs=performance.now()-start;
      const manifest=${old?'null':`await fetch('/fresh/runtime/runtime-manifest.json').then(response=>response.json())`};
      if(!window.__engine) {
        window.__engine=globalThis[Symbol.for('animatix.sdk.engine.'+manifest.build_id)];
      }
      if(typeof window.__engine?.debug_gpu_drain!=='function')throw Error('runtime does not expose GPU queue drain');
      assertNvidiaDeviceRequests();
      const player=el._player;
      if(!player) throw Error('element ready without a player');
      const startupRaster={scale:player.render_scale(),raster:[player.raster_width(),player.raster_height()]};
      // This comparison holds the engine raster fixed; it does not measure
      // either embed's adaptive quality governor.
      el.pause?.();
      el._qualityScale=scale;
      el._pixelScale=1;
      el._adaptBudget=()=>{};
      el._applyRenderScale=()=>{};
      player.set_render_scale(scale);
      const duration=player.duration_s();
      if(!Number.isFinite(duration)||duration<=0)throw Error('player reported invalid scene duration: '+duration);
      const sampleTime=(index,count)=>duration*((index+0.5)/count);
      const frameTimes=Array.from({length:frames},(_,index)=>sampleTime(index,frames));
      const drawStats=()=>player.debug_dedup_stats();
      const fixedRaster=phase=>{
        const actualScale=player.render_scale(), raster=[player.raster_width(),player.raster_height()];
        if(Math.abs(actualScale-scale)>0.001)throw Error(phase+' changed fixed render scale: requested '+scale+', actual '+actualScale);
        if(raster[0]!==Math.round(player.scene_width()*scale)||raster[1]!==Math.round(player.scene_height()*scale))throw Error(phase+' changed fixed raster dimensions: '+JSON.stringify(raster));
        return {scale:actualScale,raster};
      };
      const assertDraws=(before,after,requested,phase)=>{
        const drawn=after.drawn-before.drawn, deduped=after.deduped-before.deduped;
        if(drawn!==requested)throw Error(phase+' rendered '+drawn+'/'+requested+' requested GPU frames (deduped '+deduped+')');
        return{requested,drawn,deduped};
      };
      player.debug_bench_warmup(4);
      const readPixels=()=>new Promise((resolve,reject)=>{let done=false;const timer=setTimeout(()=>{if(!done)reject(Error('GPU readback timeout'))},10000);player.debug_readback_rgba(0,(value)=>{done=true;clearTimeout(timer);resolve(value)})});
      await readPixels();
      const rows=[];
      const rasterPhases={before_cpu:fixedRaster('before CPU samples')};
      const cpuBefore=drawStats();
      for(let rep=0;rep<3;rep++) {
        const cpu=[];
        for(let i=0;i<frames;i++){const t=performance.now();player.render_frame(frameTimes[i]);cpu.push(performance.now()-t)}
        rows.push({cpu_ms_per_frame:cpu,cpu_p50_ms:quantile(cpu,.5),cpu_p95_ms:quantile(cpu,.95),time_window_s:[frameTimes[0],frameTimes.at(-1)]});
      }
      const cpuDraws=assertDraws(cpuBefore,drawStats(),3*frames,'CPU frame samples');
      rasterPhases.after_cpu=fixedRaster('after CPU samples');
      await window.__engine.debug_gpu_drain();
      const batchBefore=drawStats();
      rasterPhases.before_drained_batch=fixedRaster('before drained batch');
      const gpuStart=performance.now();
      for(const time of frameTimes)player.render_frame(time);
      await window.__engine.debug_gpu_drain();
      const wallMs=(performance.now()-gpuStart);
      const batchDraws=assertDraws(batchBefore,drawStats(),frames,'drained batch');
      rasterPhases.after_drained_batch=fixedRaster('after drained batch');
      const drainedFrames=[];
      const queueTimes=Array.from({length:drainFrames},(_,index)=>sampleTime(index,drainFrames));
      const queueBefore=drawStats();
      rasterPhases.before_queue_depth_one=fixedRaster('before queue-depth-one samples');
      for(const time of queueTimes){const t=performance.now();player.render_frame(time);await window.__engine.debug_gpu_drain();drainedFrames.push(performance.now()-t)}
      const queueDraws=assertDraws(queueBefore,drawStats(),drainFrames,'queue-depth-one samples');
      rasterPhases.after_queue_depth_one=fixedRaster('after queue-depth-one samples');
      const pixels=await readPixels();
      const bytes=pixels.bytes;const ink=inkCount(bytes,pixels.width,pixels.height);
      if(ink.pixels < 100) throw Error('GPU pixel assertion failed: frame has no visible ink against its corner background');
      const pixelHash=[...new Uint8Array(await crypto.subtle.digest('SHA-256',bytes))].map(b=>b.toString(16).padStart(2,'0')).join('');
      if(!wasmMemories.size) throw Error('Wasm memory instrumentation captured no exported memory');
      const heapBytes=performance.memory?.usedJSHeapSize ?? null;
      let userAgentMemory=null; try { userAgentMemory=await performance.measureUserAgentSpecificMemory?.() ?? null; } catch {}
      const fontResources=performance.getEntriesByType('resource').filter(entry=>/[.](?:ttf|otf)(?:[.][0-9a-f]{16,})?[.]gz([?]|$)/i.test(entry.name)).map(entry=>({url:new URL(entry.name).pathname,duration_ms:entry.duration,transfer_bytes:entry.transferSize,encoded_bytes:entry.encodedBodySize}));
      const runtimeResources=performance.getEntriesByType('resource').filter(entry=>{const path=new URL(entry.name).pathname;return path.includes('/old/pkg')||path.includes('/fresh/runtime/')}).map(entry=>({url:new URL(entry.name).pathname,duration_ms:entry.duration,transfer_bytes:entry.transferSize,encoded_bytes:entry.encodedBodySize}));
      assertNvidiaDeviceRequests();
      window.__result={kind:${JSON.stringify(kind)},startup_ms:startupMs,document_load_ms:phases.document_load_ms,prepare_ms:phases.prepare_ms,build_ms:phases.build_ms,legacy_load_scene_ms:phases.legacy_load_scene_ms,scene_duration_s:duration,sample_time_window_s:[frameTimes[0],frameTimes.at(-1)],queue_time_window_s:[queueTimes[0],queueTimes.at(-1)],gpu_draws:{cpu_samples:cpuDraws,drained_batch:batchDraws,queue_depth_one:queueDraws},frame_samples:rows,drained_batch_ms:wallMs,drained_per_frame_ms:wallMs/frames,throughput_fps:1000*frames/wallMs,queue_depth_one_frame_samples:drainedFrames,queue_depth_one_frame_p50_ms:quantile(drainedFrames,.5),queue_depth_one_frame_p95_ms:quantile(drainedFrames,.95),quality,requested_scale:scale,actual_scale:player.render_scale(),startup_raster:startupRaster,fixed_raster_phases:rasterPhases,scene:[player.scene_width(),player.scene_height()],raster:[player.raster_width(),player.raster_height()],profile_used:window.__engineProfile??null,capabilities:manifest?Object.keys(manifest.capabilities??{}):[],font_stats:window.__debugRuntimeStats?.()??null,font_resources:fontResources,runtime_resources:runtimeResources,device_adapters:window.__deviceAdapterInfo,pixel_assertion:{width:pixels.width,height:pixels.height,ink_pixels:ink.pixels,background_rgba:ink.background,sha256:pixelHash},wasm_memory_count:wasmMemories.size,wasm_memory_capacity_bytes:Array.from(wasmMemories).reduce((total,memory)=>total+memory.buffer.byteLength,0),heap_bytes:heapBytes,user_agent_memory_bytes:userAgentMemory?.bytes??null,ua:navigator.userAgent};
    } catch(e) { window.__failure=errorText(e); }
  </script>`;
}

function quantile(values, p) { const sorted=[...values].sort((a,b)=>a-b); return sorted[Math.min(sorted.length-1,Math.floor((sorted.length-1)*p))]; }
function summarize(values) {
  const valid = values.filter(Number.isFinite);
  const skippedIndices = values.flatMap((value,index)=>Number.isFinite(value)?[]:[index]);
  if (valid.length === 0) return { n: 0, expected_n: values.length, skipped_indices: skippedIndices, p50: null, p95: null, mean: null, raw: values };
  return { n: valid.length, expected_n: values.length, skipped_indices: skippedIndices, p50: quantile(valid,.5), p95: quantile(valid,.95), mean: valid.reduce((a,b)=>a+b,0)/valid.length, raw: values };
}

async function staticServer(stage, port) {
  const { createServer } = await import('node:http');
  const roots = { '/old/': join(stage,'old'), '/fresh/': join(stage,'fresh'), '/repo/': ROOT };
  const server = createServer((req,res)=>{
    try {
      const url = new URL(req.url, `http://localhost:${port}`); let path;
      if(url.pathname==='/old-page.html') path=join(stage,'old-page.html');
      else if(url.pathname==='/fresh-page.html') path=join(stage,'fresh-page.html');
      else { const root=Object.entries(roots).find(([prefix])=>url.pathname.startsWith(prefix)); if(!root) throw Error('not found'); path=resolve(root[1], decodeURIComponent(url.pathname.slice(root[0].length))); if(!path.startsWith(root[1]+sep)&&path!==root[1]) throw Error('invalid path'); }
      const bytes=readFileSync(path); const cache=/page\.html$|scene\.amx$/.test(path)?'no-store':'public, max-age=3600'; res.writeHead(200,{'content-type':contentType(path),'cache-control':cache,'access-control-allow-origin':'*'}); res.end(bytes);
    } catch { res.writeHead(404);res.end('not found'); }
  });
  await new Promise((ok,no)=>{server.once('error',no);server.listen(port,'127.0.0.1',ok)}); return server;
}

async function runOne(kind, order, scene, stage, chrome, port, options = {}) {
  const owned = !options.context;
  const context = options.context || (await chrome.send('Target.createBrowserContext', { disposeOnDetach: true })).browserContextId;
  try {
    const sceneLocation=sceneFile(scene).url;
    const pageUrl=new URL(`http://127.0.0.1:${port}/${kind==='old'?'old-page.html':'fresh-page.html'}`);pageUrl.searchParams.set('scene',sceneLocation);
    const target = (await chrome.send('Target.createTarget', { url: 'about:blank', browserContextId: context })).targetId;
    const { sessionId } = await chrome.send('Target.attachToTarget', { targetId: target, flatten: true });
    await chrome.send('Network.enable',{},sessionId);
    if(options.cacheDisabled) await chrome.send('Network.setCacheDisabled',{cacheDisabled:true},sessionId);
    await chrome.send('Runtime.enable',{},sessionId);
    await chrome.send('Page.enable',{},sessionId);
    await chrome.send('Page.navigate',{url:pageUrl.href},sessionId);
    await waitFor(`${kind} runtime result`,120000,async()=>{
      const response=await chrome.send('Runtime.evaluate',{expression:'window.__result || window.__failure || null',returnByValue:true},sessionId);
      return response.result?.value;
    });
    const response=await chrome.send('Runtime.evaluate',{expression:'({result:window.__result,failure:window.__failure})',returnByValue:true},sessionId);
    if(response.result.value.failure) fail(`${kind} failed: ${response.result.value.failure}`);
    const result=response.result.value.result; result.order=order; result.context=options.context?'reused':'fresh'; result.scene_path=scene;
    await chrome.send('Target.closeTarget',{targetId:target}).catch(()=>{});
    return result;
  } finally { if(owned) await chrome.send('Target.disposeBrowserContext',{browserContextId:context}).catch(()=>{}); }
}

async function measureScene(scene, stage, chrome, sitePort, staging) {
  const measurements=[];
  for(let i=0;i<WARMUPS+REPS;i++) {
    const order=i%2===0?['old','fresh']:['fresh','old'];
    const pair=[];
    for(const kind of order) pair.push(await runOne(kind,i,scene,stage,chrome,sitePort,{cacheDisabled:true}));
    if(i>=WARMUPS) measurements.push(Object.fromEntries(pair.map(row=>[row.kind,row])));
    log(`${scene}: cold pair ${i+1}/${WARMUPS+REPS} ${order.join(' then ')}`);
  }
  const warmContext=(await chrome.send('Target.createBrowserContext',{disposeOnDetach:true})).browserContextId;
  const warmSamples={old:[],fresh:[]};
  try {
    for(const kind of ['old','fresh']) await runOne(kind,'warm-prime',scene,stage,chrome,sitePort,{context:warmContext});
    for(let i=0;i<WARM_REPS;i++) {
      const order=i%2===0?['old','fresh']:['fresh','old'];
      for(const kind of order) warmSamples[kind].push(await runOne(kind,`warm-${i}`,scene,stage,chrome,sitePort,{context:warmContext}));
    }
  } finally { await chrome.send('Target.disposeBrowserContext',{browserContextId:warmContext}).catch(()=>{}); }

  const adapters=measurements.flatMap(pair=>[...pair.old.device_adapters,...pair.fresh.device_adapters]);
  if(adapters.length===0) fail(`engine-created WebGPU devices were not captured for ${scene}`);
  if(adapters.some(adapter=>!adapter.nvidia_verified)) fail(`engine used an adapter that is not verified as NVIDIA: ${JSON.stringify(adapters)}`);
  if(adapters.some(adapter=>/swiftshader|llvmpipe|software raster/i.test(`${adapter.description} ${adapter.device} ${adapter.vendor}`))) fail(`software WebGPU adapter detected: ${JSON.stringify(adapters)}`);
  const cpuFrame = (row, percentile) => quantile(row.frame_samples.flatMap(sample => sample.cpu_ms_per_frame), percentile);
  const startupComparable=measurements.every(pair=>pair.old.startup_raster.scale===pair.fresh.startup_raster.scale&&JSON.stringify(pair.old.startup_raster.raster)===JSON.stringify(pair.fresh.startup_raster.raster));
  const pairedValues = {
    cold_startup_ms: measurements.map(pair => startupComparable ? [pair.old.startup_ms, pair.fresh.startup_ms] : [null,null]),
    document_load_ms: measurements.map(pair => [pair.old.document_load_ms, pair.fresh.document_load_ms]),
    cpu_frame_p50_ms: measurements.map(pair => [cpuFrame(pair.old, .5), cpuFrame(pair.fresh, .5)]),
    cpu_frame_p95_ms: measurements.map(pair => [cpuFrame(pair.old, .95), cpuFrame(pair.fresh, .95)]),
    queue_depth_one_frame_p50_ms: measurements.map(pair => [pair.old.queue_depth_one_frame_p50_ms, pair.fresh.queue_depth_one_frame_p50_ms]),
    queue_depth_one_frame_p95_ms: measurements.map(pair => [pair.old.queue_depth_one_frame_p95_ms, pair.fresh.queue_depth_one_frame_p95_ms]),
    throughput_fps: measurements.map(pair => [pair.old.throughput_fps, pair.fresh.throughput_fps]),
    heap_bytes: measurements.map(pair => [pair.old.heap_bytes, pair.fresh.heap_bytes]),
    user_agent_memory_bytes: measurements.map(pair => [pair.old.user_agent_memory_bytes, pair.fresh.user_agent_memory_bytes]),
    wasm_memory_capacity_bytes: measurements.map(pair => [pair.old.wasm_memory_capacity_bytes, pair.fresh.wasm_memory_capacity_bytes]),
  };
  const warmStartupPairs = warmSamples.old.map((oldRow, index) => [oldRow.startup_ms, warmSamples.fresh[index].startup_ms]);
  pairedValues.warm_startup_ms = warmStartupPairs;
  const summary = {};
  const metricComparisons = {};
  for (const [metric, pairs] of Object.entries(pairedValues)) {
    const oldValues = pairs.map(pair => pair[0]);
    const freshValues = pairs.map(pair => pair[1]);
    const higherIsBetter = metric === 'throughput_fps';
    const primaryPolicy = PRIMARY_METRIC_POLICY[metric];
    const paired = comparePaired(oldValues, freshValues, {
      threshold: RELATIVE_MARGIN,
      absoluteThreshold: primaryPolicy?.absoluteThreshold ?? 0,
      higherIsBetter,
      required: Boolean(primaryPolicy),
    });
    if (!primaryPolicy) {
      paired.signal_status = paired.status;
      if (paired.status === 'regression' || paired.status === 'inconclusive') paired.status = 'diagnostic-signal';
    }
    summary[metric] = {
      role: primaryPolicy ? 'primary-user-impact' : 'diagnostic-only',
      gate_effect: primaryPolicy ? 'blocking-primary' : 'none',
      units: metric.endsWith('_ms') ? 'milliseconds' : metric.endsWith('_fps') ? 'frames-per-second' : 'bytes',
      old: summarize(oldValues),
      fresh: summarize(freshValues),
      paired,
    };
    metricComparisons[metric] = summary[metric].paired;
  }
  // These stage timings are intentionally reported without an A/B ratio: the
  // legacy load_scene phase includes module/import/asset work, while the new
  // SDK exposes preparation and its synchronous build call separately.
  summary.phase_timings = {
    legacy_load_scene_ms: summarize(measurements.map(pair => pair.old.legacy_load_scene_ms)),
    fresh_prepare_ms: summarize(measurements.map(pair => pair.fresh.prepare_ms)),
    fresh_build_ms: summarize(measurements.map(pair => pair.fresh.build_ms)),
  };
  const settingsMatch=measurements.every(pair=>pair.old.quality===pair.fresh.quality&&pair.old.requested_scale===pair.fresh.requested_scale&&pair.old.actual_scale===pair.fresh.actual_scale&&JSON.stringify(pair.old.scene)===JSON.stringify(pair.fresh.scene)&&JSON.stringify(pair.old.raster)===JSON.stringify(pair.fresh.raster));
  const pixelHashesMatch=measurements.every(pair=>pair.old.pixel_assertion.sha256===pair.fresh.pixel_assertion.sha256);
  const visualStatus=pixelHashesMatch?'pixel-identical':scene.endsWith('rich-media.amx')?'incomparable-rich-media':/(?:english|cjk)[.]amx$/i.test(scene)?'incomparable-text-rendering':'different-pixel-hash-review-required';
  const primaryComparisons = Object.keys(PRIMARY_METRIC_POLICY).map(metric => metricComparisons[metric]);
  const hardFailures = settingsMatch ? [] : ['old/new quality, scale, scene, or raster dimensions differ'];
  const primaryVerdict = combinePrimaryVerdict(primaryComparisons);
  const verdict = hardFailures.length ? 'hard-failure' : primaryVerdict;
  const diagnostics = Object.entries(metricComparisons).filter(([metric]) => !PRIMARY_METRIC_POLICY[metric]);
  const diagnosticCoverage = {
    status: diagnostics.some(([, result]) => ['diagnostic-signal', 'unverified', 'incomplete'].includes(result.status)) ? 'warning' : 'complete',
    diagnostic_metric_count: diagnostics.length,
    unverified_metrics: diagnostics.filter(([, result]) => result.status === 'unverified').map(([metric]) => metric),
    incomplete_metrics: diagnostics.filter(([, result]) => result.status === 'incomplete').map(([metric]) => metric),
    relative_or_uncertain_signals: diagnostics.filter(([, result]) => result.status === 'diagnostic-signal').map(([metric, result]) => ({ metric, signal_status: result.signal_status })),
  };
  return {
    scene,
    settings_match: settingsMatch,
    cold_startup_comparable: startupComparable,
    cold_startup_incomparability_reason: startupComparable ? null : 'old and current startup scale or raster dimensions differed before fixed-scale pinning',
    hard_failures: hardFailures,
    visual_status: visualStatus,
    pixel_hashes_match: pixelHashesMatch,
    performance_policy: PERFORMANCE_POLICY,
    verdict,
    primary_verdict: primaryVerdict,
    diagnostic_coverage: diagnosticCoverage,
    metric_comparisons: metricComparisons,
    summary,
    cold_pairs: measurements,
    warm_reuse: warmSamples,
  };
}

async function main() {
  if (!GPU) fail('WPR_GPU=1 is required; this harness does not make performance claims from software rendering');
  if (!process.env.VK_ICD_FILENAMES || !process.env.VK_ICD_FILENAMES.includes('nvidia_icd.json')) fail('set VK_ICD_FILENAMES to the host NVIDIA Vulkan ICD');
  requireFile(process.env.VK_ICD_FILENAMES, 'NVIDIA Vulkan ICD');
  if (!PROFILE && !process.env.DISPLAY) log('running headless native Vulkan');
  const staging=mkdtempSync(join(tmpdir(),'animatix-web-perf-'));
  let server, browser, chrome, chromeLog='';
  try {
    const stage=prepareSite(staging); const sitePort=await freePort(); const cdpPort=await freePort();
    server=await staticServer(stage,sitePort);
    const args=['--no-sandbox','--disable-dev-shm-usage','--disable-gpu-sandbox','--enable-features=Vulkan,WebGPUService','--ignore-gpu-blocklist','--no-first-run','--no-default-browser-check','--disable-background-networking','--disable-extensions','--use-vulkan=native',`--force-device-scale-factor=1`,`--remote-debugging-port=${cdpPort}`,`--user-data-dir=${join(staging,'profile')}`];
    if(!PROFILE) args.unshift('--headless=new','--use-gl=angle','--use-angle=vulkan');
    browser=spawn(CHROME,args,{cwd:ROOT,detached:true,stdio:['ignore','pipe','pipe']});
    browser.stderr.on('data',chunk=>{chromeLog+=chunk.toString();});
    const version=await waitFor('Chromium DevTools endpoint',20000,async()=>{try{return await (await fetch(`http://127.0.0.1:${cdpPort}/json/version`)).json()}catch{return null}});
    chrome=new Cdp(version.webSocketDebuggerUrl);await chrome.open();
    const gpuSystem=await chrome.send('SystemInfo.getInfo');
    const nvidia=gpuSystem.gpu?.devices?.some(device=>device.vendorId===0x10de||/nvidia/i.test(`${device.vendorString||''} ${device.deviceString||''} ${device.driverVendor||''}`));
    if(!nvidia) fail(`Chromium did not report the NVIDIA adapter: ${JSON.stringify(gpuSystem.gpu?.devices||[])}`);
    const scenarios={};
    for(const scene of SCENES) { scenarios[scene]=await measureScene(scene,stage,chrome,sitePort,staging); }
    const result={schema_version:4,baseline_commit:OLD_COMMIT,created_at:new Date().toISOString(),browser:run(CHROME,['--version']),gpu_icd:process.env.VK_ICD_FILENAMES,gpu_system:gpuSystem.gpu,quality:QUALITY,render_scale:SCALE,canvas:[WIDTH,HEIGHT],frames:FRAME_COUNT,cold_pairs:REPS,warm_pairs:WARM_REPS,warmup_pairs:WARMUPS,queue_depth_one_frames:DRAIN_FRAMES,performance_policy:PERFORMANCE_POLICY,scenarios,notes:['WPR_SCENE accepts one workspace-relative .amx path or a comma-separated scene list. Default scenarios cover English text, CJK fallback and combined rich-text/image capabilities.','Cold A/B pairs alternate order in fresh contexts with cache disabled; warm runs reuse one context with cache enabled. Warm startup measures a new player reaching ready in a reused context; it is not an applySource() cached-source rebuild.','Frame-cost comparisons pin the engine raster to the requested scale, disable the embed adaptive quality governor in the benchmark page, and verify exact scale and raster dimensions before and after each measured phase. The startup scale/raster is reported before pinning. This isolates equal-resolution renderer cost and does not measure adaptive playback behavior.','The primary user-impact metrics are cold player-ready startup, warm player-ready startup, and queue-depth-one p95 completion latency. Relative and paired absolute-delta intervals are both retained.','Blocking requires the paired 95% lower bounds to clear both the 5% relative margin and the metric-specific provisional absolute floor. No-material status requires either upper bound to stay within its floor; remaining overlap or incomplete required pairs is inconclusive (exit 2). These engineering floors are not validated product SLOs.','The paired Student-t intervals cover within-run pairs in one Chromium process; they are not family-wise intervals and do not estimate between-process/day variation. Six cold and three warm pairs provide limited uncertainty estimates.','CPU p50/p95, document/phase timings, batch throughput, JS heap, browser-wide memory, and Wasm capacity are diagnostic-only and never affect the primary verdict. Unsupported optional measurements are explicitly unverified.','Every engine-created WebGPU device must report NVIDIA (vendor 0x10de or NVIDIA info); the host GPU list alone is insufficient.','Each queue-depth-one sample renders one frame and awaits debug_gpu_drain; its latency includes browser/WASM promise overhead and is not pure GPU time or v-sync playback latency.','Sustained throughput is reported in frames per second from the batched frame loop plus debug_gpu_drain; it is diagnostic, not a playback SLO.','Legacy load_scene, fresh preparation, and fresh synchronous build timings are stage details, not directly comparable A/B phases.','wasm_memory_capacity_bytes is the sum of unique exported WebAssembly.Memory buffer byte lengths after rendering; it reflects linear-memory capacity, not resident memory, and excludes GPU allocations and non-exported memories.','Visual hashes are reported separately because legacy/current text rendering may legitimately differ; the hard pixel assertion rejects empty output. SDK integrity and actual-draw checks remain hard failures. The legacy frame hash is a measurement reference, not a visual golden.','heap_bytes is Chromium usedJSHeapSize at the end of the sample, not peak process memory. user_agent_memory_bytes is browser-wide when supported; this harness does not establish cross-origin isolation, so unsupported readings remain unverified.']};
    mkdirSync(dirname(OUT),{recursive:true});writeFileSync(OUT,JSON.stringify(result,null,2)+'\n');process.stdout.write(JSON.stringify(result,null,2)+'\n');
    const verdicts=Object.values(scenarios).map(scenario=>scenario.verdict);
    if(verdicts.some(verdict=>verdict==='regression'||verdict==='hard-failure')) process.exitCode=1;
    else if(verdicts.includes('inconclusive')) process.exitCode=2;
  } finally {
    chrome?.close(); if(browser) killGroup(browser); server?.close(); rmSync(staging,{recursive:true,force:true});
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error=>{process.stderr.write(`web-runtime-perf: ${error.stack||error}\n`);process.exitCode=1;});
}
