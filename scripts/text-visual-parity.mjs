#!/usr/bin/env node
// Compare the same single-scene source and exact ordered font bytes through
// native OffscreenRenderer and the final SDK's direct Wasm player API.
// The glyph-probe CLI remains separate; this checks real raster output.
//
// Usage (after building the current final SDK, inside nix develop, on idle NVIDIA Vulkan):
//   DISPLAY=:0 VK_ICD_FILENAMES=/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json \
//     WPR_GPU=1 node scripts/text-visual-parity.mjs
//   TVP_SCENES=tools/animatix-web-build/fixtures/text-parity/counter.amx \
//     TVP_SDK=web/runtime WPR_GPU=1 node scripts/text-visual-parity.mjs
//
// No performance measurements are made here. The SDK must be current and
// release-verified. Outputs are raw RGBA comparisons plus PNGs encoded from
// those raw bytes; Canvas serialization is deliberately not used.

import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { deflateSync, gunzipSync } from 'node:zlib';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';
import { verify as verifySdk } from '../web/tools/sdk.mjs';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SDK_DIR = resolve(ROOT, process.env.TVP_SDK || 'web/runtime');
const ARABIC_FONT = join(ROOT, 'tools/animatix-web-build/fixtures/text-parity/fonts/NotoNaskhArabicUI.ttf');
const ARABIC_SHA = '4713198071a99afad0a596df8ab3c71b1b59da092ad4f3c27c439949a7118fc8';
const DEFAULT_SCENES = [
  'tools/animatix-web-build/fixtures/text-parity/equation.amx',
  'tools/animatix-web-build/fixtures/text-parity/captions.amx',
  'tools/animatix-web-build/fixtures/text-parity/counter.amx',
  'testfixtures/web-runtime-perf/plain-cjk-bidi-weights.amx',
];
const SCENES = (process.env.TVP_SCENES || DEFAULT_SCENES.join(',')).split(',').map(value => value.trim()).filter(Boolean);
const TIMES = [0.25, 1.25, 1.9];
const WIDTH = 960;
const HEIGHT = 540;
const CHROME = process.env.CHROME || 'chromium';
const GPU = process.env.WPR_GPU === '1';
const HEADED = process.env.TVP_HEADED !== '0';
const OUT = resolve(ROOT, process.env.TVP_OUT || 'target/perf/text-visual-parity');
const THRESHOLDS = { rgb_channel_mae: 2.0, alpha_mae: 1.0, pixels_over_16_percent: 1.0, pixels_over_64_percent: 0.1 };
const sleep = ms => new Promise(resolveSleep => setTimeout(resolveSleep, ms));

function fail(message) { throw new Error(message); }
function log(message) { process.stderr.write(`text-visual-parity: ${message}\n`); }
function requireFile(path, label) { if (!existsSync(path)) fail(`${label} missing: ${path}`); }
function run(command, args, options = {}) {
  return execFileSync(command, args, { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options }).trim();
}
function sha256(bytes) { return createHash('sha256').update(bytes).digest('hex'); }
function scenePath(scene) {
  const path = resolve(ROOT, scene);
  if (!path.startsWith(ROOT + sep)) fail(`scene must be inside the workspace: ${scene}`);
  requireFile(path, `scene ${scene}`);
  return path;
}
function freePort() {
  return new Promise((resolvePort, reject) => {
    const server = net.createServer(); server.unref(); server.once('error', reject);
    server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => resolvePort(port)); });
  });
}
async function waitFor(label, ms, probe) {
  const deadline = Date.now() + ms; let last;
  while (Date.now() < deadline) {
    try { const value = await probe(); if (value) return value; } catch (error) { last = error; }
    await sleep(150);
  }
  fail(`timed out waiting for ${label}${last ? `: ${last.message}` : ''}`);
}

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
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP ${method} timeout`)); }, 20000);
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

function loadExactFonts(manifest) {
  const fontFiles = manifest.fonts?.faces;
  if (!Array.isArray(fontFiles) || !fontFiles.length) fail('SDK manifest has no font faces');
  const fonts = fontFiles.map((face, index) => {
    const packed = readFileSync(join(SDK_DIR, face.url));
    const bytes = packed[0] === 0x1f && packed[1] === 0x8b ? gunzipSync(packed) : packed;
    if (sha256(bytes) !== face.sha256 || bytes.length !== face.size_bytes) fail(`SDK font hash/size mismatch: ${face.file}`);
    return { name: face.file, bytes, sha256: face.sha256, index };
  });
  const arabic = readFileSync(ARABIC_FONT);
  if (sha256(arabic) !== ARABIC_SHA) fail('Arabic fixture font hash mismatch');
  fonts.push({ name: 'NotoNaskhArabicUI.ttf', bytes: arabic, sha256: ARABIC_SHA, index: fonts.length });
  return fonts;
}

export function page(fontCount) {
  return `<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:#111}canvas{display:block;width:${WIDTH}px;height:${HEIGHT}px}</style><canvas id="stage" width="${WIDTH}" height="${HEIGHT}"></canvas><script type="module">
    window.__result=null; window.__failure=null; window.__done=false;
    const times=${JSON.stringify(TIMES)};
    const errorText=e=>String(e?.stack||e?.message||e);
    function bytesToB64(bytes){let chunks=[];for(let i=0;i<bytes.length;i+=0x8000)chunks.push(String.fromCharCode(...bytes.subarray(i,i+0x8000)));return btoa(chunks.join(''))}
    function inkCount(bytes,width,height){const points=[[0,0],[width-1,0],[0,height-1],[width-1,height-1]],channels=[0,1,2,3],samples=points.map(([x,y])=>channels.map(c=>bytes[(y*width+x)*4+c])),background=channels.map(c=>samples.map(p=>p[c]).sort((a,b)=>a-b)[2]);let count=0;for(let i=0;i<bytes.length;i+=4){const rgb=Math.abs(bytes[i]-background[0])+Math.abs(bytes[i+1]-background[1])+Math.abs(bytes[i+2]-background[2]);if(rgb>36||Math.abs(bytes[i+3]-background[3])>16)count++}return{pixels:count,background}}
    async function readFrame(player,time){return await new Promise((resolve,reject)=>{let settled=false;const timer=setTimeout(()=>{if(!settled)reject(Error('RGBA readback timed out'))},20000);player.debug_readback_rgba(time,value=>{settled=true;clearTimeout(timer);if(typeof value==='string'){try{const parsed=JSON.parse(value);if(parsed.error)throw Error(parsed.error)}catch(error){reject(error);return}reject(Error(value));return}resolve({time,width:value.width,height:value.height,rgba_b64:bytesToB64(value.bytes)})})})}
    async function main(){
      const scene=new URLSearchParams(location.search).get('scene');if(!scene)throw Error('scene path is required');
      const manifestUrl=new URL('/runtime/runtime-manifest.json',location.origin);
      const manifestResponse=await fetch(manifestUrl);if(!manifestResponse.ok)throw Error('manifest HTTP '+manifestResponse.status);
      const manifest=await manifestResponse.json();
      const sceneUrl=new URL('/repo/'+scene,location.origin);const sourceResponse=await fetch(sceneUrl);if(!sourceResponse.ok)throw Error('scene HTTP '+sourceResponse.status);const source=await sourceResponse.text();
      const entryUrl=new URL(manifest.entry,manifestUrl);await import(entryUrl.href);
      const engine=globalThis[Symbol.for('animatix.sdk.engine.'+manifest.build_id)];if(!engine)throw Error('SDK entry did not register its engine API');
      const wasmResponse=await fetch(new URL(manifest.wasm,manifestUrl));if(!wasmResponse.ok)throw Error('wasm HTTP '+wasmResponse.status);
      const wasm=await WebAssembly.compile(await wasmResponse.arrayBuffer());engine.initSync({module:wasm});
      const fontResponses=await Promise.all(Array.from({length:${fontCount}},(_,index)=>fetch('/font/'+index).then(r=>{if(!r.ok)throw Error('font '+index+' HTTP '+r.status);return r.arrayBuffer()})));
      const fonts=fontResponses.map(bytes=>new Uint8Array(bytes));
      const prepared=engine.prepare_source(source,{}, {},sceneUrl.href);const analysis=prepared.analysis();
      if(!analysis.ok)throw Error('source analysis failed: '+JSON.stringify(analysis.diagnostics));
      if(analysis.missing_imports?.length)throw Error('visual fixtures may not have unresolved imports: '+analysis.missing_imports.join(', '));
      await engine.ensure_capabilities(Number(analysis.required_features??0)>>>0,manifest.files);
      await engine.init_engine();
      const canvas=document.getElementById('stage');const player=await engine.create_player(canvas);player.set_render_scale(1);
      const built=prepared.build(player,[],[],fonts,'production');if(!built.ok)throw Error('build failed: '+JSON.stringify(built.diagnostics));
      if(player.scene_width()!==${WIDTH}||player.scene_height()!==${HEIGHT})throw Error('scene resolution mismatch: '+player.scene_width()+'x'+player.scene_height());
      const frames=[];for(const time of times){const frame=await readFrame(player,time);const ink=inkCount(Uint8Array.from(atob(frame.rgba_b64),c=>c.charCodeAt(0)),frame.width,frame.height);if(ink.pixels<40)throw Error('frame contains no visible text/glyph ink at '+time+'s');frame.ink_pixels=ink.pixels;frame.background_rgba=ink.background;frames.push(frame)}
      const uniqueTimes=[...new Set(frames.map(frame=>frame.time))];if(uniqueTimes.length!==times.length)throw Error('sample times collapsed');
      return {scene,build_id:manifest.build_id,required_features:Number(analysis.required_features??0)>>>0,font_count:fonts.length,frames};
    }
    main().then(result=>{window.__result=result;window.__done=true},error=>{window.__failure=errorText(error);window.__done=true});
  </script>`;
}

async function startServer(port, fontBuffers, html) {
  const server = createServer((request, response) => {
    try {
      const url = new URL(request.url, `http://127.0.0.1:${port}`);
      let path; let bytes;
      if (url.pathname === '/harness.html') { bytes = Buffer.from(html); path = url.pathname; }
      else if (url.pathname.startsWith('/font/')) {
        const index = Number(url.pathname.slice('/font/'.length));
        if (!Number.isInteger(index) || index < 0 || index >= fontBuffers.length) throw Error('font index out of bounds');
        bytes = fontBuffers[index]; path = fontBuffers[index].name;
      } else {
        const roots = [['/runtime/', SDK_DIR], ['/repo/', ROOT]];
        const root = roots.find(([prefix]) => url.pathname.startsWith(prefix)); if (!root) throw Error('not found');
        path = resolve(root[1], decodeURIComponent(url.pathname.slice(root[0].length)));
        if (!path.startsWith(root[1] + sep)) throw Error('path escapes static root');
        bytes = readFileSync(path);
      }
      const mime = typeof path === 'string' && path.endsWith('.wasm') ? 'application/wasm'
        : typeof path === 'string' && (path.endsWith('.js') || path.endsWith('.mjs')) ? 'text/javascript'
          : typeof path === 'string' && path.endsWith('.json') ? 'application/json'
            : typeof path === 'string' && path.endsWith('.amx') ? 'text/plain'
              : typeof path === 'string' && path.endsWith('.png') ? 'image/png'
                : typeof path === 'string' && path.endsWith('.html') ? 'text/html; charset=utf-8'
                  : 'application/octet-stream';
      response.writeHead(200, { 'content-type': mime, 'cache-control': 'public, max-age=3600', 'access-control-allow-origin': '*' }); response.end(bytes);
    } catch { response.writeHead(404); response.end('not found'); }
  });
  await new Promise((ok, no) => { server.once('error', no); server.listen(port, '127.0.0.1', ok); });
  return server;
}

async function browserFrames(chrome, port, scene) {
  const context=(await chrome.send('Target.createBrowserContext',{disposeOnDetach:true})).browserContextId;
  try {
    const target=(await chrome.send('Target.createTarget',{url:'about:blank',browserContextId:context})).targetId;
    const {sessionId}=await chrome.send('Target.attachToTarget',{targetId:target,flatten:true});
    await chrome.send('Runtime.enable',{},sessionId);await chrome.send('Page.enable',{},sessionId);
    const sceneArg=relative(ROOT,scenePath(scene)).split(sep).map(encodeURIComponent).join('/');
    await chrome.send('Page.navigate',{url:`http://127.0.0.1:${port}/harness.html?scene=${encodeURIComponent(sceneArg)}`},sessionId);
    const status=await waitFor(`${scene} Wasm frames`,180000,async()=>{
      const value=await chrome.send('Runtime.evaluate',{expression:'({done:window.__done,failure:window.__failure})',returnByValue:true},sessionId);
      return value.result?.value?.done?value.result.value:null;
    });
    if(status.failure)fail(`browser render failed for ${scene}: ${status.failure}`);
    const response=await chrome.send('Runtime.evaluate',{expression:'({required_features:window.__result.required_features,font_count:window.__result.font_count,frames:window.__result.frames.map(frame=>({time:frame.time,width:frame.width,height:frame.height,ink_pixels:frame.ink_pixels,background_rgba:frame.background_rgba,base64_length:frame.rgba_b64.length}))})',returnByValue:true},sessionId);
    if(response.exceptionDetails)fail(`browser frame metadata failed for ${scene}: ${response.exceptionDetails.text}`);
    const result=response.result?.value;
    if(!result||!Array.isArray(result.frames))fail(`browser returned invalid frame metadata for ${scene}`);
    for(let index=0;index<result.frames.length;index++){
      const frame=result.frames[index];
      if(!Number.isInteger(frame.width)||frame.width<=0||!Number.isInteger(frame.height)||frame.height<=0)fail(`browser returned invalid dimensions for ${scene} frame ${index}`);
      const expectedBytes=frame.width*frame.height*4;
      const expectedBase64Length=Math.ceil(expectedBytes/3)*4;
      if(!Number.isSafeInteger(frame.base64_length)||frame.base64_length!==expectedBase64Length)fail(`browser returned invalid RGBA length for ${scene} frame ${index}`);
      let base64='';
      for(let offset=0;offset<frame.base64_length;offset+=65536){
        const end=Math.min(offset+65536,frame.base64_length);
        const chunk=await chrome.send('Runtime.evaluate',{expression:`window.__result.frames[${index}].rgba_b64.slice(${offset},${end})`,returnByValue:true},sessionId);
        if(chunk.exceptionDetails)fail(`browser RGBA chunk failed for ${scene} frame ${index}: ${chunk.exceptionDetails.text}`);
        const value=chunk.result?.value;
        if(typeof value!=='string'||value.length!==end-offset)fail(`browser returned an invalid RGBA chunk for ${scene} frame ${index} at ${offset}`);
        base64+=value;
      }
      if(base64.length!==frame.base64_length||!/^[A-Za-z0-9+/]*={0,2}$/.test(base64))fail(`browser returned malformed RGBA base64 for ${scene} frame ${index}`);
      const rgba=Buffer.from(base64,'base64');
      if(rgba.length!==expectedBytes)fail(`browser RGBA byte count mismatch for ${scene} frame ${index}: ${rgba.length} != ${expectedBytes}`);
      frame.rgba=rgba;
      delete frame.base64_length;
    }
    await chrome.send('Target.closeTarget',{targetId:target}).catch(()=>{});
    return {required_features:result.required_features,font_count:result.font_count,frames:result.frames};
  } finally { await chrome.send('Target.disposeBrowserContext',{browserContextId:context}).catch(()=>{}); }
}

function crc32(buffer) {
  let crc=0xffffffff;
  for(const byte of buffer){crc^=byte;for(let bit=0;bit<8;bit++)crc=(crc&1)?(0xedb88320^(crc>>>1)):(crc>>>1)}
  return (crc^0xffffffff)>>>0;
}
function pngChunk(type,data) {
  const kind=Buffer.from(type);const length=Buffer.alloc(4);length.writeUInt32BE(data.length);
  const checksum=Buffer.alloc(4);checksum.writeUInt32BE(crc32(Buffer.concat([kind,data])));
  return Buffer.concat([length,kind,data,checksum]);
}
function encodePng(rgba,width,height) {
  if(rgba.length!==width*height*4)fail(`RGBA length mismatch for ${width}x${height}`);
  const stride=width*4;const scanlines=Buffer.alloc((stride+1)*height);
  for(let y=0;y<height;y++){scanlines[y*(stride+1)]=0;rgba.copy(scanlines,y*(stride+1)+1,y*stride,(y+1)*stride)}
  const header=Buffer.alloc(13);header.writeUInt32BE(width,0);header.writeUInt32BE(height,4);header[8]=8;header[9]=6;header[10]=0;header[11]=0;header[12]=0;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),pngChunk('IHDR',header),pngChunk('IDAT',deflate(scanlines)),pngChunk('IEND',Buffer.alloc(0))]);
}
function deflate(bytes) { return deflateSync(bytes,{level:6}); }

function compare(native, web) {
  if(native.width!==web.width||native.height!==web.height)fail(`native/web dimensions differ: ${native.width}x${native.height} vs ${web.width}x${web.height}`);
  const a=native.rgba,b=web.rgba;if(a.length!==b.length)fail(`RGBA buffer sizes differ: ${a.length} vs ${b.length}`);
  let sumRgb=0,sumAlpha=0,over16=0,over64=0,unequal=0,max=0;
  const histogram=new Uint32Array(256);
  const diff=Buffer.alloc(a.length);
  for(let pixel=0;pixel<a.length/4;pixel++){
    let pixelMax=0;
    for(let channel=0;channel<4;channel++){
      const index=pixel*4+channel;const delta=Math.abs(a[index]-b[index]);if(channel===3)sumAlpha+=delta;else sumRgb+=delta;histogram[delta]++;pixelMax=Math.max(pixelMax,delta);max=Math.max(max,delta);
      diff[index]=channel===3?255:Math.min(255,delta*4);
    }
    if(pixelMax>0)unequal++;if(pixelMax>16)over16++;if(pixelMax>64)over64++;
  }
  const pixels=a.length/4;const rgbMae=sumRgb/(pixels*3),alphaMae=sumAlpha/pixels;
  const percentile=p=>{const target=Math.ceil(a.length*p);let total=0;for(let value=0;value<histogram.length;value++){total+=histogram[value];if(total>=target)return value}return 255};
  const metrics={width:native.width,height:native.height,pixel_count:pixels,rgb_channel_mae:rgbMae,alpha_mae:alphaMae,p95_channel_delta:percentile(.95),p99_channel_delta:percentile(.99),max_channel_delta:max,unequal_pixels_percent:unequal/pixels*100,pixels_over_16_percent:over16/pixels*100,pixels_over_64_percent:over64/pixels*100};
  const status=rgbMae<=THRESHOLDS.rgb_channel_mae&&alphaMae<=THRESHOLDS.alpha_mae&&metrics.pixels_over_16_percent<=THRESHOLDS.pixels_over_16_percent&&metrics.pixels_over_64_percent<=THRESHOLDS.pixels_over_64_percent?'within-antialias-threshold':'over-threshold-review';
  return {metrics,status,diff};
}

function inkCount(bytes,width,height) {
  const points=[[0,0],[width-1,0],[0,height-1],[width-1,height-1]];
  const samples=points.map(([x,y])=>Array.from(bytes.subarray((y*width+x)*4,(y*width+x)*4+4)));
  const background=[0,1,2,3].map(channel=>samples.map(pixel=>pixel[channel]).sort((a,b)=>a-b)[2]);
  let pixels=0;
  for(let offset=0;offset<bytes.length;offset+=4){
    const rgb=Math.abs(bytes[offset]-background[0])+Math.abs(bytes[offset+1]-background[1])+Math.abs(bytes[offset+2]-background[2]);
    if(rgb>36||Math.abs(bytes[offset+3]-background[3])>16)pixels++;
  }
  return {pixels,background};
}

function runNative(binary, scene, time, fonts) {
  const child=spawnSync(binary,['--render',scene,String(time),...fonts],{cwd:ROOT,encoding:'utf8',timeout:120000,maxBuffer:32*1024*1024});
  if(child.error)throw child.error;
  if(child.status!==0)fail(`native render failed (${scene} @ ${time}s):\n${child.stderr}`);
  const output=JSON.parse(child.stdout);
  if(!output.native_adapter||output.native_adapter.vendor!==0x10de)fail(`native renderer did not report NVIDIA: ${JSON.stringify(output.native_adapter||null)}`);
  return {...output,rgba:Buffer.from(output.rgba_base64,'base64')};
}

async function main() {
  if(!GPU)fail('WPR_GPU=1 is required; visual parity uses a real GPU on both native and browser paths');
  if(!process.env.VK_ICD_FILENAMES||!process.env.VK_ICD_FILENAMES.includes('nvidia_icd.json'))fail('set VK_ICD_FILENAMES to the host NVIDIA Vulkan ICD');
  requireFile(process.env.VK_ICD_FILENAMES,'NVIDIA Vulkan ICD');
  const manifest=verifySdk(SDK_DIR,{release:true,current:true,commit:run('git',['rev-parse','HEAD'])});
  const fonts=loadExactFonts(manifest);
  for(const scene of SCENES)scenePath(scene);
  const scratch=mkdtempSync(join(tmpdir(),'animatix-text-visual-parity-'));
  let server,browser,chrome;
  try {
    const outputFonts=fonts.map((font,index)=>{const path=join(scratch,`${String(index).padStart(2,'0')}-${font.name}`);writeFileSync(path,font.bytes);return path});
    const cargo=spawnSync('cargo',['build','--locked','-p','animatix-text-parity'],{cwd:ROOT,encoding:'utf8',timeout:900000,maxBuffer:64*1024*1024});
    if(cargo.error)throw cargo.error;
    if(cargo.status!==0)fail(`native visual renderer build failed:\n${cargo.stderr}`);
    const binary=resolve(ROOT,process.env.CARGO_TARGET_DIR||'target','debug','animatix-text-parity');requireFile(binary,'native parity renderer');
    const port=await freePort();server=await startServer(port,fonts.map(font=>font.bytes),page(fonts.length));
    const cdpPort=await freePort();const profile=mkdtempSync(join(scratch,'chromium-profile-'));
    const args=['--no-sandbox','--disable-dev-shm-usage','--disable-gpu-sandbox','--enable-features=Vulkan,WebGPUService','--ignore-gpu-blocklist','--no-first-run','--no-default-browser-check','--disable-background-networking','--disable-extensions','--use-vulkan=native','--force-device-scale-factor=1',`--remote-debugging-port=${cdpPort}`,`--user-data-dir=${profile}`];
    if(!HEADED)args.unshift('--headless=new','--use-gl=angle','--use-angle=vulkan');
    browser=spawn(CHROME,args,{cwd:ROOT,detached:true,stdio:['ignore','ignore','pipe']});
    const version=await waitFor('Chromium DevTools endpoint',20000,async()=>{try{return await(await fetch(`http://127.0.0.1:${cdpPort}/json/version`)).json()}catch{return null}});
    chrome=new Cdp(version.webSocketDebuggerUrl);await chrome.open();
    const gpu=await chrome.send('SystemInfo.getInfo');
    const nvidia=gpu.gpu?.devices?.some(device=>device.vendorId===0x10de||/nvidia/i.test(`${device.vendorString||''} ${device.deviceString||''} ${device.driverVendor||''}`));
    if(!nvidia)fail(`browser did not report NVIDIA: ${JSON.stringify(gpu.gpu?.devices||[])}`);
    const results=[];let nativeAdapter=null;
    for(const scene of SCENES){
      const webResult=await browserFrames(chrome,port,scene);const web=webResult.frames;
      const scenario=[];
      const nativeHashes=[];
      for(const time of TIMES){
        const native=runNative(binary,scene,time,outputFonts);
        nativeAdapter??=native.native_adapter;
        if(native.native_adapter.vendor!==nativeAdapter.vendor||native.native_adapter.name!==nativeAdapter.name)fail(`native adapter changed during visual parity: ${JSON.stringify(native.native_adapter)}`);
        nativeHashes.push(sha256(native.rgba));
        const webFrame=web.find(frame=>frame.time===time);if(!webFrame)fail(`browser omitted ${time}s for ${scene}`);
        if(native.width!==WIDTH||native.height!==HEIGHT)fail(`native dimensions ${native.width}x${native.height} for ${scene}`);
        const nativeInk=inkCount(native.rgba,native.width,native.height);const nativeVisiblePixels=nativeInk.pixels;
        if(nativeVisiblePixels<40)fail(`native frame is empty/hidden at ${scene} @ ${time}s (${nativeVisiblePixels} pixels)`);
        const webInk=inkCount(webFrame.rgba,webFrame.width,webFrame.height);const visiblePixels=webInk.pixels;
        if(visiblePixels<40)fail(`web frame is empty/hidden at ${scene} @ ${time}s (${visiblePixels} pixels)`);
        const comparison=compare(native,webFrame);
        const key=`${scene.replaceAll('/','_').replaceAll('.amx','')}-${time.toFixed(2)}`;
        mkdirSync(OUT,{recursive:true});
        writeFileSync(join(OUT,`${key}-native.png`),encodePng(native.rgba,native.width,native.height));
        writeFileSync(join(OUT,`${key}-web.png`),encodePng(webFrame.rgba,webFrame.width,webFrame.height));
        writeFileSync(join(OUT,`${key}-diff.png`),encodePng(comparison.diff,native.width,native.height));
        scenario.push({time_s:time,width:native.width,height:native.height,native_ink_pixels:nativeVisiblePixels,web_ink_pixels:visiblePixels,native_background_rgba:nativeInk.background,web_background_rgba:webInk.background,...comparison.metrics,status:comparison.status,files:{native:`${key}-native.png`,web:`${key}-web.png`,diff:`${key}-diff.png`}});
      }
      if(/(?:counter|captions)[.]amx$/i.test(scene)&&(new Set(web.map(frame=>sha256(frame.rgba))).size<2||new Set(nativeHashes).size<2))fail(`expected native and web Counter/Captions frames to vary over time for ${scene}`);
      results.push({scene,required_features:webResult.required_features,font_count:fonts.length,frames:scenario});
      log(`rendered ${scene}`);
    }
    const report={schema_version:1,created_at:new Date().toISOString(),build_id:manifest.build_id,source_commit:manifest.build.source_commit,browser:run(CHROME,['--version']),gpu_devices:gpu.gpu.devices,native_adapter:nativeAdapter,font_set_sha256:sha256(Buffer.from(fonts.map(font=>`${font.index}:${font.name}:${font.sha256}`).join('\n'))),font_faces:fonts.map(font=>({index:font.index,name:font.name,sha256:font.sha256,bytes:font.bytes.length})),times_s:TIMES,quality:'production',render_scale:1,thresholds:THRESHOLDS,visual_pass:results.every(result=>result.frames.every(frame=>frame.status==='within-antialias-threshold')),results};
    writeFileSync(join(OUT,'report.json'),JSON.stringify(report,null,2)+'\n');process.stdout.write(JSON.stringify(report,null,2)+'\n');
    if(!report.visual_pass)process.exitCode=1;
  } finally {
    chrome?.close();if(browser)killGroup(browser);server?.close();rmSync(scratch,{recursive:true,force:true});
  }
}

if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))main().catch(error=>{process.stderr.write(`text-visual-parity: ${error.stack||error}\n`);process.exitCode=1});
