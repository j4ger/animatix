const b=[...document.querySelectorAll("script[type=module]")].find(u=>(u.src||"").includes("amx-player.js")),A=(()=>{const u=b?.src||"",t=u.lastIndexOf("/amx-player.js");return t>=0?u.slice(0,t):new URL(".",import.meta.url).href.replace(/\/$/,"")})();function v(u){const t=u!=="full",e=b?.getAttribute("data-runtime-base"),i=e?new URL(e,document.baseURI).href.replace(/\/$/,""):`${A}/..`,o=i.split("/").pop();let a,n;o==="pkg-slim"?(a=i,n=i.slice(0,-5)):o==="pkg"?(n=i,a=`${i}-slim`):(a=`${i}/pkg-slim`,n=`${i}/pkg`);const d=t?a:n,h=t?n:a;return d===h?[d]:[d,h]}const E=window.matchMedia?.("(prefers-reduced-motion: reduce)").matches??!1,C=navigator.connection?.saveData===!0,L=.28,S={playBig:'<svg width="26" height="26" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>'},f=new Map;function $(u){const[t]=v(u);return f.has(t)||f.set(t,(async()=>{const e=v(u);let i;for(const[o,a]of e.entries())try{const n=await import(`${a}/animatix_web.js`);return await n.default(),await n.init_engine?.(),n}catch(n){i=n,f.delete(t),o+1<e.length&&console.info(`amx-player: no engine at ${a}, trying ${e[o+1]}`)}throw i})()),f.get(t)}const m=new Set;let g=!1,y=0;function w(u){const t=Math.min((u-y)/1e3,.1);y=u;let e=!1;for(const i of m)i.advance(t)&&(e=!0);e?requestAnimationFrame(w):g=!1}function T(){g||(g=!0,y=performance.now(),requestAnimationFrame(w))}const k={"16:9":16/9,"4:3":4/3,"1:1":1,"9:16":9/16};class x extends HTMLElement{static observedAttributes=["src","autoplay","loop","hold","controls","title","aspect","profile","quality"];constructor(){super(),this.attachShadow({mode:"open"}),this._state="idle",this._player=null,this._playing=!1,this._loop=!1,this._time=0,this._duration=0,this._holdSeconds=0,this._cycle=0,this._fade=0,this._restTime=0,this._lastAlpha=1,this._visible=!1,this._observer=null,this._initialized=!1,this._scrubbing=!1,this._peeking=!1,this._latched=!1,this._resumeOnLeave=!1,this._suppressHoverPeek=!1,this._downX=0,this._rate=1,this._markers=[]}connectedCallback(){this._initialized||(this._initialized=!0,this._renderSkeleton(),this._setupObserver())}disconnectedCallback(){this._observer?.disconnect(),m.delete(this),this._playing=!1}attributeChangedCallback(t){if(t==="aspect"&&this._initialized&&(this._stage.style.aspectRatio=String(this._aspectRatio())),t==="hold"&&this._initialized&&this._duration>0&&this._configureCycle(),t==="quality"&&this._initialized&&this._player?.set_quality){try{this._player.set_quality(this._quality())}catch(e){console.warn(`amx-player: ${e.message}`)}this._resetForReload()}(t==="src"||t==="profile")&&this._initialized&&this._resetForReload()}_resetForReload(){this._state="idle",this._player=null,this._time=0,this._renderSkeleton(),this._maybeStartLoading()}_profile(){return this.getAttribute("profile")==="full"?"full":"slim"}_quality(){const t=this.getAttribute("quality")??"draft";return["draft","preview","production"].includes(t)?t:(console.warn(`amx-player: unknown quality '${t}', using draft`),"draft")}_aspectRatio(){const t=this.getAttribute("aspect");if(t&&k[t])return k[t];const[e,i]=(t||"").split(":").map(Number);return e>0&&i>0?e/i:16/9}_renderSkeleton(){this._stage=document.createElement("div"),this._stage.className="stage",this._stage.style.aspectRatio=String(this._aspectRatio());const t=this.getAttribute("title")||"",e=document.createElement("style");e.textContent=`
      :host { display: block; overflow: hidden;
              border-radius: 8px; background: #0a0f17; }
      .stage { position: relative; overflow: hidden; }
      .skeleton {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        background: linear-gradient(120deg, #0a0f17 40%, #141c28 50%, #0a0f17 60%);
        background-size: 300% 100%;
        animation: shimmer 2.2s linear infinite;
        color: #808fa6; font: 13px/1.4 system-ui, sans-serif;
      }
      @keyframes shimmer { to { background-position: -300% 0; } }
      canvas { position: absolute; inset: 0; width: 100%; height: 100%;
               display: block; object-fit: contain; }
      .veil {
        position: absolute; inset: 0; display: flex; flex-direction: column;
        align-items: center; justify-content: center; gap: 10px;
        color: #8b96a7; font: 13px/1.4 system-ui, sans-serif;
        background: rgba(13,16,22,0.55); opacity: 0; transition: opacity .3s;
        pointer-events: none; text-align: center; padding: 12px;
      }
      .veil.show { opacity: 1; pointer-events: auto; }
      .veil.error { color: #ef6a6a; }
      .playbtn {
        position: absolute; left: 50%; top: 50%;
        transform: translate(-50%, -50%);
        width: 52px; height: 52px; border-radius: 50%;
        border: 1px solid rgba(245,185,66,.5); background: rgba(245,185,66,.14);
        color: #f5b942; cursor: pointer; display: none;
        align-items: center; justify-content: center; padding: 0;
      }
      .playbtn.show { display: flex; }
      .playbtn svg { display: block; }
      /* The strip lives below the stage, in flow, and spans the full width
         and height of the bar: it never covers the picture. Hovering it
         freezes the clock and peeks the frame under the pointer; the clock
         resumes on leave unless the pause was latched by a click. */
      .strip {
        position: relative; height: 44px;
        display: flex; align-items: center;
        border-top: 1px solid rgba(255,255,255,.07);
        cursor: pointer; touch-action: none;
      }
      .strip:focus-visible { outline: 2px solid #f5b942; outline-offset: -2px; }
      .strip .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        background: rgba(245,185,66,.16);
        border-right: 2px solid #f5b942;
        transform-origin: left; transform: scaleX(0);
        pointer-events: none;
      }
      /* Timeline landmarks. Keyframes are thin ticks; scene starts are
         diamonds; a transition is a hatched span \u2014 all full height. */
      .strip .tick {
        position: absolute; top: 0; bottom: 0; width: 2px;
        transform: translateX(-50%);
        background: rgba(255,255,255,.28);
        pointer-events: none;
      }
      .strip .diamond {
        position: absolute; top: 50%; width: 9px; height: 9px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 2px;
        pointer-events: none;
      }
      .strip .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.35) 0 4px, transparent 4px 8px);
        pointer-events: none;
      }
      .strip .cursor {
        position: absolute; top: 0; bottom: 0; width: 2px;
        transform: translateX(-50%);
        background: rgba(245,185,66,.9);
        opacity: 0; pointer-events: none;
      }
      .strip.peeking .cursor { opacity: 1; }
      .chip {
        position: absolute; right: 56px; top: 50%;
        transform: translateY(-50%);
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums;
        background: rgba(5,8,12,.55); border-radius: 6px; padding: 3px 8px;
        pointer-events: none;
      }
      .speed {
        position: absolute; right: 4px; top: 50%;
        transform: translateY(-50%);
        width: 48px; height: 36px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        font: 12px ui-monospace, monospace;
        display: flex; align-items: center; justify-content: center;
      }
      .speed:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .speed:focus-visible { outline: 2px solid #f5b942; }
      @media (pointer: coarse) {
        .strip { height: 52px; }
        .speed { width: 52px; height: 44px; }
      }
    `,this.shadowRoot.replaceChildren(e,this._stage),this._skeleton=document.createElement("div"),this._skeleton.className="skeleton",this._skeleton.textContent=t?`${t}`:"animatix scene",this._stage.appendChild(this._skeleton),this._canvas=document.createElement("canvas"),this._canvas.width=1280,this._canvas.height=720,this._canvas.hidden=!0,this._veil=document.createElement("div"),this._veil.className="veil",this._playbtn=document.createElement("button"),this._playbtn.className="playbtn",this._playbtn.setAttribute("aria-label","Play"),this._playbtn.innerHTML=S.playBig,this._playbtn.addEventListener("click",()=>this.play()),this._controls=null}_showVeil(t,e){this._veil.className=`veil show${e?" error":""}`,this._veil.textContent=t,this._veil.isConnected||this.shadowRoot.appendChild(this._veil)}_hideVeil(){this._veil.className="veil"}_looping(){return this.hasAttribute("loop")||this._loop===!0}_configureCycle(){const t=this.getAttribute("hold"),e=t===null?NaN:Number(t),i=Number.isFinite(e)&&e>=0?Math.min(e,30):.7;this._holdSeconds=i,this._cycle=this._duration+i,this._fade=i>0?Math.min(L,i/2,this._duration/4):0}_loopAlpha(){if(!this._playing||!this._looping()||this._fade<=0)return 1;if(this._time<this._fade)return this._time/this._fade;const t=this._cycle-this._fade;return this._time>t?Math.max(0,(this._cycle-this._time)/this._fade):1}_setupObserver(){this._observer=new IntersectionObserver(t=>{for(const e of t)this._visible=e.isIntersecting,this._visible&&this._state==="idle"&&this._maybeStartLoading(),!this._visible&&this._playing&&this.pause()},{rootMargin:"200px"}),this._observer.observe(this)}_shouldAutoplay(){return this.hasAttribute("autoplay")&&!E&&!C}async _loadFonts(t){const e=this.getAttribute("data-fonts");if(!(!e||!this._player.add_font))for(const i of e.split(/\s+/).filter(Boolean))try{const o=await fetch(new URL(i,document.baseURI));if(!o.ok)throw new Error(`HTTP ${o.status}`);this._player.add_font(new Uint8Array(await o.arrayBuffer()))}catch(o){console.warn(`amx-player: font '${i}' skipped (${o.message})`)}}async _loadScene(t,e,i){const o=new URL(i,document.baseURI).href,a=typeof t.add_module=="function",n=new Map,d=new Set,h=new Map;let c=null;for(let p=0;p<24;p+=1){for(const[r,_]of n)t.add_module(r,_);await this._fetchAssets(t,e,n,o,h),c=this._buildWithAssets(t,e,h);const s=a?c?.missing_imports??[]:[],l=s.filter(r=>!n.has(r)&&!d.has(r));if(!s.length||!l.length)break;await Promise.all(l.map(async r=>{try{const _=await fetch(new URL(r,o));if(!_.ok)throw new Error(`HTTP ${_.status}`);n.set(r,await _.text())}catch(_){d.add(r),console.warn(`amx-player: import '${r}' skipped (${_.message})`)}}))}return c}async _fetchAssets(t,e,i,o,a){const n=[],d=(h,c)=>{let p=[];try{p=t.list_asset_urls(h)??[]}catch(s){console.warn(`amx-player: could not list asset urls (${s.message})`)}for(const s of p){const l=new URL(s,c).href;!a.has(l)&&!n.some(([r])=>r===l)&&n.push([l,s])}};d(e,o);for(const[h,c]of i)d(c,new URL(h,o).href);await Promise.all(n.map(async([h,c])=>{try{const p=await fetch(h);if(!p.ok)throw new Error(`HTTP ${p.status}`);const s=c.toLowerCase().endsWith(".svg")?await p.text():new Uint8Array(await p.arrayBuffer());a.set(h,{key:c,payload:s})}catch(p){console.warn(`amx-player: asset '${c}' skipped (${p.message})`)}}))}_buildWithAssets(t,e,i){const o=[],a=[];for(const{key:n,payload:d}of i.values())o.push(n),a.push(d);return typeof t.load_source_with_assets=="function"?t.load_source_with_assets(e,o,a):t.load_source(e)}async _maybeStartLoading(){const t=this.getAttribute("src");if(!(!t||this._state!=="idle")){if(this._state="loading",!("gpu"in navigator)){this._state="error",this._showVeil("This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).",!0);return}try{const e=this._profile(),[i,o]=await Promise.all([$(e),fetch(t).then(h=>{if(!h.ok)throw new Error(`HTTP ${h.status}`);return h.text()})]);if(this.getAttribute("src")!==t||(this._player=await i.create_player(this._canvas),this.getAttribute("src")!==t)||(typeof this._player.set_quality=="function"&&this._player.set_quality(this._quality()),await this._loadFonts(i),this.getAttribute("src")!==t))return;const a=await this._loadScene(this._player,o,t),n=a.diagnostics??[],d=n.filter(h=>h.severity==="error");if(!a.ok){this._state="error";const h=d[0]??n[0];this._showVeil(`Scene error${h?.line?` (line ${h.line})`:""}: ${h?.message??"build failed"}`,!0);return}this._duration=Math.max(a.duration_s,.05),this._canvas.width=Math.round(a.width||1280),this._canvas.height=Math.round(a.height||720),this._stage.style.aspectRatio=`${this._canvas.width} / ${this._canvas.height}`,this._configureCycle(),this._restTime=this._duration,this._time=this._duration,this._renderScene(),this._markers=Array.isArray(a.markers)?a.markers:[],this._skeleton.remove(),this._canvas.hidden=!1,this._stage.appendChild(this._canvas),this.hasAttribute("controls")?(this._buildControls(),this._setupGestures()):this._stage.appendChild(this._playbtn),this._stage.appendChild(this._veil),n.length>0&&console.warn(`amx-player: ${t} built with ${n.length} diagnostic(s)`,n[0]),this._state="ready",this._shouldAutoplay()?(this._loop=this.hasAttribute("loop"),this.play()):this.hasAttribute("controls")||this._playbtn.classList.add("show")}catch(e){this._state="error",this._showVeil(`Failed to load scene: ${e?.message??e}`,!0)}}}_buildControls(){const t=document.createElement("div");t.className="strip",t.tabIndex=0,t.setAttribute("role","slider"),t.setAttribute("aria-label","Timeline"),t.setAttribute("aria-valuemin","0"),t.setAttribute("aria-valuemax",String(this._duration));const e=document.createElement("div");e.className="fill";const i=document.createElement("div");i.className="cursor";const o=document.createDocumentFragment();for(const s of this._markers){const l=this._duration>0?Math.min(s.t/this._duration,1):0;if(s.kind==="transition"&&s.dur>0){const r=document.createElement("div");r.className="span",r.style.left=`${l*100}%`,r.style.width=`${Math.min(s.dur/this._duration,1-l)*100}%`,o.appendChild(r)}else{const r=document.createElement("div");r.className=s.kind==="scene"?"diamond":"tick",r.style.left=`${l*100}%`,o.appendChild(r)}}const a=document.createElement("span");a.className="chip";const n=document.createElement("button");n.className="speed",n.textContent="1\xD7",n.setAttribute("aria-label","Playback speed 1\xD7"),n.addEventListener("pointerdown",s=>s.stopPropagation()),n.addEventListener("click",s=>{s.stopPropagation();const l=[1,1.5,2,.5];this._rate=l[(l.indexOf(this._rate)+1)%l.length],n.textContent=`${this._rate}\xD7`,n.setAttribute("aria-label",`Playback speed ${this._rate}\xD7`)}),t.append(e,o,i,a,n),this.shadowRoot.append(t);const d=s=>{for(const r of this._markers){if(Math.abs(s-r.t)<=.2)return r.t;if(r.kind==="transition"&&r.dur>0&&Math.abs(s-(r.t+r.dur))<=.2)return r.t+r.dur}return null},h=s=>{const l=t.getBoundingClientRect();if(l.width<=0||!Number.isFinite(s.clientX))return this._time;const _=Math.min(Math.max((s.clientX-l.left)/l.width,0),1)*this._duration;return d(_)??_},c=s=>{this._time=h(s),this._restTime=this._time,this._renderScene(),this._syncControls()};t.addEventListener("pointerenter",s=>{s.pointerType!=="mouse"||this._suppressHoverPeek||(this._peeking=!0,this._resumeOnLeave=this._playing,t.classList.add("peeking"),c(s))}),t.addEventListener("pointermove",s=>{s.pointerType==="mouse"&&(this._peeking||this._scrubbing?c(s):i.style.left=`${(s.clientX-t.getBoundingClientRect().left)/t.getBoundingClientRect().width*100}%`)}),t.addEventListener("pointerdown",s=>{try{t.setPointerCapture(s.pointerId)}catch{}this._downX=s.clientX,this._scrubbing=!0,s.pointerType==="mouse"&&!this._peeking&&!this._suppressHoverPeek&&(this._peeking=!0,this._resumeOnLeave=this._playing,t.classList.add("peeking")),c(s),s.stopPropagation()}),t.addEventListener("pointermove",s=>{!this._scrubbing&&!this._peeking||c(s)});const p=s=>{const l=this._scrubbing;this._scrubbing=!1,l&&Math.abs(s.clientX-this._downX)<=4&&(s.pointerType==="mouse"?this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play(),this._suppressHoverPeek=!0,this._peeking=!1,t.classList.remove("peeking")):this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play()))};t.addEventListener("pointerup",p),t.addEventListener("pointercancel",p),t.addEventListener("pointerleave",s=>{if(s.pointerType!=="mouse")return;t.classList.remove("peeking");const l=this._peeking&&this._resumeOnLeave&&!this._latched;this._peeking=!1,this._resumeOnLeave=!1,this._suppressHoverPeek=!1,l?this.play():this._renderScene(),this._syncControls()}),t.addEventListener("keydown",s=>{const l=s.key==="ArrowLeft"?-1:s.key==="ArrowRight"?1:0;l!==0?(s.preventDefault(),s.stopPropagation(),this.seek(this._nextLandmark(l))):s.key==="Home"?(s.preventDefault(),this.seek(0)):(s.key===" "||s.key==="k")&&(s.preventDefault(),s.stopPropagation(),this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play()))}),this._controls={strip:t,fill:e,cursor:i,chip:a,speed:n},this._syncControls()}_nextLandmark(t){const e=[];for(const a of this._markers)e.push(a.t),a.kind==="transition"&&a.dur>0&&e.push(a.t+a.dur);e.sort((a,n)=>a-n);const i=.001;return t>0?e.find(n=>n>this._time+i)??Math.min(this._time+1,this._duration):[...e].reverse().find(a=>a<this._time-i)??Math.max(this._time-1,0)}_syncControls(){const t=this._controls;if(!t)return;const e=this._duration>0?this._time/this._duration:0;t.fill.style.transform=`scaleX(${Math.min(e,1)})`,t.chip.textContent=`${this._time.toFixed(1)} / ${this._duration.toFixed(1)}`,t.strip.setAttribute("aria-valuenow",this._time.toFixed(1))}_setupGestures(){this._canvas.addEventListener("pointerdown",t=>{this._controls&&(this._playing?(this._latched=!0,this.pause()):this._latched||this.play(),t.preventDefault())}),this._canvas.addEventListener("keydown",t=>{this._controls&&(t.key===" "||t.key==="k")&&(t.preventDefault(),this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play()))})}play(){this._state==="ready"&&this._player?.has_document()&&(!this._looping()&&this._time>=this._duration&&(this._time=0),this._playing=!0,this._playbtn.classList.remove("show"),this._syncControls(),m.add(this),T())}pause(){this._playing=!1,m.delete(this),this._state==="ready"&&((!this._controls||!this.hasAttribute("autoplay"))&&this._playbtn.classList.add("show"),this._syncControls())}seek(t){this._state==="ready"&&(this._time=Math.min(Math.max(t,0),this._duration),this._restTime=this._time,this._renderScene(),this._syncControls())}advance(t){if(!this._playing||!this._visible||this._scrubbing||this._peeking)return!1;const e=this._looping(),i=e?this._cycle:this._duration;if(this._time+=t*(this._rate??1),this._time>=i)if(e)this._time%=i;else return this._time=this._duration,this._restTime=this._duration,this.pause(),this._renderScene(),!1;return this._renderScene(),this._controls&&this._syncControls(),!0}_renderScene(){if(this._player)try{const t=this._playing?Math.min(this._time,this._duration):this._restTime;this._playing&&(this._restTime=t),this._player.render_frame(t);const e=this._loopAlpha();e!==this._lastAlpha&&(this._canvas.style.opacity=String(e),this._lastAlpha=e)}catch(t){console.warn("amx-player: render failed",t)}}}customElements.get("amx-player")||customElements.define("amx-player",x);export{x as AmxPlayerElement};
