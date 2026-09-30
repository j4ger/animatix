const k=[...document.querySelectorAll("script[type=module]")].find(p=>(p.src||"").includes("amx-player.js")),T=(()=>{const p=k?.src||"",t=p.lastIndexOf("/amx-player.js");return t>=0?p.slice(0,t):new URL(".",import.meta.url).href.replace(/\/$/,"")})();function x(p){const t=p!=="full",e=k?.getAttribute("data-runtime-base"),i=e?new URL(e,document.baseURI).href.replace(/\/$/,""):`${T}/..`,r=i.split("/").pop();let a,n;r==="pkg-slim"?(a=i,n=i.slice(0,-5)):r==="pkg"?(n=i,a=`${i}-slim`):(a=`${i}/pkg-slim`,n=`${i}/pkg`);const c=t?a:n,l=t?n:a;return c===l?[c]:[c,l]}const $=window.matchMedia?.("(prefers-reduced-motion: reduce)").matches??!1,R=navigator.connection?.saveData===!0,M=.28,P={playBig:'<svg width="26" height="26" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>'},g=new Map;function N(p){const[t]=x(p);return g.has(t)||g.set(t,(async()=>{const e=x(p);let i;for(const[r,a]of e.entries())try{const n=await import(`${a}/animatix_web.js`);return await n.default(),await n.init_engine?.(),n}catch(n){i=n,g.delete(t),r+1<e.length&&console.info(`amx-player: no engine at ${a}, trying ${e[r+1]}`)}throw i})()),g.get(t)}const f=new Set;let v=!1,w=0;const A=[1,.85,.72,.6,.5],O=24,H=18.5;let m=0,y=0,b=0;function S(p){const t=Math.max(0,Math.min(A.length-1,p));if(t!==m){m=t;for(const e of f)e._applyRenderScale()}}function E(p){const t=p-w,e=Math.min(t/1e3,.1);w=p;let i=!1;for(const r of f)r.advance(e)&&(i=!0);i?(t>O?(y+=1,b=0,y>=8&&(S(m+1),y=0)):t<H&&(b+=1,y=0,b>=90&&(S(m-1),b=0)),requestAnimationFrame(E)):v=!1}function F(){v||(v=!0,w=performance.now(),requestAnimationFrame(E))}const C={"16:9":16/9,"4:3":4/3,"1:1":1,"9:16":9/16};class L extends HTMLElement{static observedAttributes=["src","autoplay","loop","hold","controls","title","aspect","profile","quality"];constructor(){super(),this.attachShadow({mode:"open"}),this._state="idle",this._player=null,this._playing=!1,this._loop=!1,this._time=0,this._duration=0,this._holdSeconds=0,this._cycle=0,this._fade=0,this._restTime=0,this._lastAlpha=1,this._visible=!1,this._observer=null,this._renderScaleObserver=null,this._initialized=!1,this._scrubbing=!1,this._peeking=!1,this._latched=!1,this._resumeOnLeave=!1,this._suppressHoverPeek=!1,this._downX=0,this._rate=1,this._markers=[]}connectedCallback(){this._initialized||(this._initialized=!0,this._renderSkeleton(),this._setupObserver())}disconnectedCallback(){this._observer?.disconnect(),this._renderScaleObserver?.disconnect(),f.delete(this),this._playing=!1}attributeChangedCallback(t){if(t==="aspect"&&this._initialized&&(this._stage.style.aspectRatio=String(this._aspectRatio())),t==="hold"&&this._initialized&&this._duration>0&&this._configureCycle(),t==="quality"&&this._initialized&&this._player?.set_quality){try{this._player.set_quality(this._quality())}catch(e){console.warn(`amx-player: ${e.message}`)}this._resetForReload()}(t==="src"||t==="profile")&&this._initialized&&this._resetForReload()}_resetForReload(){this._state="idle",this._player=null,this._time=0,this._renderSkeleton(),this._maybeStartLoading()}_profile(){return this.getAttribute("profile")==="full"?"full":"slim"}_quality(){const t=this.getAttribute("quality")??"draft";return["draft","preview","production"].includes(t)?t:(console.warn(`amx-player: unknown quality '${t}', using draft`),"draft")}_aspectRatio(){const t=this.getAttribute("aspect");if(t&&C[t])return C[t];const[e,i]=(t||"").split(":").map(Number);return e>0&&i>0?e/i:16/9}_renderSkeleton(){this._stage=document.createElement("div"),this._stage.className="stage",this._stage.style.aspectRatio=String(this._aspectRatio());const t=this.getAttribute("title")||"",e=document.createElement("style");e.textContent=`
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
    `,this.shadowRoot.replaceChildren(e,this._stage),this._skeleton=document.createElement("div"),this._skeleton.className="skeleton",this._skeleton.textContent=t?`${t}`:"animatix scene",this._stage.appendChild(this._skeleton),this._canvas=document.createElement("canvas"),this._canvas.width=1280,this._canvas.height=720,this._canvas.hidden=!0,this._veil=document.createElement("div"),this._veil.className="veil",this._playbtn=document.createElement("button"),this._playbtn.className="playbtn",this._playbtn.setAttribute("aria-label","Play"),this._playbtn.innerHTML=P.playBig,this._playbtn.addEventListener("click",()=>this.play()),this._controls=null}_showVeil(t,e){this._veil.className=`veil show${e?" error":""}`,this._veil.textContent=t,this._veil.isConnected||this.shadowRoot.appendChild(this._veil)}_hideVeil(){this._veil.className="veil"}_looping(){return this.hasAttribute("loop")||this._loop===!0}_configureCycle(){const t=this.getAttribute("hold"),e=t===null?NaN:Number(t),i=Number.isFinite(e)&&e>=0?Math.min(e,30):.7;this._holdSeconds=i,this._cycle=this._duration+i,this._fade=i>0?Math.min(M,i/2,this._duration/4):0}_loopAlpha(){if(!this._playing||!this._looping()||this._fade<=0)return 1;if(this._time<this._fade)return this._time/this._fade;const t=this._cycle-this._fade;return this._time>t?Math.max(0,(this._cycle-this._time)/this._fade):1}_setupObserver(){this._observer=new IntersectionObserver(t=>{for(const e of t)this._visible=e.isIntersecting,this._visible&&this._state==="idle"&&this._maybeStartLoading(),!this._visible&&this._playing&&this.pause()},{rootMargin:"200px"}),this._observer.observe(this)}_shouldAutoplay(){return this.hasAttribute("autoplay")&&!$&&!R}async _loadFonts(t){const e=this.getAttribute("data-fonts");if(!(!e||!this._player.add_font))for(const i of e.split(/\s+/).filter(Boolean))try{const r=await fetch(new URL(i,document.baseURI));if(!r.ok)throw new Error(`HTTP ${r.status}`);this._player.add_font(new Uint8Array(await r.arrayBuffer()))}catch(r){console.warn(`amx-player: font '${i}' skipped (${r.message})`)}}async _loadScene(t,e,i){const r=new URL(i,document.baseURI).href,a=typeof t.add_module=="function",n=new Map,c=new Set,l=new Map;let d=null;for(let u=0;u<24;u+=1){for(const[o,_]of n)t.add_module(o,_);await this._fetchAssets(t,e,n,r,l),d=this._buildWithAssets(t,e,l);const s=a?d?.missing_imports??[]:[],h=s.filter(o=>!n.has(o)&&!c.has(o));if(!s.length||!h.length)break;await Promise.all(h.map(async o=>{try{const _=await fetch(new URL(o,r));if(!_.ok)throw new Error(`HTTP ${_.status}`);n.set(o,await _.text())}catch(_){c.add(o),console.warn(`amx-player: import '${o}' skipped (${_.message})`)}}))}return d}async _fetchAssets(t,e,i,r,a){const n=[],c=(l,d)=>{let u=[];try{u=t.list_asset_urls(l)??[]}catch(s){console.warn(`amx-player: could not list asset urls (${s.message})`)}for(const s of u){const h=new URL(s,d).href;!a.has(h)&&!n.some(([o])=>o===h)&&n.push([h,s])}};c(e,r);for(const[l,d]of i)c(d,new URL(l,r).href);await Promise.all(n.map(async([l,d])=>{try{const u=await fetch(l);if(!u.ok)throw new Error(`HTTP ${u.status}`);const s=d.toLowerCase().endsWith(".svg")?await u.text():new Uint8Array(await u.arrayBuffer());a.set(l,{key:d,payload:s})}catch(u){console.warn(`amx-player: asset '${d}' skipped (${u.message})`)}}))}_buildWithAssets(t,e,i){const r=[],a=[];for(const{key:n,payload:c}of i.values())r.push(n),a.push(c);return typeof t.load_source_with_assets=="function"?t.load_source_with_assets(e,r,a):t.load_source(e)}async _maybeStartLoading(){const t=this.getAttribute("src");if(!(!t||this._state!=="idle")){if(this._state="loading",!("gpu"in navigator)){this._state="error",this._showVeil("This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).",!0);return}try{const e=this._profile(),[i,r]=await Promise.all([N(e),fetch(t).then(l=>{if(!l.ok)throw new Error(`HTTP ${l.status}`);return l.text()})]);if(this.getAttribute("src")!==t||(this._player=await i.create_player(this._canvas),this.getAttribute("src")!==t)||(typeof this._player.set_quality=="function"&&this._player.set_quality(this._quality()),await this._loadFonts(i),this.getAttribute("src")!==t))return;const a=await this._loadScene(this._player,r,t),n=a.diagnostics??[],c=n.filter(l=>l.severity==="error");if(!a.ok){this._state="error";const l=c[0]??n[0];this._showVeil(`Scene error${l?.line?` (line ${l.line})`:""}: ${l?.message??"build failed"}`,!0);return}this._duration=Math.max(a.duration_s,.05),this._canvas.width=Math.round(a.width||1280),this._canvas.height=Math.round(a.height||720),this._stage.style.aspectRatio=`${this._canvas.width} / ${this._canvas.height}`,this._configureCycle(),this._restTime=this._duration,this._time=this._duration,this._renderScene(),this._markers=Array.isArray(a.markers)?a.markers:[],this._skeleton.remove(),this._canvas.hidden=!1,this._stage.appendChild(this._canvas),this._applyRenderScale(),this._renderScaleObserver=new ResizeObserver(()=>this._applyRenderScale()),this._renderScaleObserver.observe(this._stage),this.hasAttribute("controls")?(this._buildControls(),this._setupGestures()):this._stage.appendChild(this._playbtn),this._stage.appendChild(this._veil),n.length>0&&console.warn(`amx-player: ${t} built with ${n.length} diagnostic(s)`,n[0]),this._state="ready",this._shouldAutoplay()?(this._loop=this.hasAttribute("loop"),this.play()):this.hasAttribute("controls")||this._playbtn.classList.add("show")}catch(e){this._state="error",this._showVeil(`Failed to load scene: ${e?.message??e}`,!0)}}}_buildControls(){const t=document.createElement("div");t.className="strip",t.tabIndex=0,t.setAttribute("role","slider"),t.setAttribute("aria-label","Timeline"),t.setAttribute("aria-valuemin","0"),t.setAttribute("aria-valuemax",String(this._duration));const e=document.createElement("div");e.className="fill";const i=document.createElement("div");i.className="cursor";const r=document.createDocumentFragment();for(const s of this._markers){const h=this._duration>0?Math.min(s.t/this._duration,1):0;if(s.kind==="transition"&&s.dur>0){const o=document.createElement("div");o.className="span",o.style.left=`${h*100}%`,o.style.width=`${Math.min(s.dur/this._duration,1-h)*100}%`,r.appendChild(o)}else{const o=document.createElement("div");o.className=s.kind==="scene"?"diamond":"tick",o.style.left=`${h*100}%`,r.appendChild(o)}}const a=document.createElement("span");a.className="chip";const n=document.createElement("button");n.className="speed",n.textContent="1\xD7",n.setAttribute("aria-label","Playback speed 1\xD7"),n.addEventListener("pointerdown",s=>s.stopPropagation()),n.addEventListener("click",s=>{s.stopPropagation();const h=[1,1.5,2,.5];this._rate=h[(h.indexOf(this._rate)+1)%h.length],n.textContent=`${this._rate}\xD7`,n.setAttribute("aria-label",`Playback speed ${this._rate}\xD7`)}),t.append(e,r,i,a,n),this.shadowRoot.append(t);const c=s=>{for(const o of this._markers){if(Math.abs(s-o.t)<=.2)return o.t;if(o.kind==="transition"&&o.dur>0&&Math.abs(s-(o.t+o.dur))<=.2)return o.t+o.dur}return null},l=s=>{const h=t.getBoundingClientRect();if(h.width<=0||!Number.isFinite(s.clientX))return this._time;const _=Math.min(Math.max((s.clientX-h.left)/h.width,0),1)*this._duration;return c(_)??_},d=s=>{this._time=l(s),this._restTime=this._time,this._renderScene(),this._syncControls()};t.addEventListener("pointerenter",s=>{s.pointerType!=="mouse"||this._suppressHoverPeek||(this._peeking=!0,this._resumeOnLeave=this._playing,t.classList.add("peeking"),d(s))}),t.addEventListener("pointermove",s=>{s.pointerType==="mouse"&&(this._peeking||this._scrubbing?d(s):i.style.left=`${(s.clientX-t.getBoundingClientRect().left)/t.getBoundingClientRect().width*100}%`)}),t.addEventListener("pointerdown",s=>{try{t.setPointerCapture(s.pointerId)}catch{}this._downX=s.clientX,this._scrubbing=!0,s.pointerType==="mouse"&&!this._peeking&&!this._suppressHoverPeek&&(this._peeking=!0,this._resumeOnLeave=this._playing,t.classList.add("peeking")),d(s),s.stopPropagation()}),t.addEventListener("pointermove",s=>{!this._scrubbing&&!this._peeking||d(s)});const u=s=>{const h=this._scrubbing;this._scrubbing=!1,h&&Math.abs(s.clientX-this._downX)<=4&&(s.pointerType==="mouse"?this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play(),this._suppressHoverPeek=!0,this._peeking=!1,t.classList.remove("peeking")):this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play()))};t.addEventListener("pointerup",u),t.addEventListener("pointercancel",u),t.addEventListener("pointerleave",s=>{if(s.pointerType!=="mouse")return;t.classList.remove("peeking");const h=this._peeking&&this._resumeOnLeave&&!this._latched;this._peeking=!1,this._resumeOnLeave=!1,this._suppressHoverPeek=!1,h?this.play():this._renderScene(),this._syncControls()}),t.addEventListener("keydown",s=>{const h=s.key==="ArrowLeft"?-1:s.key==="ArrowRight"?1:0;h!==0?(s.preventDefault(),s.stopPropagation(),this.seek(this._nextLandmark(h))):s.key==="Home"?(s.preventDefault(),this.seek(0)):(s.key===" "||s.key==="k")&&(s.preventDefault(),s.stopPropagation(),this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play()))}),this._controls={strip:t,fill:e,cursor:i,chip:a,speed:n},this._syncControls()}_nextLandmark(t){const e=[];for(const a of this._markers)e.push(a.t),a.kind==="transition"&&a.dur>0&&e.push(a.t+a.dur);e.sort((a,n)=>a-n);const i=.001;return t>0?e.find(n=>n>this._time+i)??Math.min(this._time+1,this._duration):[...e].reverse().find(a=>a<this._time-i)??Math.max(this._time-1,0)}_syncControls(){const t=this._controls;if(!t)return;const e=Math.min(this._time,this._duration),i=this._duration>0?e/this._duration:0;t.fill.style.transform=`scaleX(${i})`,t.chip.textContent=`${e.toFixed(1)} / ${this._duration.toFixed(1)}`,t.strip.setAttribute("aria-valuenow",e.toFixed(1))}_setupGestures(){this._canvas.addEventListener("pointerdown",t=>{this._controls&&(this._playing?(this._latched=!0,this.pause()):this._latched||this.play(),t.preventDefault())}),this._canvas.addEventListener("keydown",t=>{this._controls&&(t.key===" "||t.key==="k")&&(t.preventDefault(),this._playing?(this._latched=!0,this.pause()):(this._latched=!1,this.play()))})}play(){this._state==="ready"&&this._player?.has_document()&&(!this._looping()&&this._time>=this._duration&&(this._time=0),this._playing=!0,this._applyRenderScale(),this._playbtn.classList.remove("show"),this._syncControls(),f.add(this),F())}pause(){this._playing=!1,f.delete(this),this._state==="ready"&&((!this._controls||!this.hasAttribute("autoplay"))&&this._playbtn.classList.add("show"),this._syncControls())}seek(t){this._state==="ready"&&(this._time=Math.min(Math.max(t,0),this._duration),this._restTime=this._time,this._renderScene(),this._syncControls())}advance(t){if(!this._playing||!this._visible||this._scrubbing||this._peeking)return!1;const e=this._looping(),i=e?this._cycle:this._duration;if(this._time+=t*(this._rate??1),this._time>=i)if(e)this._time%=i;else return this._time=this._duration,this._restTime=this._duration,this.pause(),this._renderScene(),!1;return this._renderScene(),this._controls&&this._syncControls(),!0}_applyRenderScale(){const t=this._player;if(!t?.set_render_scale||!t.scene_width)return;const e=t.scene_width()||this._canvas.width,i=t.scene_height()||this._canvas.height,r=this._canvas.clientWidth||this._stage.clientWidth||0,a=this._canvas.clientHeight||this._stage.clientHeight||0,n=window.devicePixelRatio||1,c=r>0&&a>0&&e>0&&i>0?Math.min(1,r*n/e,a*n/i):1,l=Math.min(1,Math.max(.25,c*A[m]));this._renderScale=t.set_render_scale(l)}_renderScene(){if(this._player)try{const t=this._playing?Math.min(this._time,this._duration):this._restTime;this._playing&&(this._restTime=t),this._player.render_frame(t);const e=this._loopAlpha();e!==this._lastAlpha&&(this._canvas.style.opacity=String(e),this._lastAlpha=e)}catch(t){console.warn("amx-player: render failed",t)}}}customElements.get("amx-player")||customElements.define("amx-player",L);export{L as AmxPlayerElement};
