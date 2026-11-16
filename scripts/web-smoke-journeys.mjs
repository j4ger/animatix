import {until} from './web-smoke-browser.mjs';
import {assertPaint} from './web-smoke-pixels.mjs';
const players = `document.querySelectorAll('amx-player')`;
export async function journey(cdp, contract, capture, record, checkErrors) {
  const diagnose = async label => {
    if(process.env.SMOKE_DIAGNOSTICS==='1') await cdp.evaluate(`window.__smokeDiagnostics?.sample(${JSON.stringify(label)})`);
  };
  await until('page complete', () => cdp.evaluate(`document.readyState==='complete'`));
  if (!await cdp.evaluate(`!!document.querySelector('body')`)) throw new Error('Missing document body');
  if (contract.action !== 'fixture') {
    const cold = await cdp.evaluate(`({players:${players}.length, wasm:performance.getEntriesByType('resource').filter(x=>/\\.wasm(?:[?#]|$)/.test(x.name)).length})`);
    if (cold.players || cold.wasm) throw new Error('Public page eagerly initialized the runtime before user interaction');
  }
  if (contract.action === 'static') {
    for (const path of contract.links || []) {
      const found = await cdp.evaluate(`Array.from(document.querySelectorAll('a[href]')).some(a=>new URL(a.href).pathname===${JSON.stringify(path)})`);
      if (!found) throw new Error(`Missing navigation link ${path}`);
    }
    await checkErrors();
    return;
  }
  record({kind:'action',action:contract.action,pageTimeMs:await cdp.evaluate('performance.now()')});
  if (contract.action === 'load') {
    await cdp.evaluate(`(()=>{const b=document.querySelector('.load-scene');if(!b||b.hidden)throw Error('Missing Load interactive button');b.scrollIntoView({block:'center'});b.click();})()`);
  }
  if (contract.action === 'edit') {
    await until('source ready', () => cdp.evaluate(`document.querySelector('#editor-status')?.textContent.startsWith('Source ready.')`));
    await cdp.evaluate(`(()=>{const code=document.querySelector('#source-code');if(!code.value.includes('#65D9E8'))throw Error('Unexpected first-move source');code.value=code.value.replace('#65D9E8','#F46EA0');code.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('#run').click();})()`);
  }
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
      await until('editor build', async () => {
        await checkErrors();
        return cdp.evaluate(`document.querySelector('#editor-status').textContent.startsWith('Built.')`);
      });
    }
    await diagnose('before-pause');
    const info = await cdp.evaluate(`(()=>{const p=${players}[${index}];p.pause();return {src:p.getAttribute('src'),duration:p.duration,report:p.debugReport?.()};})()`);
    if (!(info.duration > 0)) throw new Error('Invalid public duration');
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
    await cdp.evaluate(`(()=>{const code=document.querySelector('#source-code');code.value+='\\nthis is deliberately invalid ???';window.__submitted=code.value;document.querySelector('#run').click();})()`);
    await until('visible build diagnostic', () => cdp.evaluate(`document.querySelector('#editor-status').textContent.startsWith('Not built:')`));
    if (!await cdp.evaluate(`document.querySelector('#source-code').value===window.__submitted`)) throw new Error('Failed build changed source');
    // A failed edit must retain the last successful scene, not merely its DOM.
    await cdp.evaluate(`${players}[0].pause();${players}[0].seek(2.1)`);
    assertPaint(await capture(0,'retained'), {color:'#F46EA0',x:920/1280,y:0.5,tolerance:0.08});
  }
  const resources = await cdp.evaluate(`performance.getEntriesByType('resource').map(x=>({name:new URL(x.name).pathname,startMs:x.startTime,durationMs:x.duration,bytes:x.transferSize}))`);
  const fonts = resources.filter(x=>/\/fonts\/.*\.gz$/.test(x.name));
  if (contract.fonts !== undefined && Boolean(fonts.length) !== contract.fonts) throw new Error(`Font contract violated: expected ${contract.fonts}, requests ${fonts.length}`);
  const events = await cdp.evaluate(`__smoke.events`);
  record({kind:'resources',resources,events});
  await checkErrors();
}

async function presetJourney(cdp, capture, checkErrors, record) {
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
