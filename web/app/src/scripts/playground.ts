import {fetchSource} from '../lib/requests';
import {readableError} from '../lib/errors';
import {presetFor,withOverride} from '../lib/overrides';
import {loadPlayer} from './scene';
const root=document.querySelector<HTMLElement>('[data-playground]');
if(root){
 const code=root.querySelector<HTMLTextAreaElement>('#source-code')!;
 const selector=root.querySelector<HTMLSelectElement>('#scene-choice')!;
 const status=root.querySelector<HTMLElement>('#editor-status')!;
 const diagnostics=root.querySelector<HTMLElement>('#diagnostics')!;
 const shell=root.querySelector<HTMLElement>('[data-scene-shell]')!;
 const run=root.querySelector<HTMLButtonElement>('#run')!;
 let original='', lastDownloaded:string|null=null, current='', pending=0, busy=false, sourceLoading=false;
 function lockSource(locked:boolean){sourceLoading=locked;root!.querySelector<HTMLButtonElement>('#download')!.disabled=locked||!current;code.readOnly=locked;selector.disabled=locked;run.disabled=locked;root!.querySelectorAll<HTMLButtonElement>('#open-source,#reset').forEach(b=>b.disabled=locked);}
 const ids=Array.from(selector.options,x=>x.value);
 async function open(id:string,initial=false){
  if(busy||sourceLoading)return;const ticket=++pending;lockSource(true);
  status.textContent='Loading source…';
  try{
   let text=await fetchSource(`${root!.dataset.base}${id}.amx`);if(ticket!==pending)return;
   const preset=initial?presetFor(id,new URLSearchParams(location.search)):null;
   if(preset)text=withOverride(text,...preset);
   original=text;lastDownloaded=null;current=id;code.value=text;selector.value=id;diagnostics.textContent='';
   shell.dataset.source=`${root!.dataset.base}${id}.amx`;
   const previous=shell.querySelector('amx-player') as (HTMLElement & {destroy():void})|null;previous?.destroy();previous?.remove();shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden=false;
   const next=new URL(location.href);next.searchParams.set('scene',id);if(!initial){next.searchParams.delete('preset');next.searchParams.delete('phase_degrees');}history.replaceState(null,'',next);
   status.textContent='Source ready. Edit, then Run. Nothing is saved automatically.';
  }catch(error){if(ticket===pending){selector.value=current;status.textContent=`Could not load source: ${readableError(error)}. Your existing text was kept.`;}}finally{if(ticket===pending)lockSource(false);}
 }
 async function build(){
  if(busy||sourceLoading||!current)return;const submitted=code.value;busy=true;run.disabled=true;root!.querySelectorAll<HTMLButtonElement>('#open-source,#reset').forEach(b=>b.disabled=true);status.textContent='Building…';diagnostics.textContent='';
  try{
   const player=await loadPlayer(shell);
   const result=await player.applySource(submitted);player.seek(0);player.play();
   status.textContent=code.value===submitted?'Built. The preview uses your current source.':'Built the submitted version. You have newer edits; Run again to preview them.';
   if(result?.length)diagnostics.textContent=JSON.stringify(result,null,2);
  }catch(error){
   status.textContent=`Not built: ${readableError(error)}. Your source is unchanged.`;
   if(code.value!==submitted)status.textContent+=' Diagnostics refer to the submitted version, not your newer edits.';
   const items=(error as {diagnostics?:{line?:number;column?:number;message?:string;code?:string}[]}).diagnostics;
   for(const d of items??[]){const b=document.createElement('button');b.className='diagnostic';b.textContent=`${d.line?`Line ${d.line}: `:''}${d.code??''} ${d.message??''}`;b.addEventListener('click',()=>{if(code.value!==submitted){status.textContent='These diagnostics describe the last submitted version. Run your current edits again to locate the error.';return;}const lines=code.value.split('\n');const offset=lines.slice(0,Math.max(0,(d.line??1)-1)).join('\n').length+(d.line&&d.line>1?1:0);code.focus();code.setSelectionRange(offset,offset+(lines[(d.line??1)-1]?.length??0));});diagnostics.append(b);}
  }finally{busy=false;run.disabled=false;root!.querySelectorAll<HTMLButtonElement>('#open-source,#reset').forEach(b=>b.disabled=false);}
 }
 root.querySelector('#open-source')!.addEventListener('click',()=>{if(code.value!==original&&!confirm('Replace your unsaved edits with another source?')){selector.value=current;return;}void open(selector.value);});
 root.querySelector('#reset')!.addEventListener('click',()=>{if(code.value===original||confirm('Reset to the original source? Your edits will be replaced.')){code.value=original;status.textContent='Source reset. Press Run to update the preview.';}});
 run.addEventListener('click',()=>void build());
 code.addEventListener('keydown',event=>{if((event.metaKey||event.ctrlKey)&&event.key==='Enter'){event.preventDefault();void build();}});
 root.querySelector('#download')!.addEventListener('click',()=>{const u=URL.createObjectURL(new Blob([code.value],{type:'text/plain'}));const a=document.createElement('a');a.href=u;a.download=`${current||'scene'}.amx`;a.click();lastDownloaded=code.value;setTimeout(()=>URL.revokeObjectURL(u),1000);});
 const hasUnsaved=()=>code.value!==original&&code.value!==lastDownloaded;
 document.addEventListener('click',event=>{
  const link=(event.target as Element)?.closest<HTMLAnchorElement>('a[href]');
  if(event.defaultPrevented||event.ctrlKey||event.metaKey||event.shiftKey||!link||link.hasAttribute('download')||link.target==='_blank'||!hasUnsaved())return;
  const destination=new URL(link.href);
  if(destination.origin===location.origin&&destination.pathname===location.pathname&&destination.search===location.search&&destination.hash)return;
  if(!confirm('Leave this page? Download your source first if you want to keep these edits.')){event.preventDefault();}
  else lastDownloaded=code.value; // Explicitly discarded; avoid a second beforeunload prompt.
 });
 window.addEventListener('beforeunload',event=>{if(code.value!==original&&code.value!==lastDownloaded){event.preventDefault();event.returnValue='';}});
 const requested=new URLSearchParams(location.search).get('scene')??'';void open(ids.includes(requested)?requested:'first-move',true);
}
