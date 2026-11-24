import {until} from './web-smoke-browser.mjs';
import {assertPaint} from './web-smoke-pixels.mjs';
const players = `document.querySelectorAll('amx-player')`;
export async function journey(cdp, contract, capture, record, checkErrors) {
  const diagnose = async label => {
    if(process.env.SMOKE_DIAGNOSTICS==='1') await cdp.evaluate(`window.__smokeDiagnostics?.sample(${JSON.stringify(label)})`);
  };
  await until('page complete', () => cdp.evaluate(`document.readyState==='complete'`));
  if (!await cdp.evaluate(`!!document.querySelector('body')`)) throw new Error('Missing document body');
  if (contract.action === 'static') {
    const cold = await cdp.evaluate(`({
      players: ${players}.length,
      requests: performance.getEntriesByType('resource').filter(entry =>
        ['.wasm', '.wasm.gz', '.mp4', '/runtime/amx-player.js'].some(suffix =>
          new URL(entry.name).pathname.endsWith(suffix))).length,
    })`);
    if (cold.players || cold.requests) throw new Error('Documentation loaded a player or its runtime');
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
  if (contract.action === 'recording') {
    await recordingJourney(cdp, record);
    return;
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
    if (contract.action === 'load') await playReadyScene(cdp, contract);
    if (contract.action === 'edit') {
      await cdp.evaluate(`(() => {
        const code = document.querySelector('#source-code');
        if (!code.value.includes('color: accent.primary')) throw Error('Unexpected first-move dot color');
        const source = code.value
          .replace('color: accent.primary', 'color: hex_color("f46ea0")')
          .replace('duration: 3', 'duration: 4').replace('#0.5s', '#0.25s')
          .replace('#3s', '#1.9s\\n\\n#3s');
        if (source === code.value) throw Error('Unexpected first-move source');
        code.value = source;
        code.dispatchEvent(new Event('input', {bubbles: true}));
      })()`);
      await until('new timestamp split a cell', () => cdp.evaluate(`
        Array.from(document.querySelectorAll('.source-cell .keyframe-line')).some(line => line.textContent === '#1.9s')
      `));
      await until('live editor build and updated timeline', async () => {
        await checkErrors();
        return cdp.evaluate(`(() => {
          const player = ${players}[${index}];
          const marks = document.querySelector('.sample .amx-marks');
          return player.duration === 4 && !marks.querySelector('[data-t="0.5"]') &&
            marks.querySelector('[data-t="0.25"]')?.style.left === '6.25%';
        })()`);
      });
    }
    await diagnose('before-pause');
    const info = await cdp.evaluate(`(()=>{const p=${players}[${index}];p.pause();return {src:p.getAttribute('src'),duration:p.duration,backend:p.backend,report:p.debugReport?.()};})()`);
    if (!(info.duration > 0)) throw new Error('Invalid public duration');
    if (contract.backend && info.backend !== contract.backend) throw Error(`Expected ${contract.backend}, got ${info.backend}`);
    const samples = contract.motion ? [0, 2.1] : [info.duration * 0.65];
    for (const time of samples) {
      await diagnose(`before-seek-${time}`);
      if (contract.action === 'story') {
        await cdp.evaluate(`(() => {
          const slider = document.querySelector('.timeline-input');
          slider.value = '${time}';
          slider.dispatchEvent(new Event('input', {bubbles: true}));
        })()`);
      } else {
        await cdp.evaluate(`${players}[${index}].seek(${time})`);
      }
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
  if (contract.action === 'preview') {
    await cdp.evaluate(`(async () => {
      const toggle = document.querySelector('#insertion-sort .inspect');
      toggle.scrollIntoView({block: 'center', behavior: 'instant'});
      const before = toggle.getBoundingClientRect().top;
      for (const expanded of ['true', 'false']) {
        toggle.click();
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const offset = toggle.getBoundingClientRect().top - before;
        if (toggle.getAttribute('aria-expanded') !== expanded || Math.abs(offset) > 2) {
          throw Error('Source toggle moved vertically by ' + offset + 'px (expanded: ' + expanded + ')');
        }
      }
    })()`);
  }
  for (const module of contract.modules || []) {
    const count = await cdp.evaluate(`performance.getEntriesByType('resource').filter(entry=>new URL(entry.name).pathname.endsWith('/dev/smoke/imports/${module}')).length`);
    if (count !== 1) throw new Error(`Expected exactly one on-demand fetch for ${module}, got ${count}`);
  }
  if (contract.preset) await presetJourney(cdp, capture, checkErrors, record);
  if (contract.editSource) await inlineEditorJourney(cdp, checkErrors, contract.editSource);
  if (contract.action === 'story') await storyEditorJourney(cdp, checkErrors);
  if (contract.action === 'edit') {
    await cdp.evaluate(`(() => {
      const code = document.querySelector('#source-code');
      code.value = code.value.replace('#1.9s\\n\\n', '') + '\\nthis is deliberately invalid ???';
      window.__submitted = code.value;
      code.dispatchEvent(new Event('input', {bubbles: true}));
    })()`);
    await until('removed timestamp merged its cell', () => cdp.evaluate(`
      document.querySelector('.playground .source-editor[data-highlighted]') &&
      Array.from(document.querySelectorAll('.source-cell .keyframe-line')).map(line => line.textContent).join(',') === '#0s,#0.25s,#3s'
    `));
    await until('visible build diagnostic', () => cdp.evaluate(`document.querySelector('#editor-status').textContent.startsWith('Not built:')`));
    if (!await cdp.evaluate(`document.querySelector('#source-code').value===window.__submitted`)) throw new Error('Failed build changed source');
    // A failed edit must retain the last successful scene, not merely its DOM.
    await cdp.evaluate(`(() => {
      const slider = document.querySelector('.timeline-input');
      for (const time of [1, 2.1]) {
        slider.value = String(time);
        slider.dispatchEvent(new Event('input', {bubbles: true}));
      }
      if (document.querySelector('.sample-clock').textContent !== '2.10s') {
        throw Error('The time label did not follow the last paused seek');
      }
    })()`);
    assertPaint(await capture(0,'retained'), {color:'#F46EA0',x:920/1280,y:0.5,tolerance:0.08});
    await playgroundSwitchJourney(cdp);
  }
  const resources = await cdp.evaluate(`performance.getEntriesByType('resource').map(x=>({name:new URL(x.name).pathname,startMs:x.startTime,durationMs:x.duration,bytes:x.transferSize}))`);
  if (resources.some(x=>x.name.endsWith('.mp4'))) throw Error('Live playback fetched a recording');
  const fonts = resources.filter(x=>/\/fonts\/.*\.gz$/.test(x.name));
  if (contract.fonts !== undefined && Boolean(fonts.length) !== contract.fonts) throw new Error(`Font contract violated: expected ${contract.fonts}, requests ${fonts.length}`);
  const events = await cdp.evaluate(`__smoke.events`);
  record({kind:'resources',resources,events});
  await checkErrors();
}

async function playgroundSwitchJourney(cdp) {
  const choose = (id, accept, expectedPrompts) => cdp.evaluate(`(() => {
    const selector = document.querySelector('#scene-choice');
    const confirm = window.confirm;
    let prompts = 0;
    window.confirm = () => { prompts++; return ${accept}; };
    try {
      selector.value = ${JSON.stringify(id)};
      selector.dispatchEvent(new Event('change', {bubbles: true}));
      if (prompts !== ${expectedPrompts}) throw Error('Unexpected scene switch confirmation count: ' + prompts);
    } finally {
      window.confirm = confirm;
    }
  })()`);
  const loaded = id => until(`selected ${id} without an Open button`, () => cdp.evaluate(`(() => {
    const id = ${JSON.stringify(id)};
    const selector = document.querySelector('#scene-choice');
    const player = document.querySelector('amx-player');
    return selector.value === id && !selector.disabled &&
      new URLSearchParams(location.search).get('scene') === id &&
      player?.getAttribute('src').endsWith('/' + id + '.amx') && __smoke.ready.has(player);
  })()`));

  await cdp.evaluate(`window.__previewBeforeSwitch = document.querySelector('amx-player')`);
  await choose('compose', false, 1);
  if (!await cdp.evaluate(`
    document.querySelector('#scene-choice').value === 'first-move' &&
    document.querySelector('#source-code').value === window.__submitted &&
    document.querySelector('amx-player') === window.__previewBeforeSwitch &&
    new URLSearchParams(location.search).get('scene') === 'first-move'
  `)) throw Error('Cancelling scene selection must retain the source, selection and preview');

  await choose('compose', true, 1);
  await loaded('compose');
  await choose('ripple-and-cascade', true, 0);
  await loaded('ripple-and-cascade');
  if (!await cdp.evaluate(`
    document.querySelector('[data-scene-filename]').textContent === 'ripple-and-cascade.amx' &&
    document.querySelector('[data-source-download]').download === 'ripple-and-cascade.amx'
  `)) throw Error('Playground filename and download must use the selected scene name');
  await choose('first-move', true, 0);
  await loaded('first-move');
}

async function playReadyScene(cdp, {recording}) {
  await until('preloaded preview with enabled controls', () => cdp.evaluate(`
    document.querySelector('.scene-placeholder').hidden &&
    !document.querySelector('.scene-surface').hasAttribute('aria-busy') &&
    !document.querySelector('.sample-play').disabled
  `));
  await cdp.evaluate(`(() => {
    const surface = document.querySelector('.scene-surface');
    const action = document.querySelector('.sample-play');
    const fallback = document.querySelector('.scene-fallback');
    const player = document.querySelector('amx-player');
    if (player.time !== 0) throw Error('The preloaded scene must wait at the first frame');
    if (!action.getClientRects().length || action.getBoundingClientRect().top < surface.getBoundingClientRect().bottom) {
      throw Error('Editable scene playback controls must sit under the picture');
    }
    if (Boolean(fallback) !== ${Boolean(recording)} || (fallback && !fallback.hidden)) {
      throw Error('Unexpected recording action on an unopened scene');
    }
    action.focus();
    action.click();
    if (surface.hasAttribute('aria-busy') || player !== document.querySelector('amx-player')) {
      throw Error('Play must reuse the prepared preview');
    }
  })()`);
  await until('Play advances the prepared scene', () => cdp.evaluate(`document.querySelector('amx-player').time > 0`));
  await cdp.evaluate(`document.querySelector('.sample-play').click()`);
}

async function recordingJourney(cdp, record) {
  async function failedWithPoster() {
    await until('graphics failure and recording choice', () => cdp.evaluate(`
      document.querySelector('.scene-status').textContent.includes('could not start WebGPU or WebGL2') &&
      !document.querySelector('.scene-placeholder').hidden &&
      !document.querySelector('.scene-fallback').hidden &&
      !document.querySelector('amx-player') &&
      document.querySelector('[data-scene-preset]').disabled
    `));
  }
  await failedWithPoster();
  const point = await cdp.evaluate(`(() => {
    const video = document.querySelector('video');
    if (video.hasAttribute('src') || performance.getEntriesByType('resource').some(entry =>
      new URL(entry.name).pathname.endsWith('.mp4'))) throw Error('Recording fetched before the user chose it');
    const button = document.querySelector('.watch-recording');
    button.scrollIntoView({block:'center', behavior:'instant'});
    const box = button.getBoundingClientRect();
    return {x: box.x + box.width / 2, y: box.y + box.height / 2};
  })()`);
  for (const type of ['mousePressed', 'mouseReleased']) {
    await cdp.send('Input.dispatchMouseEvent', {type, ...point, button:'left', clickCount:1});
  }
  await until('recording playback', () => cdp.evaluate(`document.querySelector('video').currentTime > 0.1`));
  await cdp.evaluate(`(() => {
    const retry = document.querySelector('.sample-play');
    retry.focus();
    retry.click();
  })()`);
  await failedWithPoster();
  if (!await cdp.evaluate(`
    document.querySelector('.recording-panel').hidden && document.querySelector('video').paused &&
    document.activeElement === document.querySelector('.watch-recording')
  `)) {
    throw Error('Retry must pause the recording and return to the poster');
  }
  record({kind:'recording-fallback', liveRetry:true});
}

async function presetJourney(cdp, capture, checkErrors, record) {
  await cdp.evaluate(`(()=>{const input=document.querySelector('[data-scene-preset]');if(input.disabled)throw Error('Preset control did not enable after load');input.value='1';input.dispatchEvent(new Event('change',{bubbles:true}));})()`);
  await until('preset applied', async()=>{
    await checkErrors();
    return cdp.evaluate(`(() => {
      const input = document.querySelector('[data-scene-preset]');
      const status = input.closest('.scene-presets').querySelector('[role=status]').textContent;
      if (status) throw Error(status);
      return !input.disabled && input.value === '1';
    })()`);
  });
  const result=await cdp.evaluate(`(async()=>{
    const player=document.querySelector('amx-player');
    const draft=document.querySelector('.source-editor textarea').value;
    const download=document.querySelector('[data-source-download]').href;
    if(!download.startsWith('blob:'))throw Error('Preset download is not the configured source');
    const source=await (await fetch(download)).text();
    player.pause();player.seek(6);
    return {draft,source};
  })()`);
  if (!containsPortableLayout(result.source, result.draft) || !/^let start_pattern = 1$/m.test(result.source)) {
    throw Error('Preset download did not preserve the draft body and include its layout');
  }
  await until('full grid painted with the cascade preset', async () => {
    await checkErrors();
    try {
      return assertPaint(await capture(0, 'preset-1')).coverage > 0.07;
    } catch {
      return false;
    }
  }, 10000);
  record({kind:'preset',value:1,downloadMatches:true});
}

async function inlineEditorJourney(cdp, checkErrors, {duration, stamp}) {
  const editedStamp = stamp + 0.25;
  await cdp.evaluate(`(() => {
    const address = location.href;
    document.querySelector('.inspect').click();
    const source = document.querySelector('.source-editor textarea');
    if (!source.getClientRects().length || location.href !== address) throw Error('Source must open in place');
    source.value = source.value.replace('duration: ${duration}', 'duration: ${duration + 2}').replace('#${stamp}s', '#${editedStamp}s');
    source.dispatchEvent(new Event('input', {bubbles:true}));
  })()`);
  await until('inline edit rebuilt the timeline', async () => {
    await checkErrors();
    return cdp.evaluate(`document.querySelector('amx-player').duration === ${duration + 2} &&
      !!document.querySelector('.sample .amx-marks [data-t="${editedStamp}"]')`);
  });
  await cdp.evaluate(`document.querySelector('.reset-source').click()`);
  await until('inline editor reset', () => cdp.evaluate(`document.querySelector('amx-player').duration === ${duration} &&
    !document.querySelector('.sample .amx-marks [data-t="${editedStamp}"]')`));
}

async function storyEditorJourney(cdp, checkErrors) {
  await until('story exposes the three AMX cue names', () => cdp.evaluate(`
    JSON.stringify(document.querySelector('amx-player').timelineMarkers
      .filter(marker => marker.kind === 'cue').map(({id, t}) => [id, t])) ===
      JSON.stringify([['train', 0], ['speaker', 8], ['ear', 16]])
  `));
  await scrollStory(cdp, 0.5, 4);
  await scrollStory(cdp, 1, 8);
  await scrollStory(cdp, 0, 0);

  try {
    await cdp.send('Emulation.setDeviceMetricsOverride', {width: 620, height: 850, deviceScaleFactor: 1, mobile: false});
    await until('story switched to its narrow layout', () => cdp.evaluate(`matchMedia('(max-width: 740px)').matches`));
    await scrollStory(cdp, 1, 8);
    await scrollStory(cdp, 0.5, 4);
    await cdp.send('Emulation.setEmulatedMedia', {features: [{name: 'prefers-reduced-motion', value: 'reduce'}]});
    await scrollStory(cdp, 0.8, 8);
    await scrollStory(cdp, 0.2, 0);
  } finally {
    await cdp.send('Emulation.setEmulatedMedia', {features: []});
    await cdp.send('Emulation.setDeviceMetricsOverride', {width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false});
  }

  await cdp.evaluate(`(() => {
    document.querySelector('.inspect').click();
    const editor = document.querySelector('.source-editor textarea');
    editor.value = editor.value.replace('#8s\\n#cue:speaker', '#9s\\n#cue:speaker');
    editor.dispatchEvent(new Event('input', {bubbles: true}));
  })()`);
  await until('editing AMX moves the speaker cue to 9 seconds', () => cdp.evaluate(`
    document.querySelector('amx-player').timelineMarkers.some(m => m.id === 'speaker' && m.t === 9)
  `));
  const retained = await cdp.evaluate(`(async () => {
    const player = document.querySelector('amx-player');
    const installed = JSON.stringify(player.timelineMarkers);
    try { await player.applySource('config {'); } catch {
      return JSON.stringify(player.timelineMarkers) === installed;
    }
    return false;
  })()`);
  if (!retained) throw Error('A failed build changed the installed cue snapshot');
  await cdp.evaluate(`document.querySelector('.inspect').click()`);
  await scrollStory(cdp, 1, 9);

  await cdp.evaluate(`(() => {
    document.querySelector('.inspect').click();
    const editor = document.querySelector('.source-editor textarea');
    editor.value = editor.value.replace('#cue:speaker', '#cue:renamed');
    editor.dispatchEvent(new Event('input', {bubbles: true}));
  })()`);
  await until('a missing cue is reported without disabling playback', () => cdp.evaluate(`
    document.querySelector('[data-cue-status]').textContent.includes('speaker') &&
      !document.querySelector('.sample-play').disabled
  `));
  await cdp.evaluate(`document.querySelector('.reset-source').click()`);
  await until('reset restores the original cue binding', () => cdp.evaluate(`
    !document.querySelector('[data-cue-status]').textContent &&
      document.querySelector('amx-player').timelineMarkers.some(m => m.id === 'speaker' && m.t === 8)
  `));

  await cdp.evaluate(`document.querySelector('[data-edit-film="half"]').click()`);
  await until('half strength edits and replays the scene inline', async () => {
    await checkErrors();
    return cdp.evaluate(`(() => {
      const editor = document.querySelector('.source-editor textarea');
      const download = document.querySelector('[data-source-download]');
      return editor.getClientRects().length && editor.value.includes('let speaker_level = 0.5') &&
        !(document.querySelector('.sample-play').disabled) && download.href.startsWith('blob:') &&
        location.pathname.endsWith('/journey/');
    })()`);
  });
  const result = await cdp.evaluate(`(async () => ({
    draft: document.querySelector('.source-editor textarea').value,
    source: await (await fetch(document.querySelector('[data-source-download]').href)).text(),
  }))()`);
  if (!containsPortableLayout(result.source, result.draft)) {
    throw Error('Journey download did not preserve the edited scene and include its layout');
  }
}

async function scrollStory(cdp, progress, seconds) {
  await cdp.evaluate(`(async () => {
    await new Promise(resolve => requestAnimationFrame(resolve));
    const speaker = document.querySelector('#speaker').getBoundingClientRect();
    const film = document.querySelector('.film');
    const narrow = matchMedia('(max-width: 740px)').matches;
    const offset = narrow ? -film.offsetHeight - 20 : (speaker.height - innerHeight) / 2;
    const stop = Math.max(0, Math.min(document.documentElement.scrollHeight - innerHeight,
      speaker.top + scrollY + offset));
    window.scrollTo({top: stop * ${progress}, behavior: 'instant'});
  })()`);
  await until('reading scroll seeks to ' + seconds + ' seconds', () => cdp.evaluate(`
    Math.abs(document.querySelector('amx-player').time - ${seconds}) < 0.04
  `));
}

function containsPortableLayout(source, draft) {
  const sceneBody = draft.split('import "../templates/layout.amx"\n')[1];
  return !!sceneBody && source.endsWith(sceneBody) && source.includes('pub component SceneCaption');
}
