import {until} from './web-smoke-browser.mjs';
import {assertPaint} from './web-smoke-pixels.mjs';
const players = `document.querySelectorAll('amx-player')`;
export async function journey(cdp, contract, capture, record, checkErrors) {
  const diagnose = async label => {
    if(process.env.SMOKE_DIAGNOSTICS==='1') await cdp.evaluate(`window.__smokeDiagnostics?.sample(${JSON.stringify(label)})`);
  };
  await until('page complete', () => cdp.evaluate(`document.readyState==='complete'`));
  if (!await cdp.evaluate(`!!document.querySelector('body')`)) throw new Error('Missing document body');
  if (['static', 'load', 'recording'].includes(contract.action)) {
    const cold = await cdp.evaluate(`({
      players: ${players}.length,
      requests: performance.getEntriesByType('resource').filter(entry =>
        ['.wasm', '.wasm.gz', '.mp4', '/runtime/amx-player.js'].some(suffix =>
          new URL(entry.name).pathname.endsWith(suffix))).length,
    })`);
    if (cold.players || cold.requests) throw new Error('Public page loaded the runtime or a recording before user interaction');
  }
  for (const path of contract.links || []) {
    const found = await cdp.evaluate(`Array.from(document.querySelectorAll('a[href]')).some(a => {
      const destination = new URL(a.href);
      return destination.pathname + destination.hash === ${JSON.stringify(path)};
    })`);
    if (!found) throw new Error(`Missing navigation link ${path}`);
  }
  if (contract.action === 'static') {
    await checkErrors();
    return;
  }
  record({kind:'action',action:contract.action,pageTimeMs:await cdp.evaluate('performance.now()')});
  if (contract.backend === 'webgl2' || contract.action === 'recording') {
    await cdp.evaluate(`Object.defineProperty(navigator, 'gpu', {value: undefined}); true`);
  }
  if (contract.action === 'recording') {
    await recordingJourney(cdp, record);
    return;
  }
  if (contract.action === 'load') await startPoster(cdp, Boolean(contract.recording));
  await until('expected players', async () => {
    await checkErrors();
    return cdp.evaluate(`${players}.length===${contract.players}`);
  });
  for (let index = 0; index < contract.players; index++) {
    await cdp.evaluate(`${players}[${index}].scrollIntoView({block:'center',behavior:'instant'})`);
    await until(`player ${index} ready`, async () => {
      await checkErrors();
      return cdp.evaluate(`__smoke.ready.has(${players}[${index}])`);
    });
    if (contract.action === 'edit') {
      await cdp.evaluate(`(() => {
        const code = document.querySelector('#source-code');
        const source = code.value.replace('hex_color("65d9e8")', 'hex_color("f46ea0")').replace('duration: 3', 'duration: 4');
        if (source === code.value) throw Error('Unexpected first-move source');
        code.value = source;
        code.dispatchEvent(new Event('input', {bubbles: true}));
      })()`);
      await until('live editor build', async () => {
        await checkErrors();
        return cdp.evaluate(`${players}[${index}].duration === 4`);
      });
    }
    await diagnose('before-pause');
    const info = await cdp.evaluate(`(()=>{const p=${players}[${index}];p.pause();return {src:p.getAttribute('src'),duration:p.duration,backend:p.backend,report:p.debugReport?.()};})()`);
    if (!(info.duration > 0)) throw new Error('Invalid public duration');
    if (contract.backend && info.backend !== contract.backend) throw Error(`Expected ${contract.backend}, got ${info.backend}`);
    if (contract.action === 'load') {
      await until('poster replaced by the ready player', () => cdp.evaluate(`
        document.querySelector('.scene-placeholder').hidden &&
        !document.querySelector('.scene-surface').hasAttribute('aria-busy') &&
        (document.querySelector('.scene-fallback')?.hidden ?? true) &&
        document.activeElement === document.querySelector('.scene-mount')
      `));
    }
    const samples = contract.motion ? [0, 2.1] : [info.duration * 0.65];
    for (const time of samples) {
      await diagnose(`before-seek-${time}`);
      await cdp.evaluate(`${players}[${index}].seek(${time})`);
      await diagnose(`after-seek-${time}`);
      let stats, last;
      await until(`paint at ${time}s`, async () => {
        await checkErrors();
        await diagnose(`paint-poll-${time}`);
        try {
          const png = await capture(index, `${index}-${time}`);
          stats = assertPaint(png, contract.motion ? {color:contract.action==='edit'?'#F46EA0':'#65D9E8',x:(time===0?360:920)/1280,y:0.5,tolerance:0.08} : (contract.paint || {}));
          return true;
        } catch (error) { last = error; return false; }
      }, 10000).catch(error => { throw new Error(`${error.message}: ${last?.message}`); });
      record({kind:'paint',index,time,pageTimeMs:await cdp.evaluate('performance.now()'),...info,stats});
    }
  }
  for (const module of contract.modules || []) {
    const count = await cdp.evaluate(`performance.getEntriesByType('resource').filter(entry=>new URL(entry.name).pathname.endsWith('/dev/smoke/imports/${module}')).length`);
    if (count !== 1) throw new Error(`Expected exactly one on-demand fetch for ${module}, got ${count}`);
  }
  if (contract.preset) await presetJourney(cdp, capture, checkErrors, record);
  if (contract.action === 'edit') {
    await cdp.evaluate(`(()=>{const code=document.querySelector('#source-code');code.value+='\\nthis is deliberately invalid ???';window.__submitted=code.value;code.dispatchEvent(new Event('input',{bubbles:true}));})()`);
    await until('visible build diagnostic', () => cdp.evaluate(`document.querySelector('#editor-status').textContent.startsWith('Not built:')`));
    if (!await cdp.evaluate(`document.querySelector('#source-code').value===window.__submitted`)) throw new Error('Failed build changed source');
    // A failed edit must retain the last successful scene, not merely its DOM.
    await cdp.evaluate(`${players}[0].pause();${players}[0].seek(2.1)`);
    assertPaint(await capture(0,'retained'), {color:'#F46EA0',x:920/1280,y:0.5,tolerance:0.08});
  }
  const resources = await cdp.evaluate(`performance.getEntriesByType('resource').map(x=>({name:new URL(x.name).pathname,startMs:x.startTime,durationMs:x.duration,bytes:x.transferSize}))`);
  if (resources.some(x=>x.name.endsWith('.mp4'))) throw Error('Live playback fetched a recording');
  const fonts = resources.filter(x=>/\/fonts\/.*\.gz$/.test(x.name));
  if (contract.fonts !== undefined && Boolean(fonts.length) !== contract.fonts) throw new Error(`Font contract violated: expected ${contract.fonts}, requests ${fonts.length}`);
  const events = await cdp.evaluate(`__smoke.events`);
  record({kind:'resources',resources,events});
  await checkErrors();
}

async function startPoster(cdp, hasRecording) {
  await until('rendered poster image', () => cdp.evaluate(`document.querySelector('.scene-poster img')?.naturalWidth === 1280`));
  await cdp.evaluate(`(() => {
    const poster = document.querySelector('.scene-placeholder');
    const action = poster.querySelector('.load-scene');
    const fallback = document.querySelector('.scene-fallback');
    if (poster.hidden || !action.getClientRects().length) throw Error('Missing visible Play live poster');
    if (Boolean(fallback) !== ${hasRecording} || (fallback && !fallback.hidden)) {
      throw Error('Unexpected recording action on an unopened scene');
    }
    action.scrollIntoView({block:'center', behavior:'instant'});
    action.focus();
    action.click();
    if (poster.hidden || document.querySelector('.scene-surface').getAttribute('aria-busy') !== 'true') {
      throw Error('Poster must stay visible while live playback starts');
    }
  })()`);
}

async function recordingJourney(cdp, record) {
  await cdp.evaluate(`(() => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function(kind, ...args) {
      return kind === 'webgl2' ? null : getContext.call(this, kind, ...args);
    };
  })()`);
  await startPoster(cdp, true);
  async function failedWithPoster() {
    await until('graphics failure and recording choice', () => cdp.evaluate(`
      document.querySelector('.scene-status').textContent.includes('could not start WebGPU or WebGL2') &&
      !document.querySelector('.scene-placeholder').hidden &&
      !document.querySelector('.scene-fallback').hidden &&
      !document.querySelector('amx-player') &&
      document.querySelector('[data-controls] select').disabled
    `));
  }
  await failedWithPoster();
  const point = await cdp.evaluate(`(() => {
    const video = document.querySelector('video');
    if (video.hasAttribute('src') || performance.getEntriesByType('resource').some(entry =>
      new URL(entry.name).pathname.endsWith('.mp4'))) throw Error('Recording fetched before the user chose it');
    const button = document.querySelector('.watch-recording');
    if (document.activeElement !== button) throw Error('Recording choice did not receive keyboard focus');
    button.scrollIntoView({block:'center', behavior:'instant'});
    const box = button.getBoundingClientRect();
    return {x: box.x + box.width / 2, y: box.y + box.height / 2};
  })()`);
  for (const type of ['mousePressed', 'mouseReleased']) {
    await cdp.send('Input.dispatchMouseEvent', {type, ...point, button:'left', clickCount:1});
  }
  await until('recording playback', () => cdp.evaluate(`document.querySelector('video').currentTime > 0.1`));
  await cdp.evaluate(`document.querySelector('.recording-panel .load-scene').click()`);
  await failedWithPoster();
  if (!await cdp.evaluate(`document.querySelector('.recording-panel').hidden && document.querySelector('video').paused`)) {
    throw Error('Retry must pause the recording and return to the poster');
  }
  record({kind:'recording-fallback', liveRetry:true});
}

async function presetJourney(cdp, capture, checkErrors, record) {
  if (await cdp.evaluate(`document.querySelector('[data-controls] [role=status]').textContent.trim()`)) {
    throw new Error('Enabled presets still display a startup instruction');
  }
  await cdp.evaluate(`(()=>{const input=document.querySelector('[data-controls] select');if(!input||input.disabled)throw Error('Preset control did not enable after load');input.value='1';input.dispatchEvent(new Event('change',{bubbles:true}));})()`);
  await until('preset applied', async()=>{
    await checkErrors();
    const status=await cdp.evaluate(`document.querySelector('[data-controls] [role=status]').textContent`);
    if(status.startsWith('Not applied:'))throw Error(status);
    return status.startsWith('Applied preset: 1.');
  });
  const result=await cdp.evaluate(`(async()=>{
    const player=document.querySelector('amx-player');
    const edit=new URL(document.querySelector('[data-open-code]').href);
    const download=document.querySelector('[data-source-download]').href;
    if(!download.startsWith('blob:'))throw Error('Preset download is not the configured source');
    const source=await (await fetch(download)).text();
    player.pause();player.seek(6);
    return {vars:player.vars,edit:edit.searchParams.get('preset'),source};
  })()`);
  if(result.vars.preset!==1 || result.edit!=='1' || !/^let preset = 1$/m.test(result.source))throw Error('Preset, Open code and downloaded source disagree');
  await until('preset painted',async()=>{await checkErrors();try{assertPaint(await capture(0,'preset-1'));return true;}catch{return false;}},10000);
  record({kind:'preset',value:1,downloadMatches:true,openCodeMatches:true});
}
