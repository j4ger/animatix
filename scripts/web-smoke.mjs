#!/usr/bin/env node
// Distinct SDK fixtures and real assembled-site journeys. No legacy page crawl.
import {spawn, execFileSync} from 'node:child_process';
import {existsSync,mkdtempSync,rmSync,mkdirSync,writeFileSync,appendFileSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import net from 'node:net';
import {Cdp,until,sleep,instrumentation,navigationCommitted} from './web-smoke-browser.mjs';
import {contracts,siteBase,withBase} from './web-smoke-contracts.mjs';
import {journey} from './web-smoke-journeys.mjs';
import {chromiumLaunch} from './web-smoke-launch.mjs';
import {visualPreflight} from './web-smoke-preflight.mjs';
const ROOT=dirname(dirname(fileURLToPath(import.meta.url)));
const SUITE=process.env.SMOKE_SUITE || 'sdk';
const WEB_ROOT=resolve(ROOT,process.env.ANIMATIX_WEB_ROOT || (SUITE==='site'?'_site':'web'));
const OUTPUT=resolve(ROOT,process.env.SMOKE_OUTPUT_DIR || `target/web-smoke/${SUITE}-${process.env.SMOKE_ATTEMPT || 1}`);
const PREFIX=siteBase(process.env.SITE_BASE);
const PAGES=contracts(SUITE,process.env.SMOKE_DEPTH || 'critical').map(contract=>withBase(contract,PREFIX));
const PAGE_BUDGET=180000;
function freePort() {
  return new Promise((resolve,reject)=>{
    const s=net.createServer();s.on('error',reject);s.listen(0,'127.0.0.1',()=>{const port=s.address().port;s.close(()=>resolve(port));});
  });
}
function kill(child) {
  if(!child || child.exitCode!==null)return;
  try{process.kill(-child.pid,'SIGKILL');}catch{child.kill('SIGKILL');}
}
async function main(){
  mkdirSync(OUTPUT,{recursive:true});
  if(!existsSync(join(WEB_ROOT,'runtime/runtime-manifest.json')))throw Error(`SDK missing in ${WEB_ROOT}; build/restore the SDK and assemble site first`);
  if(SUITE==='site' && WEB_ROOT===join(ROOT,'web'))throw Error('site suite refuses the legacy web/ source tree');
  const metadata={suite:SUITE,depth:process.env.SMOKE_DEPTH || 'critical',attempt:Number(process.env.SMOKE_ATTEMPT||1),sha:execFileSync('git',['rev-parse','HEAD'],{cwd:ROOT,encoding:'utf8'}).trim(),viewport:{width:1440,height:1000},backend:'SwiftShader (requested; adapter reported below)'};
  writeFileSync(join(OUTPUT,'runtime-manifest.json'),readFileSync(join(WEB_ROOT,'runtime/runtime-manifest.json')));
  const record=event=>appendFileSync(join(OUTPUT,'timings.jsonl'),JSON.stringify({...metadata,recordedAt:new Date().toISOString(),...event})+'\n');
  const port=await freePort(), debugPort=await freePort(),base=`http://127.0.0.1:${port}`;
  const profile=mkdtempSync(join(tmpdir(),'amx-smoke-'));
  let cdp,server,chrome,current='startup',pageErrors=[],browserLog='',consoleLog='',networkLog='';
  const watchdog=setTimeout(()=>{record({kind:'failure',category:'watchdog',page:current});kill(chrome);kill(server);process.exit(1);},PAGES.length*PAGE_BUDGET+60000);
  try{
    server=spawn(process.env.PYTHON||'python3',[join(ROOT,'scripts/serve-web.py'),String(port)],{cwd:ROOT,env:{...process.env,SERVE_ROOT:WEB_ROOT,SERVE_BASE:PREFIX},detached:true,stdio:['ignore','pipe','pipe']});
    for(const stream of [server.stdout,server.stderr])stream.on('data',data=>appendFileSync(join(OUTPUT,'server.log'),data));
    const launch=chromiumLaunch(process.env.CHROME||'chromium',profile,debugPort);
    chrome=spawn(launch.command,launch.args,{detached:true,stdio:['ignore','pipe','pipe']});
    let spawnError;
    for(const child of [server,chrome])child.on('error',error=>{spawnError=error;});
    for(const stream of [chrome.stdout,chrome.stderr])stream.on('data',data=>{browserLog+=data;appendFileSync(join(OUTPUT,'chromium.log'),data);});
    await until('server',async()=>{if(spawnError)throw spawnError;try{return (await fetch(base+PREFIX+'/',{signal:AbortSignal.timeout(2000)})).ok;}catch{return false;}},20000);
    await until('CDP',async()=>{if(spawnError)throw spawnError;try{return (await fetch(`http://127.0.0.1:${debugPort}/json/version`,{signal:AbortSignal.timeout(2000)})).ok;}catch{return false;}},30000);
    const response=await fetch(`http://127.0.0.1:${debugPort}/json/new?about:blank`,{method:'PUT',signal:AbortSignal.timeout(5000)});
    if(!response.ok)throw Error(`CDP target failed: ${response.status}`);
    cdp=new Cdp((await response.json()).webSocketDebuggerUrl);
    cdp.onEvent=event=>{
      if(event.method==='Runtime.consoleAPICalled'){
        const text=event.params.args.map(arg=>arg.value??arg.description??'').join(' ');
        consoleLog+=`[${event.params.type}] ${text}\n`;
        if(event.params.type==='error'||/amx-player: render failed/.test(text))pageErrors.push(text);
      }
      if(event.method==='Runtime.exceptionThrown')pageErrors.push(event.params.exceptionDetails.exception?.description||event.params.exceptionDetails.text);
      if(event.method==='Network.responseReceived'){
        const r=event.params.response;networkLog+=JSON.stringify({url:r.url,status:r.status,mime:r.mimeType})+'\n';
        // Chromium probes this optional conventional URL even without an authored link.
        if(r.status>=400 && new URL(r.url).pathname!=='/favicon.ico')pageErrors.push(`HTTP ${r.status}: ${r.url}`);
      }
      if(event.method==='Network.loadingFailed'&&!event.params.canceled)pageErrors.push(`Network failed: ${event.params.errorText}`);
    };
    await cdp.open();
    for(const domain of ['Page','Runtime','Network'])await cdp.send(`${domain}.enable`);
    await cdp.send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
    await cdp.send('Page.addScriptToEvaluateOnNewDocument',{source:instrumentation});
    metadata.browser=await cdp.send('Browser.getVersion');
    // No page/SDK has run yet. Only an adapter failure here is retryable (75).
    await cdp.send('Page.navigate',{url:base+PREFIX+'/runtime/runtime-manifest.json'});
    await navigationCommitted(cdp,base+PREFIX+'/runtime/runtime-manifest.json');
    await until('preflight document',()=>cdp.evaluate(`location.origin===${JSON.stringify(base)}&&document.readyState==='complete'`));
    const adapter=await cdp.evaluate(`(async()=>{if(!navigator.gpu)return null;const a=await navigator.gpu.requestAdapter();return a?{vendor:a.info?.vendor,architecture:a.info?.architecture,device:a.info?.device,description:a.info?.description}:null;})()`);
    if(!adapter){const error=Error('WebGPU adapter unavailable before suite');error.exitCode=75;throw error;}
    metadata.adapter=adapter;
    const systemResponse=await(await fetch(`http://127.0.0.1:${debugPort}/json/version`)).json();
    const system=new Cdp(systemResponse.webSocketDebuggerUrl);await system.open();
    try {metadata.gpu=(await system.send('SystemInfo.getInfo')).gpu;} finally {system.close();}
    metadata.compositorPixels=await visualPreflight(cdp,bytes=>writeFileSync(join(OUTPUT,'preflight.png'),bytes));
    record({kind:'preflight'});
    for(let n=0;n<PAGES.length;n++){
      const contract=PAGES[n], started=Date.now();current=contract.path;pageErrors=[];browserLog='';
      const checkErrors=async()=>{
        const errors=await cdp.evaluate(`window.__smoke?.errors||[]`);
        const status=await cdp.evaluate(`document.querySelector('.scene-status')?.textContent||''`);
        if(errors.length||pageErrors.length||/Could not load|Playback unavailable/.test(status))throw Error([...errors,...pageErrors,status].filter(Boolean).join('\n'));
        if(/Error while parsing WGSL|GPU validation error/.test(browserLog))throw Error('GPU validation failed (see chromium.log)');
      };
      const capture=async(index,name)=>{
        const clip=await cdp.evaluate(`(()=>{const c=document.querySelectorAll('amx-player')[${index}]?.shadowRoot?.querySelector('canvas');if(!c)throw Error('Missing player canvas');const r=c.getBoundingClientRect();if(r.width<32||r.height<32||r.x<0||r.y<0||r.bottom>innerHeight||r.right>innerWidth)throw Error('Canvas not fully visible');return {x:r.x+scrollX,y:r.y+scrollY,width:r.width,height:r.height,scale:1};})()`);
        const {data}=await cdp.send('Page.captureScreenshot',{format:'png',clip,captureBeyondViewport:false});
        const buffer=Buffer.from(data,'base64');writeFileSync(join(OUTPUT,`${n}-${name}.png`),buffer);return buffer;
      };
      let timer;
      try{
        await Promise.race([(async()=>{
          await cdp.send('Page.navigate',{url:base+contract.path});
          await navigationCommitted(cdp,base+contract.path);
          await journey(cdp,contract,capture,event=>record({page:current,...event}),checkErrors);
        })(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(`Host page deadline (${PAGE_BUDGET}ms)`)),PAGE_BUDGET);})]);
      }finally{clearTimeout(timer);}
      record({kind:'page-complete',page:current,durationMs:Date.now()-started,cache:n===0?'browser-first-journey':'browser-reused'});
      console.log(`web-smoke ${SUITE}: ${current} ok (${Date.now()-started}ms)`);
    }
  }catch(error){
    record({kind:'failure',page:current,message:error.message,retryable:error.exitCode===75});
    if(cdp)try{const shot=await cdp.send('Page.captureScreenshot',{format:'png'},3000);writeFileSync(join(OUTPUT,'failure.png'),Buffer.from(shot.data,'base64'));}catch{/* transport may have failed */}
    throw error;
  }finally{
    clearTimeout(watchdog);writeFileSync(join(OUTPUT,'console.log'),consoleLog);writeFileSync(join(OUTPUT,'network.jsonl'),networkLog);
    cdp?.close();kill(chrome);kill(server);await sleep(100);rmSync(profile,{recursive:true,force:true});
  }
}
main().catch(error=>{console.error(`web-smoke: FAILED: ${error.message}`);process.exitCode=error.exitCode||1;});
