const S=[...document.querySelectorAll("script[type=module]")].find(p=>(p.src||"").includes("amx-player.js")),T=(()=>{const p=S?.src||"",t=p.lastIndexOf("/amx-player.js");return t>=0?p.slice(0,t):new URL(".",import.meta.url).href.replace(/\/$/,"")})();function C(p){const t=p!=="full",e=S?.getAttribute("data-runtime-base"),s=e?new URL(e,document.baseURI).href.replace(/\/$/,""):`${T}/..`,n=s.split("/").pop();let i,a;n==="pkg-slim"?(i=s,a=s.slice(0,-5)):n==="pkg"?(a=s,i=`${s}-slim`):(i=`${s}/pkg-slim`,a=`${s}/pkg`);const c=t?i:a,o=t?a:i;return c===o?[c]:[c,o]}const P=window.matchMedia?.("(prefers-reduced-motion: reduce)").matches??!1,N=navigator.connection?.saveData===!0,U=.28,w={play:'<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>',pause:'<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4 2h3.6v14H4zM10.4 2H14v14h-3.6z"/></svg>',playBig:'<svg width="26" height="26" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>'},y=new Map;function q(p){const[t]=C(p);return y.has(t)||y.set(t,(async()=>{const e=C(p);let s;for(const[n,i]of e.entries())try{const a=await import(`${i}/animatix_web.js`);return await a.default(),await a.init_engine?.(),a}catch(a){s=a,y.delete(t),n+1<e.length&&console.info(`amx-player: no engine at ${i}, trying ${e[n+1]}`)}throw s})()),y.get(t)}const g=new Set;let k=!1,x=0;function $(p){const t=Math.min((p-x)/1e3,.1);x=p;let e=!1;for(const s of g)s.advance(t)&&(e=!0);e?requestAnimationFrame($):k=!1}function z(){k||(k=!0,x=performance.now(),requestAnimationFrame($))}const L={"16:9":16/9,"4:3":4/3,"1:1":1,"9:16":9/16};class M extends HTMLElement{static observedAttributes=["src","autoplay","loop","hold","controls","title","aspect","profile","quality"];constructor(){super(),this.attachShadow({mode:"open"}),this._state="idle",this._player=null,this._playing=!1,this._loop=!1,this._time=0,this._duration=0,this._holdSeconds=0,this._cycle=0,this._fade=0,this._restTime=0,this._lastAlpha=1,this._visible=!1,this._observer=null,this._initialized=!1,this._scrubbing=!1,this._rate=1,this._markers=[]}connectedCallback(){this._initialized||(this._initialized=!0,this._renderSkeleton(),this._setupObserver())}disconnectedCallback(){this._observer?.disconnect(),g.delete(this),this._playing=!1}attributeChangedCallback(t){if(t==="aspect"&&this._initialized&&(this._stage.style.aspectRatio=String(this._aspectRatio())),t==="hold"&&this._initialized&&this._duration>0&&this._configureCycle(),t==="quality"&&this._initialized&&this._player?.set_quality){try{this._player.set_quality(this._quality())}catch(e){console.warn(`amx-player: ${e.message}`)}this._resetForReload()}(t==="src"||t==="profile")&&this._initialized&&this._resetForReload()}_resetForReload(){this._state="idle",this._player=null,this._time=0,this._renderSkeleton(),this._maybeStartLoading()}_profile(){return this.getAttribute("profile")==="full"?"full":"slim"}_quality(){const t=this.getAttribute("quality")??"draft";return["draft","preview","production"].includes(t)?t:(console.warn(`amx-player: unknown quality '${t}', using draft`),"draft")}_aspectRatio(){const t=this.getAttribute("aspect");if(t&&L[t])return L[t];const[e,s]=(t||"").split(":").map(Number);return e>0&&s>0?e/s:16/9}_renderSkeleton(){this._stage=document.createElement("div"),this._stage.className="stage",this._stage.style.aspectRatio=String(this._aspectRatio());const t=this.getAttribute("title")||"",e=document.createElement("style");e.textContent=`
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
      /* The bar lives below the stage, in flow: it never covers the picture,
         so it stays visible permanently and the canvas tap maps straight to
         play/pause. */
      .bar {
        display: flex; align-items: center; gap: 8px;
        padding: 6px 12px 8px;
        border-top: 1px solid rgba(255,255,255,.07);
      }
      .bar button {
        width: 40px; height: 40px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #e8edf4; cursor: pointer; padding: 0;
        display: flex; align-items: center; justify-content: center;
        font: 12px ui-monospace, monospace;
      }
      .bar button:hover { background: rgba(255,255,255,.14); }
      .bar button:focus-visible { outline: 2px solid #f5b942; }
      .bar svg { display: block; }
      .track {
        flex: 1; height: 32px; display: flex; align-items: stretch;
        cursor: pointer; touch-action: none; border-radius: 6px;
      }
      .track:focus-visible { outline: 2px solid #f5b942; }
      .track .zone, .track .rest { position: relative; display: flex; align-items: center; }
      .track .rail {
        position: relative; height: 4px; width: 100%;
        border-radius: 2px; background: rgba(255,255,255,.24);
        transition: height .12s;
      }
      .track:hover .rail, .track.scrubbing .rail { height: 7px; }
      .track .rest .rail { background: rgba(255,255,255,.11); }
      .track .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        border-radius: 2px; background: #f5b942;
        transform-origin: left; transform: scaleX(0);
      }
      .track .thumb {
        position: absolute; top: 50%; width: 13px; height: 13px;
        border-radius: 50%; background: #f5b942;
        transform: translate(-50%, -50%); opacity: 0; transition: opacity .12s;
      }
      .track:hover .thumb, .track.scrubbing .thumb, .track:focus-visible .thumb {
        opacity: 1;
      }
      /* Timeline landmarks. Keyframes are small ticks; scene starts are
         taller diamonds; a transition is a hatched span on the rail. */
      .track .tick {
        position: absolute; top: 50%; width: 2px; height: 8px;
        transform: translate(-50%, -50%);
        background: rgba(255,255,255,.45); border-radius: 1px;
        pointer-events: none;
      }
      .track .diamond {
        position: absolute; top: 50%; width: 7px; height: 7px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 1px;
        pointer-events: none;
      }
      .track .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.4) 0 3px, transparent 3px 6px);
        border-radius: 2px; pointer-events: none;
      }
      .time {
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums; flex: none;
      }
      .speed { min-width: 44px; justify-content: center; color: #c7cfd9; }
      @media (pointer: coarse) {
        .track { height: 40px; }
        .bar button { width: 44px; height: 44px; }
      }
    `,this.shadowRoot.replaceChildren(e,this._stage),this._skeleton=document.createElement("div"),this._skeleton.className="skeleton",this._skeleton.textContent=t?`${t}`:"animatix scene",this._stage.appendChild(this._skeleton),this._canvas=document.createElement("canvas"),this._canvas.width=1280,this._canvas.height=720,this._canvas.hidden=!0,this._veil=document.createElement("div"),this._veil.className="veil",this._playbtn=document.createElement("button"),this._playbtn.className="playbtn",this._playbtn.setAttribute("aria-label","Play"),this._playbtn.innerHTML=w.playBig,this._playbtn.addEventListener("click",()=>this.play()),this._controls=null}_showVeil(t,e){this._veil.className=`veil show${e?" error":""}`,this._veil.textContent=t,this._veil.isConnected||this.shadowRoot.appendChild(this._veil)}_hideVeil(){this._veil.className="veil"}_looping(){return this.hasAttribute("loop")||this._loop===!0}_configureCycle(){const t=this.getAttribute("hold"),e=t===null?NaN:Number(t),s=Number.isFinite(e)&&e>=0?Math.min(e,30):.7;this._holdSeconds=s,this._cycle=this._duration+s,this._fade=s>0?Math.min(U,s/2,this._duration/4):0}_loopAlpha(){if(!this._playing||!this._looping()||this._fade<=0)return 1;if(this._time<this._fade)return this._time/this._fade;const t=this._cycle-this._fade;return this._time>t?Math.max(0,(this._cycle-this._time)/this._fade):1}_setupObserver(){this._observer=new IntersectionObserver(t=>{for(const e of t)this._visible=e.isIntersecting,this._visible&&this._state==="idle"&&this._maybeStartLoading(),!this._visible&&this._playing&&this.pause()},{rootMargin:"200px"}),this._observer.observe(this)}_shouldAutoplay(){return this.hasAttribute("autoplay")&&!P&&!N}async _loadFonts(t){const e=this.getAttribute("data-fonts");if(!(!e||!this._player.add_font))for(const s of e.split(/\s+/).filter(Boolean))try{const n=await fetch(new URL(s,document.baseURI));if(!n.ok)throw new Error(`HTTP ${n.status}`);this._player.add_font(new Uint8Array(await n.arrayBuffer()))}catch(n){console.warn(`amx-player: font '${s}' skipped (${n.message})`)}}async _loadScene(t,e,s){const n=new URL(s,document.baseURI).href,i=typeof t.add_module=="function",a=new Map,c=new Set,o=new Map;let u=null;for(let d=0;d<24;d+=1){for(const[h,m]of a)t.add_module(h,m);await this._fetchAssets(t,e,a,n,o),u=this._buildWithAssets(t,e,o);const _=i?u?.missing_imports??[]:[],f=_.filter(h=>!a.has(h)&&!c.has(h));if(!_.length||!f.length)break;await Promise.all(f.map(async h=>{try{const m=await fetch(new URL(h,n));if(!m.ok)throw new Error(`HTTP ${m.status}`);a.set(h,await m.text())}catch(m){c.add(h),console.warn(`amx-player: import '${h}' skipped (${m.message})`)}}))}return u}async _fetchAssets(t,e,s,n,i){const a=[],c=(o,u)=>{let d=[];try{d=t.list_asset_urls(o)??[]}catch(_){console.warn(`amx-player: could not list asset urls (${_.message})`)}for(const _ of d){const f=new URL(_,u).href;!i.has(f)&&!a.some(([h])=>h===f)&&a.push([f,_])}};c(e,n);for(const[o,u]of s)c(u,new URL(o,n).href);await Promise.all(a.map(async([o,u])=>{try{const d=await fetch(o);if(!d.ok)throw new Error(`HTTP ${d.status}`);const _=u.toLowerCase().endsWith(".svg")?await d.text():new Uint8Array(await d.arrayBuffer());i.set(o,{key:u,payload:_})}catch(d){console.warn(`amx-player: asset '${u}' skipped (${d.message})`)}}))}_buildWithAssets(t,e,s){const n=[],i=[];for(const{key:a,payload:c}of s.values())n.push(a),i.push(c);return typeof t.load_source_with_assets=="function"?t.load_source_with_assets(e,n,i):t.load_source(e)}async _maybeStartLoading(){const t=this.getAttribute("src");if(!(!t||this._state!=="idle")){if(this._state="loading",!("gpu"in navigator)){this._state="error",this._showVeil("This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).",!0);return}try{const e=this._profile(),[s,n]=await Promise.all([q(e),fetch(t).then(o=>{if(!o.ok)throw new Error(`HTTP ${o.status}`);return o.text()})]);if(this.getAttribute("src")!==t||(this._player=await s.create_player(this._canvas),this.getAttribute("src")!==t)||(typeof this._player.set_quality=="function"&&this._player.set_quality(this._quality()),await this._loadFonts(s),this.getAttribute("src")!==t))return;const i=await this._loadScene(this._player,n,t),a=i.diagnostics??[],c=a.filter(o=>o.severity==="error");if(!i.ok){this._state="error";const o=c[0]??a[0];this._showVeil(`Scene error${o?.line?` (line ${o.line})`:""}: ${o?.message??"build failed"}`,!0);return}this._duration=Math.max(i.duration_s,.05),this._canvas.width=Math.round(i.width||1280),this._canvas.height=Math.round(i.height||720),this._stage.style.aspectRatio=`${this._canvas.width} / ${this._canvas.height}`,this._configureCycle(),this._restTime=this._duration,this._renderScene(),this._markers=Array.isArray(i.markers)?i.markers:[],this._skeleton.remove(),this._canvas.hidden=!1,this._stage.appendChild(this._canvas),this.hasAttribute("controls")?(this._buildControls(),this._setupGestures()):this._stage.appendChild(this._playbtn),this._stage.appendChild(this._veil),a.length>0&&console.warn(`amx-player: ${t} built with ${a.length} diagnostic(s)`,a[0]),this._state="ready",this._shouldAutoplay()?(this._loop=this.hasAttribute("loop"),this.play()):this.hasAttribute("controls")||this._playbtn.classList.add("show")}catch(e){this._state="error",this._showVeil(`Failed to load scene: ${e?.message??e}`,!0)}}}_buildControls(){const t=document.createElement("div");t.className="bar";const e=document.createElement("button");e.setAttribute("aria-label","Play"),e.innerHTML=w.play,e.addEventListener("click",r=>{r.stopPropagation(),this._playing?this.pause():this.play()});const s=document.createElement("div");s.className="track",s.tabIndex=0,s.setAttribute("role","slider"),s.setAttribute("aria-label","Seek"),s.setAttribute("aria-valuemin","0"),s.setAttribute("aria-valuemax",String(this._duration));const n=document.createElement("div");n.className="zone",n.style.flexGrow=String(this._duration);const i=document.createElement("div");i.className="rail";const a=document.createElement("div");a.className="fill";const c=document.createElement("div");c.className="thumb",i.append(a,c),n.append(i);const o=document.createElement("div");o.className="rest",o.style.flexGrow=String(this._holdSeconds);const u=document.createElement("div");u.className="rail",o.append(u),this._holdSeconds>0||(o.style.display="none");const d=n.querySelector(".rail");for(const r of this._markers){const l=this._duration>0?Math.min(r.t/this._duration,1):0;if(r.kind==="transition"&&r.dur>0){const b=document.createElement("div");b.className="span",b.style.left=`${l*100}%`,b.style.width=`${Math.min(r.dur/this._duration,1-l)*100}%`,d.appendChild(b)}else{const b=document.createElement("div");b.className=r.kind==="scene"?"diamond":"tick",b.style.left=`${l*100}%`,d.appendChild(b)}}const _=document.createElement("span");_.className="time";const f=[1,1.5,2,.5];this._rate=1;const h=document.createElement("button");h.className="speed",h.textContent="1\xD7",h.setAttribute("aria-label","Playback speed 1\xD7"),h.addEventListener("click",r=>{r.stopPropagation();const l=f[(f.indexOf(this._rate)+1)%f.length];this._rate=l,h.textContent=`${l}\xD7`,h.setAttribute("aria-label",`Playback speed ${l}\xD7`)}),s.append(n,o),t.append(e,s,_,h),this.shadowRoot.append(t);const m=.2,R=r=>{for(const l of this._markers){if(Math.abs(r-l.t)<=m)return l.t;if(l.kind==="transition"&&l.dur>0&&Math.abs(r-(l.t+l.dur))<=m)return l.t+l.dur}return null},v=r=>{const l=n.getBoundingClientRect();if(l.width<=0)return;const E=Math.min(Math.max((r.clientX-l.left)/l.width,0),1)*this._duration;this._time=R(E)??E,this._restTime=this._time,this._renderScene(),this._syncControls()};s.addEventListener("pointerdown",r=>{try{s.setPointerCapture(r.pointerId)}catch{}s.classList.add("scrubbing"),this._scrubbing=!0,v(r),r.stopPropagation()}),s.addEventListener("pointermove",r=>{this._scrubbing&&v(r)});const A=r=>{this._scrubbing&&(this._scrubbing=!1,s.classList.remove("scrubbing"),v(r))};s.addEventListener("pointerup",A),s.addEventListener("pointercancel",A),s.addEventListener("keydown",r=>{const l=r.key==="ArrowLeft"?-1:r.key==="ArrowRight"?1:0;l!==0?(r.preventDefault(),r.stopPropagation(),this.seek(this._nextLandmark(l))):r.key==="Home"?(r.preventDefault(),this.seek(0)):(r.key===" "||r.key==="k")&&(r.preventDefault(),r.stopPropagation(),this._playing?this.pause():this.play())}),this._controls={bar:t,btn:e,track:s,zone:n,fill:a,thumb:c,time:_,speed:h,icons:w},this._syncControls()}_nextLandmark(t){const e=[];for(const i of this._markers)e.push(i.t),i.kind==="transition"&&i.dur>0&&e.push(i.t+i.dur);e.sort((i,a)=>i-a);const s=.001;return t>0?e.find(a=>a>this._time+s)??Math.min(this._time+1,this._duration):[...e].reverse().find(i=>i<this._time-s)??Math.max(this._time-1,0)}_syncControls(){const t=this._controls;if(!t)return;const e=Math.min(this._time,this._duration),s=this._duration>0?e/this._duration:0;t.fill.style.transform=`scaleX(${s})`,t.thumb.style.left=`${s*100}%`,t.time.textContent=`${e.toFixed(1)} / ${this._duration.toFixed(1)}`,t.track.setAttribute("aria-valuenow",e.toFixed(1));const n=this._playing?t.icons.pause:t.icons.play;t.btn.dataset.icon!==n&&(t.btn.dataset.icon=n,t.btn.innerHTML=n,t.btn.setAttribute("aria-label",this._playing?"Pause":"Play"))}_setupGestures(){this._canvas.addEventListener("pointerdown",()=>{this._controls&&(this._playing?this.pause():this.play())}),this._canvas.addEventListener("keydown",t=>{this._controls&&(t.key===" "||t.key==="k")&&(t.preventDefault(),this._playing?this.pause():this.play())})}play(){this._state==="ready"&&this._player?.has_document()&&(!this._looping()&&this._time>=this._duration&&(this._time=0),this._playing=!0,this._playbtn.classList.remove("show"),this._syncControls(),g.add(this),z())}pause(){this._playing=!1,g.delete(this),this._state==="ready"&&((!this._controls||!this.hasAttribute("autoplay"))&&this._playbtn.classList.add("show"),this._syncControls())}seek(t){this._state==="ready"&&(this._time=Math.min(Math.max(t,0),this._duration),this._restTime=this._time,this._renderScene(),this._syncControls())}advance(t){if(!this._playing||!this._visible||this._scrubbing)return!1;const e=this._looping(),s=e?this._cycle:this._duration;if(this._time+=t*(this._rate??1),this._time>=s)if(e)this._time%=s;else return this._time=this._duration,this._restTime=this._duration,this.pause(),this._renderScene(),!1;return this._renderScene(),this._controls&&this._syncControls(),!0}_renderScene(){if(this._player)try{const t=this._playing?Math.min(this._time,this._duration):this._restTime;this._playing&&(this._restTime=t),this._player.render_frame(t);const e=this._loopAlpha();e!==this._lastAlpha&&(this._canvas.style.opacity=String(e),this._lastAlpha=e)}catch(t){console.warn("amx-player: render failed",t)}}}customElements.get("amx-player")||customElements.define("amx-player",M);export{M as AmxPlayerElement};
