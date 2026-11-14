import {fetchSource,withTimeout} from '../lib/requests';
import {readableError} from '../lib/errors';
import {withOverride} from '../lib/overrides';
type Player = HTMLElement & {play():void; pause():void; seek(time:number):void; applySource(source:string):Promise<unknown[]>; setVars(vars:Record<string,number>):Promise<void>; destroy():void;};
const ready = new WeakMap<HTMLElement, Promise<Player>>();
const loading = new WeakMap<HTMLElement, Promise<Player>>();
const imports = new Map<string, Promise<unknown>>();
export function loadPlayer(shell:HTMLElement):Promise<Player> {
 const pending=loading.get(shell);if(pending)return pending;
 const operation=createPlayer(shell).finally(()=>loading.delete(shell));loading.set(shell,operation);return operation;
}
async function createPlayer(shell:HTMLElement):Promise<Player> {
 const existing=shell.querySelector<Player>('amx-player');
 if(existing) return ready.get(existing) ?? existing;
 const module=shell.dataset.module!;
 if(!imports.has(module)) imports.set(module,import(/* @vite-ignore */module).catch(e=>{imports.delete(module);throw e;}));
 try{await withTimeout(imports.get(module)!,15000,'Player module loading timed out. Please retry.');}
 catch(error){imports.delete(module);throw error;}
 await withTimeout(customElements.whenDefined('amx-player'),15000,'Player registration timed out. Please retry.');
 const player=document.createElement('amx-player') as Player;
 player.setAttribute('src',shell.dataset.source!);
 player.setAttribute('title',shell.dataset.title || 'Animation preview');
 player.setAttribute('aspect','16:9');player.setAttribute('controls','');
 player.addEventListener('amxerror',(event)=>{
  const detail=(event as CustomEvent).detail;
  const status=shell.querySelector<HTMLElement>('.scene-status');
  if(status)status.textContent=`Playback unavailable: ${readableError(new Error(detail?.message || 'The runtime could not open this scene.'))}`;
 });
 const loaded=new Promise<Player>((resolve,reject)=>{
  const timeout=setTimeout(()=>reject(new Error('The player took too long to initialize. Please retry.')),60000);
  player.addEventListener('amxready',()=>{clearTimeout(timeout);resolve(player);},{once:true});
  player.addEventListener('amxerror',event=>{clearTimeout(timeout);reject(new Error((event as CustomEvent).detail?.message ?? 'Player initialization failed.'));},{once:true});
 });
 ready.set(player,loaded);
 shell.querySelector('.scene-mount')!.append(player);
 shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden=true;
 try{const result=await loaded;const status=shell.querySelector('.scene-status');if(status)status.textContent='';document.querySelectorAll<HTMLSelectElement>('[data-controls] select').forEach(x=>x.disabled=false);return result;}catch(error){player.destroy();player.remove();shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden=false;throw error;}
}
for(const shell of document.querySelectorAll<HTMLElement>('[data-scene-shell]')){
 const buttons=shell.querySelectorAll<HTMLButtonElement>('.load-scene');
 for(const button of buttons)button.addEventListener('click',async()=>{
  buttons.forEach(b=>b.disabled=true);
  shell.querySelector<HTMLVideoElement>('video')?.pause();
  const recording=shell.querySelector<HTMLElement>('.recording-panel');if(recording)recording.hidden=true;
  try {(await loadPlayer(shell)).play();}
  catch(error){shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden=false;shell.querySelector('.scene-status')!.textContent=`Could not load the player: ${readableError(error)}`;}
  finally{buttons.forEach(b=>b.disabled=false);}
 });
 shell.querySelector('.watch-recording')?.addEventListener('click',()=>{
  const panel=shell.querySelector<HTMLElement>('.recording-panel')!;
  const video=panel.querySelector<HTMLVideoElement>('video')!;
  if(!video.src)video.src=video.dataset.recording!;
  shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden=true;
  shell.querySelector('.scene-status')!.textContent='';panel.hidden=false;
  video.addEventListener('error',()=>{shell.querySelector('.scene-status')!.textContent='The recording could not load. You can still download its source.';},{once:true});
  video.play().catch(()=>{shell.querySelector('.scene-status')!.textContent='Press play in the video controls to start the recording.';});
 });
 const video=shell.querySelector<HTMLVideoElement>('video');
 const toggle=shell.querySelector<HTMLButtonElement>('.pause-recording');
 if(video&&toggle){
  const sync=()=>{toggle.textContent=video.paused?'Play recording':'Pause recording';};
  video.addEventListener('play',sync);video.addEventListener('pause',sync);video.addEventListener('ended',sync);
  const playRecording=()=>video.play().catch(()=>{shell.querySelector('.scene-status')!.textContent='The recording could not start. Use the video controls to retry.';});
  toggle.addEventListener('click',()=>{if(video.paused)void playRecording();else video.pause();});
  shell.querySelector('.restart-recording')?.addEventListener('click',()=>{video.currentTime=0;void playRecording();});
 }
 const observer=new IntersectionObserver(entries=>{if(!entries[0].isIntersecting){shell.querySelector<Player>('amx-player')?.pause();shell.querySelector<HTMLVideoElement>('video')?.pause();}});observer.observe(shell);
}
for(const form of document.querySelectorAll<HTMLElement>('[data-controls]')){
 const status=form.querySelector<HTMLElement>('[role=status]')!;
 let committed=form.querySelector<HTMLSelectElement>('select')?.value ?? '0';
 form.addEventListener('change',async(event)=>{
  const input=event.target as HTMLInputElement;
  const shell=document.querySelector<HTMLElement>('[data-scene-shell]')!;
  const player=shell.querySelector<Player>('amx-player');
  if(!player){status.textContent='Load the scene before applying a parameter.';return;}
  const hadFocus=document.activeElement===input;
  const value=Number(input.value);
  status.textContent='Applying…';
  try{
   input.disabled=true;
   const configured=withOverride(await fetchSource(shell.dataset.source!),input.name,value);
   await player.setVars({[input.name]:value});
   player.seek(0);player.play();
   const edit=document.querySelector<HTMLAnchorElement>('[data-open-code]');
   if(edit){const next=new URL(edit.href);next.searchParams.set(input.name,String(value));edit.href=next.href;}
   const download=document.querySelector<HTMLAnchorElement>('[data-source-download]');
   if(download){if(download.href.startsWith('blob:'))URL.revokeObjectURL(download.href);download.href=URL.createObjectURL(new Blob([configured],{type:'text/plain'}));}
   committed=input.value;status.textContent=`Applied ${input.name}: ${value}. Playing from the start. Code and download use this preset.`;
  }
  catch(error){input.value=committed;status.textContent=`Not applied: ${readableError(error)}`;}finally{input.disabled=false;if(hadFocus)input.focus();}
 });
}
