const at=60,j=/[?&]amxdebug\b/.test(location.search),N=[...document.querySelectorAll("script[type=module]")].find(c=>(c.src||"").includes("amx-player.js")),K=(()=>{const c=N?.src||"",t=c.lastIndexOf("/amx-player.js");return t>=0?c.slice(0,t):new URL(".",import.meta.url).href.replace(/\/$/,"")})();function D(c){const t=c!=="full",e=N?.getAttribute("data-runtime-base"),s=e?new URL(e,document.baseURI).href.replace(/\/$/,""):`${K}/..`,i=s.split("/").pop();let a,r;i==="pkg-slim"?(a=s,r=s.slice(0,-5)):i==="pkg"?(r=s,a=`${s}-slim`):(a=`${s}/pkg-slim`,r=`${s}/pkg`);const o=t?a:r,p=t?r:a;return o===p?[o]:[o,p]}const H=window.matchMedia?.("(prefers-reduced-motion: reduce)").matches??!1,z=navigator.connection?.saveData===!0,V=.28,k={playBig:'<svg width="26" height="26" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>',play:'<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>',pause:'<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 3h3v12h-3zm6 0h3v12h-3z"/></svg>',replay:'<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M9 3a6 6 0 1 0 6 6h-2a4 4 0 1 1-4-4V2l4 3-4 3V5z"/></svg>',stepPrev:'<svg width="16" height="16" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M12 4l-6 5 6 5zM4 4h2v10H4z"/></svg>',stepNext:'<svg width="16" height="16" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M6 4l6 5-6 5zM12 4h2v10h-2z"/></svg>'},E=new Map;function G(c){const[t]=D(c);return E.has(t)||E.set(t,(async()=>{const e=D(c);let s;for(const[i,a]of e.entries()){let r;try{r=await import(`${a}/animatix_web.js`)}catch(o){s=o,E.delete(t),i+1<e.length&&console.info(`amx-player: no engine at ${a}, trying ${e[i+1]}`);continue}try{await r.default(),await r.init_engine?.()}catch(o){throw E.delete(t),o}return r}throw s})()),E.get(t)}const S=new Set;let F=!1,$=0;const O=[1,.88,.75],Y=24,Q=18.5;let M=0,w=0,L=0;function U(c){const t=Math.max(0,Math.min(O.length-1,c));if(t!==M){M=t;for(const e of S)e._applyRenderScale()}}function q(c){const t=c-$,e=Math.min(t/1e3,.1);$=c;let s=!1;for(const i of S)i.advance(e)&&(s=!0);s?(t>100||(t>Y?(w+=1,L=0,w>=12&&(U(M+1),w=0)):t<Q?(L+=1,w>0&&(w-=1),L>=30&&(U(M-1),L=0)):w>0&&(w-=1)),requestAnimationFrame(q)):F=!1}function B(){F||(F=!0,$=performance.now(),requestAnimationFrame(q))}const J=140,Z=900,tt=50,et=8e3,x=[];let C=null,P=0,W=-1/0;addEventListener("scroll",()=>{W=performance.now()},{capture:!0,passive:!0});function R(){if(clearTimeout(P),P=0,C||!x.length)return;const c=x[0];if(!c._visible){x.shift(),R();return}const t=performance.now(),e=t-W<J,s=t-c._loadWantedAt>Z;if(e&&!s){P=setTimeout(R,tt);return}x.shift(),C=c;let i=0;const a=()=>{clearTimeout(i),C===c&&(C=null,R())};i=setTimeout(a,et),Promise.resolve(c._startLoad()).then(a,a)}function st(c){x.includes(c)||(c._loadWantedAt=performance.now(),x.push(c),R())}function it(c){const t=x.indexOf(c);t>=0&&x.splice(t,1)}const I={"16:9":16/9,"4:3":4/3,"1:1":1,"9:16":9/16};class X extends HTMLElement{static observedAttributes=["src","autoplay","loop","hold","controls","sealed","fit","title","aspect","profile","quality"];constructor(){super(),this.attachShadow({mode:"open"}),this._state="idle",this._player=null,this._playing=!1,this._loop=!1,this._time=0,this._duration=0,this._holdSeconds=0,this._cycle=0,this._fade=0,this._restTime=0,this._lastAlpha=1,this._renderFailures=0,this._debugFrame=this.hasAttribute("debug-frame")||j,this._visible=!1,this._observer=null,this._renderScaleObserver=null,this._renderScaleRetry=null,this._resizeDebounceTimer=null,this._initialized=!1,this._scrubbing=!1,this._rate=1,this._markers=[],this._marks=null,this._loadWantedAt=0,this._hudFlash=null,this._hudSkip=null,this._hudTimer=null,this._skipHudTimer=null,this._singleTapTimer=null,this._lastTapTime=0,this._lastTapX=0,this._resumeOnScrubEnd=!1,this._lastSnappedMarker=null}connectedCallback(){if(!this._initialized){this._initialized=!0,this._renderSkeleton(),this._setupObserver(),this.hasAttribute("eager")&&this.loadNow();return}this._state!=="error"&&(this._setupObserver(),this._player&&this._playing&&(S.add(this),B()))}disconnectedCallback(){this._observer?.disconnect(),this._renderScaleObserver?.disconnect(),clearTimeout(this._renderScaleRetry),clearTimeout(this._resizeDebounceTimer),clearTimeout(this._hudTimer),clearTimeout(this._skipHudTimer),clearTimeout(this._singleTapTimer),S.delete(this),it(this)}loadNow(){this._visible=!0,this._state==="idle"&&this._maybeStartLoading()}attributeChangedCallback(t){if((t==="aspect"||t==="fit")&&this._initialized&&this._applyFit(),t==="hold"&&this._initialized&&this._duration>0&&this._configureCycle(),t==="quality"&&this._initialized&&this._player?.set_quality){try{this._player.set_quality(this._quality())}catch(e){console.warn(`amx-player: ${e.message}`)}this._resetForReload()}(t==="src"||t==="profile")&&this._initialized&&this._resetForReload()}_resetForReload(){this._state="idle",this._player=null,this._time=0,this._renderSkeleton(),this._maybeStartLoading()}_profile(){return this.getAttribute("profile")==="full"?"full":"slim"}_isSealed(){return this.hasAttribute("sealed")}_fit(){return this.getAttribute("fit")==="cover"?"cover":"contain"}_applyFit(){this._fit()==="cover"?(this._stage.style.aspectRatio="",this._stage.style.position="absolute",this._stage.style.inset="0",this._canvas.style.objectFit="cover"):(this._stage.style.position="",this._stage.style.inset="",this._canvas.style.objectFit="contain",this._stage.style.aspectRatio=String(this._aspectRatio()))}_quality(){const t=this.getAttribute("quality")??"draft";return["draft","preview","production"].includes(t)?t:(console.warn(`amx-player: unknown quality '${t}', using draft`),"draft")}_aspectRatio(){const t=this.getAttribute("aspect");if(t&&I[t])return I[t];const[e,s]=(t||"").split(":").map(Number);return e>0&&s>0?e/s:16/9}_renderSkeleton(){this._stage=document.createElement("div"),this._stage.className="stage";const t=this.getAttribute("title")||"",e=document.createElement("style");e.textContent=`
      :host { display: block; position: relative; overflow: hidden;
              border-radius: 8px; background: #0a0f17; }
      .stage { position: relative; overflow: hidden; }
      .skeleton {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        background: linear-gradient(120deg, #0a0f17 40%, #141c28 50%, #0a0f17 60%);
        background-size: 300% 100%;
        animation: shimmer 2.2s linear infinite;
        color: #808fa6; font: 13px/1.4 system-ui, sans-serif;
        transition: opacity 0.28s cubic-bezier(0.2, 0, 0, 1);
      }
      .skeleton.fade-out { opacity: 0; pointer-events: none; }
      .skeleton-badge {
        display: inline-flex; align-items: center; gap: 8px;
        background: rgba(5,8,12,0.65); border: 1px solid rgba(255,255,255,0.1);
        border-radius: 20px; padding: 6px 14px; font: 12px system-ui, sans-serif;
        color: #c7cfd9;
      }
      .skeleton-spinner {
        width: 12px; height: 12px; border: 2px solid rgba(245,185,66,0.25);
        border-top-color: #f5b942; border-radius: 50%;
        animation: spin 0.8s linear infinite; display: none;
      }
      .skeleton.loading .skeleton-spinner { display: inline-block; }
      @keyframes spin { to { transform: rotate(360deg); } }
      @keyframes shimmer { to { background-position: -300% 0; } }
      canvas { position: absolute; inset: 0; width: 100%; height: 100%;
               display: block; object-fit: contain; }
      .hud-flash {
        position: absolute; left: 50%; top: 50%;
        transform: translate(-50%, -50%) scale(0.85);
        width: 64px; height: 64px; border-radius: 50%;
        background: rgba(5,8,12,0.75); border: 1px solid rgba(245,185,66,0.45);
        color: #f5b942; display: flex; align-items: center; justify-content: center;
        pointer-events: none; opacity: 0;
        transition: opacity 0.3s ease-out, transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
        z-index: 5;
      }
      .hud-flash.show { opacity: 1; transform: translate(-50%, -50%) scale(1.15); }
      .hud-flash svg { width: 26px; height: 26px; display: block; }
      .hud-skip {
        position: absolute; top: 50%; transform: translateY(-50%);
        padding: 6px 14px; border-radius: 18px;
        background: rgba(5,8,12,0.85); border: 1px solid rgba(255,255,255,0.15);
        color: #f5b942; font: 12px ui-monospace, monospace;
        pointer-events: none; opacity: 0;
        transition: opacity 0.25s, transform 0.25s; z-index: 5;
      }
      .hud-skip.left { left: 10%; }
      .hud-skip.right { right: 10%; }
      .hud-skip.show { opacity: 1; }
      .veil {
        position: absolute; inset: 0; display: flex; flex-direction: column;
        align-items: center; justify-content: center; gap: 10px;
        color: #8b96a7; font: 13px/1.4 system-ui, sans-serif;
        background: rgba(13,16,22,0.75); opacity: 0; transition: opacity .3s;
        pointer-events: none; text-align: center; padding: 16px;
      }
      .veil.show { opacity: 1; pointer-events: auto; }
      .veil.error { color: #ef6a6a; }
      .veil .retry-btn {
        margin-top: 8px; padding: 6px 16px; border-radius: 6px;
        border: 1px solid rgba(255,255,255,0.25); background: rgba(255,255,255,0.12);
        color: #fff; cursor: pointer; font: 12px system-ui, sans-serif;
        transition: background 0.15s;
      }
      .veil .retry-btn:hover { background: rgba(255,255,255,0.22); }
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
      .strip {
        position: relative; height: 44px;
        display: flex; align-items: center;
        border-top: 1px solid rgba(255,255,255,.07);
        cursor: pointer; touch-action: none;
        user-select: none; -webkit-user-select: none;
        padding: 0 4px; gap: 2px;
      }
      .strip:focus-visible { outline: 2px solid #f5b942; outline-offset: -2px; }
      .strip-btn {
        width: 36px; height: 36px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        display: flex; align-items: center; justify-content: center;
        transition: background 0.15s, color 0.15s;
      }
      .strip-btn:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .strip-btn:focus-visible { outline: 2px solid #f5b942; }
      .strip-btn svg { width: 18px; height: 18px; display: block; }
      .track {
        position: relative; flex: 1; height: 100%;
        display: flex; align-items: center; margin: 0 4px;
        cursor: pointer;
      }
      .track-bar {
        position: absolute; left: 0; right: 0; height: 4px;
        background: rgba(255,255,255,0.14); border-radius: 2px;
        overflow: visible; transition: height 0.15s;
      }
      .strip:hover .track-bar, .strip.scrubbing .track-bar { height: 6px; }
      .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        background: #f5b942; border-radius: 2px;
        transform-origin: left; transform: scaleX(0);
        pointer-events: none;
      }
      .marks { position: absolute; inset: 0; pointer-events: none; }
      .strip .tick {
        position: absolute; top: -3px; bottom: -3px; width: 2px;
        transform: translateX(-50%);
        background: rgba(255,255,255,.32);
        pointer-events: none; transition: background 0.15s, transform 0.15s, box-shadow 0.15s;
      }
      .strip .tick.snapped {
        background: #f5b942; transform: translateX(-50%) scaleY(1.3);
        box-shadow: 0 0 6px rgba(245,185,66,0.6);
      }
      .strip .diamond {
        position: absolute; top: 50%; width: 9px; height: 9px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 2px;
        pointer-events: none; transition: transform 0.15s, box-shadow 0.15s;
      }
      .strip .diamond.snapped {
        background: #f5b942; transform: translate(-50%, -50%) rotate(45deg) scale(1.3);
        box-shadow: 0 0 8px rgba(245,185,66,0.8);
      }
      .strip .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.35) 0 4px, transparent 4px 8px);
        pointer-events: none;
      }
      .cursor {
        position: absolute; top: 0; bottom: 0; width: 2px; left: 0;
        transform: translateX(-50%);
        background: #f5b942; pointer-events: none;
      }
      .cursor-handle {
        position: absolute; top: 50%; left: 50%; width: 10px; height: 10px;
        transform: translate(-50%, -50%) scale(0);
        background: #f5b942; border-radius: 50%;
        box-shadow: 0 1px 4px rgba(0,0,0,0.5);
        transition: transform 0.15s ease-out; pointer-events: none;
      }
      .strip:hover .cursor-handle, .strip.scrubbing .cursor-handle {
        transform: translate(-50%, -50%) scale(1);
      }
      .scrub-pill {
        position: absolute; bottom: 100%; left: 0;
        transform: translate(-50%, -8px);
        background: rgba(10,15,23,0.92); border: 1px solid rgba(255,255,255,0.18);
        box-shadow: 0 4px 12px rgba(0,0,0,0.4);
        color: #e8edf4; font: 11px/1.3 ui-monospace, monospace;
        padding: 3px 8px; border-radius: 6px; white-space: nowrap;
        pointer-events: none; opacity: 0; transition: opacity 0.15s;
        z-index: 10;
      }
      .scrub-pill.show { opacity: 1; }
      .chip {
        flex: none;
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums;
        background: rgba(5,8,12,.55); border-radius: 6px; padding: 3px 8px;
        pointer-events: none;
      }
      .chip.dbg { color: #8ab4f8; background: rgba(5,8,12,.8); }
      .speed {
        flex: none; width: 44px; height: 36px;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        font: 12px ui-monospace, monospace;
        display: flex; align-items: center; justify-content: center;
      }
      .speed:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .speed:focus-visible { outline: 2px solid #f5b942; }
      @media (pointer: coarse) {
        .strip { height: 52px; padding: 0 6px; gap: 4px; }
        .strip-btn { width: 44px; height: 44px; }
        .strip-btn svg { width: 22px; height: 22px; }
        .speed { width: 48px; height: 44px; }
        .cursor-handle { width: 14px; height: 14px; transform: translate(-50%, -50%) scale(1); }
        .scrub-pill {
          transform: translate(-50%, -46px);
          font-size: 13px; padding: 5px 10px; border-radius: 8px;
        }
      }
      @media (max-width: 480px) {
        .chip { font-size: 11px; padding: 2px 6px; }
      }
      @media (max-width: 360px) {
        .chip { display: none; }
      }
    `,this.shadowRoot.replaceChildren(e,this._stage),this._skeleton=document.createElement("div"),this._skeleton.className="skeleton";const s=document.createElement("div");s.className="skeleton-badge";const i=document.createElement("span");i.className="skeleton-spinner";const a=document.createElement("span");a.className="skeleton-title",a.textContent=t?`${t}`:"animatix scene",s.append(i,a),this._skeleton.appendChild(s),this._stage.appendChild(this._skeleton),this._canvas=document.createElement("canvas"),this._canvas.width=1280,this._canvas.height=720,this._canvas.hidden=!0,this._hudFlash=document.createElement("div"),this._hudFlash.className="hud-flash",this._stage.appendChild(this._hudFlash),this._hudSkip=document.createElement("div"),this._hudSkip.className="hud-skip",this._stage.appendChild(this._hudSkip),this._veil=document.createElement("div"),this._veil.className="veil",this._playbtn=document.createElement("button"),this._playbtn.className="playbtn",this._playbtn.setAttribute("aria-label","Play"),this._playbtn.innerHTML=k.playBig,this._playbtn.addEventListener("click",()=>this.play()),this._controls=null,this._marks=null,this._applyFit()}_showVeil(t,e){this._veil.className=`veil show${e?" error":""}`,this._veil.replaceChildren();const s=document.createElement("div");if(s.textContent=t,this._veil.appendChild(s),e){const i=document.createElement("button");i.type="button",i.className="retry-btn",i.textContent="Retry",i.addEventListener("click",()=>this._resetForReload()),this._veil.appendChild(i)}this._veil.isConnected||this.shadowRoot.appendChild(this._veil)}_hideVeil(){this._veil.className="veil"}_triggerHud(t){this._hudFlash&&(this._hudFlash.innerHTML=t,this._hudFlash.classList.remove("show"),this._hudFlash.offsetWidth,this._hudFlash.classList.add("show"),clearTimeout(this._hudTimer),this._hudTimer=setTimeout(()=>{this._hudFlash?.classList.remove("show")},320))}_triggerSkipHud(t,e){this._hudSkip&&(this._hudSkip.className=`hud-skip ${e?"right":"left"} show`,this._hudSkip.textContent=t,clearTimeout(this._skipHudTimer),this._skipHudTimer=setTimeout(()=>{this._hudSkip?.classList.remove("show")},450))}_looping(){return this.hasAttribute("loop")||this._loop===!0}_configureCycle(){const t=this.getAttribute("hold"),e=t===null?NaN:Number(t),s=Number.isFinite(e)&&e>=0?Math.min(e,30):.7;this._holdSeconds=s,this._cycle=this._duration+s,this._fade=s>0?Math.min(V,s/2,this._duration/4):0}_loopAlpha(){if(!this._playing||!this._looping()||this._fade<=0)return 1;if(this._time<this._fade)return this._time/this._fade;const t=this._cycle-this._fade;return this._time>t?Math.max(0,(this._cycle-this._time)/this._fade):1}_setupObserver(){this._observer=new IntersectionObserver(t=>{for(const e of t)this._visible=e.isIntersecting,this._visible&&this._state==="idle"&&this._maybeStartLoading(),!this._visible&&this._playing&&this.pause(),this._visible&&this._isSealed()&&!this._playing&&this._resumeSealed()},{rootMargin:"200px"}),this._observer.observe(this)}_resumeSealed(){this._state!=="ready"||!this.hasAttribute("autoplay")||H||z||this.play()}_shouldAutoplay(){return this.hasAttribute("autoplay")&&!H&&!z}async _loadFonts(t){const e=this.getAttribute("data-fonts");if(!(!e||!this._player.add_font))for(const s of e.split(/\s+/).filter(Boolean))try{const i=await fetch(new URL(s,document.baseURI));if(!i.ok)throw new Error(`HTTP ${i.status}`);this._player.add_font(new Uint8Array(await i.arrayBuffer()))}catch(i){console.warn(`amx-player: font '${s}' skipped (${i.message})`)}}async _loadScene(t,e,s){const i=new URL(s,document.baseURI).href,a=typeof t.add_module=="function",r=new Map,o=new Set,p=new Map;let h=null;for(let l=0;l<24;l+=1){for(const[u,b]of r)t.add_module(u,b);await this._fetchAssets(t,e,r,i,p),h=this._buildWithAssets(t,e,p);const y=a?h?.missing_imports??[]:[],g=y.filter(u=>!r.has(u)&&!o.has(u));if(!y.length||!g.length)break;await Promise.all(g.map(async u=>{try{const b=await fetch(new URL(u,i));if(!b.ok)throw new Error(`HTTP ${b.status}`);r.set(u,await b.text())}catch(b){o.add(u),console.warn(`amx-player: import '${u}' skipped (${b.message})`)}}))}return h}async _fetchAssets(t,e,s,i,a){const r=[],o=(p,h)=>{let l=[];try{l=t.list_asset_urls(p)??[]}catch(y){console.warn(`amx-player: could not list asset urls (${y.message})`)}for(const y of l){const g=new URL(y,h).href;!a.has(g)&&!r.some(([u])=>u===g)&&r.push([g,y])}};o(e,i);for(const[p,h]of s)o(h,new URL(p,i).href);await Promise.all(r.map(async([p,h])=>{try{const l=await fetch(p);if(!l.ok)throw new Error(`HTTP ${l.status}`);const y=h.toLowerCase().endsWith(".svg")?await l.text():new Uint8Array(await l.arrayBuffer());a.set(p,{key:h,payload:y})}catch(l){console.warn(`amx-player: asset '${h}' skipped (${l.message})`)}}))}_buildWithAssets(t,e,s){const i=[],a=[];for(const{key:r,payload:o}of s.values())i.push(r),a.push(o);return typeof t.load_source_with_assets=="function"?t.load_source_with_assets(e,i,a):t.load_source(e)}applySource(t){if(this._state!=="ready"||!this._player)throw new Error("the player has not finished loading yet");const e=this._player.load_source(t),s=e?.diagnostics??[];if(!e?.ok){const r=new Error(s.find(o=>o.severity==="error")?.message??"build failed");throw r.diagnostics=s,r}this._duration=Math.max(e.duration_s,.05),e.width&&e.height&&(this._canvas.width=Math.round(e.width),this._canvas.height=Math.round(e.height),this._fit()==="contain"&&(this._stage.style.aspectRatio=`${this._canvas.width} / ${this._canvas.height}`)),this._markers=Array.isArray(e.markers)?e.markers:[],this._configureCycle();const i=this.shadowRoot?.querySelector(".strip");i&&i.setAttribute("aria-valuemax",String(this._duration));const a=this.shadowRoot?.querySelector(".track");return a&&a.setAttribute("aria-valuemax",String(this._duration)),this._renderMarks(),this._applyRenderScale(),this._playing?(this._time=0,this._restTime=0):(this._restTime=this._duration,this._time=this._duration),this._renderScene(),s}_maybeStartLoading(){!this.getAttribute("src")||this._state!=="idle"||st(this)}async _startLoad(){const t=this.getAttribute("src");if(!(!t||this._state!=="idle")){if(this._state="loading",this._skeleton?.classList.add("loading"),!("gpu"in navigator)){this._state="error";const e="This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).";console.warn(`amx-player: ${e}`),this._showVeil(e,!0),this.dispatchEvent(new CustomEvent("amxerror",{bubbles:!0,detail:{src:t,error:"WebGPU unsupported"}}));return}try{const e=this._profile(),[s,i]=await Promise.all([G(e),fetch(t).then(h=>{if(!h.ok)throw new Error(`HTTP ${h.status}`);return h.text()})]);if(this.getAttribute("src")!==t||(this._player=await s.create_player(this._canvas),this.getAttribute("src")!==t)||(typeof this._player.set_quality=="function"&&this._player.set_quality(this._quality()),await this._loadFonts(s),this.getAttribute("src")!==t))return;const a=await this._loadScene(this._player,i,t),r=a.diagnostics??[],o=r.filter(h=>h.severity==="error");if(!a.ok){this._state="error";const h=o[0]??r[0],l=`Scene error${h?.line?` (line ${h.line})`:""}: ${h?.message??"build failed"}`;console.error(`amx-player: ${l} [src="${t}"]`,r),this._showVeil(l,!0),this.dispatchEvent(new CustomEvent("amxerror",{bubbles:!0,detail:{src:t,diagnostics:r,message:l}}));return}this._duration=Math.max(a.duration_s,.05),this._canvas.width=Math.round(a.width||1280),this._canvas.height=Math.round(a.height||720),this._fit()==="contain"&&(this._stage.style.aspectRatio=`${this._canvas.width} / ${this._canvas.height}`),this._configureCycle(),this._shouldAutoplay()?(this._time=0,this._restTime=0):(this._restTime=this._duration,this._time=this._duration),this._renderScene(),this._markers=Array.isArray(a.markers)?a.markers:[],this._canvas.hidden=!1,this._stage.appendChild(this._canvas),this._skeleton?.classList.add("fade-out"),setTimeout(()=>{this._skeleton?.isConnected&&this._skeleton.remove()},300),this._applyRenderScale(),this._renderScaleObserver=new ResizeObserver(()=>this._applyRenderScale(!1)),this._renderScaleObserver.observe(this._stage),this._renderScaleRetry=setTimeout(()=>this._applyRenderScale(),0),this.hasAttribute("controls")?(this._buildControls(),this._setupGestures()):this._stage.appendChild(this._playbtn),this._stage.appendChild(this._veil),r.length>0&&console.warn(`amx-player: ${t} built with ${r.length} diagnostic(s)`,r[0]),this._state="ready",this._shouldAutoplay()?(this._loop=this.hasAttribute("loop"),this.play()):!this.hasAttribute("controls")&&!this._isSealed()&&this._playbtn.classList.add("show"),this.dispatchEvent(new CustomEvent("amxready",{bubbles:!0}))}catch(e){this._state="error",console.error(`amx-player: failed to load scene "${t}":`,e),this._showVeil(`Failed to load scene: ${e?.message??e}`,!0),this.dispatchEvent(new CustomEvent("amxerror",{bubbles:!0,detail:{src:t,error:e}}))}}}_buildControls(){const t=document.createElement("div");t.className="strip",t.tabIndex=0,t.setAttribute("role","region"),t.setAttribute("aria-label","Playback controls");const e=document.createElement("button");e.type="button",e.className="strip-btn play-toggle",e.setAttribute("aria-label","Play"),e.innerHTML=k.play,e.addEventListener("click",n=>{n.stopPropagation(),this.togglePlay(!1)});const s=document.createElement("div");s.className="track",s.setAttribute("role","slider"),s.setAttribute("aria-label","Timeline"),s.setAttribute("aria-valuemin","0"),s.setAttribute("aria-valuemax",String(this._duration));const i=document.createElement("div");i.className="track-bar";const a=document.createElement("div");a.className="fill";const r=document.createElement("div");r.className="cursor";const o=document.createElement("div");o.className="cursor-handle",r.appendChild(o),this._marks=document.createElement("div"),this._marks.className="marks",this._renderMarks(),i.append(a,this._marks,r);const p=document.createElement("div");p.className="scrub-pill",s.append(i,p);const h=document.createElement("span");h.className="chip";const l=document.createElement("button");l.type="button",l.className="speed",l.textContent="1\xD7",l.setAttribute("aria-label","Playback speed 1\xD7"),l.addEventListener("pointerdown",n=>n.stopPropagation()),l.addEventListener("click",n=>{n.stopPropagation();const d=[1,1.5,2,.5];this._rate=d[(d.indexOf(this._rate)+1)%d.length],l.textContent=`${this._rate}\xD7`,l.setAttribute("aria-label",`Playback speed ${this._rate}\xD7`)}),t.append(e,s,h,l),this.shadowRoot.append(t);const y=n=>{let _=null,m=.2+1e-4;for(const f of this._markers){const v=Math.abs(n-f.t);if(v<=.2&&v<m&&(m=v,_={t:f.t,kind:f.kind}),f.kind==="transition"&&f.dur>0){const A=Math.abs(n-(f.t+f.dur));A<=.2&&A<m&&(m=A,_={t:f.t+f.dur,kind:"transition"})}}return _},g=n=>{if(this._marks)if(this._marks.querySelectorAll(".snapped").forEach(d=>d.classList.remove("snapped")),n){for(const d of this._marks.children)d.dataset.t&&Math.abs(Number(d.dataset.t)-n.t)<.001&&d.classList.add("snapped");if(navigator.vibrate&&this._lastSnappedMarker!==n.t)try{navigator.vibrate(10)}catch{}this._lastSnappedMarker=n.t}else this._lastSnappedMarker=null},u=n=>{const d=s.getBoundingClientRect();if(d.width<=0||!Number.isFinite(n.clientX))return{time:this._time,pct:0,snap:null};const _=Math.min(Math.max((n.clientX-d.left)/d.width,0),1),m=_*this._duration,f=y(m),v=f?f.t:m,A=this._duration>0?v/this._duration:_;return{time:v,pct:A,snap:f}},b=(n,d)=>{p.style.left=`${d*100}%`;const _=Math.round(n*60);p.textContent=`${n.toFixed(2)}s (f${_})`,p.classList.add("show")};s.addEventListener("pointerenter",n=>{if(n.pointerType!=="mouse"||this._scrubbing)return;const{time:d,pct:_,snap:m}=u(n);b(d,_),g(m)}),s.addEventListener("pointermove",n=>{const{time:d,pct:_,snap:m}=u(n);b(d,_),g(m),this._scrubbing&&this.seek(d)}),s.addEventListener("pointerleave",n=>{this._scrubbing||(p.classList.remove("show"),g(null))}),s.addEventListener("pointerdown",n=>{try{s.setPointerCapture(n.pointerId)}catch{}this._resumeOnScrubEnd=this._playing,this._playing&&this.pause(),this._scrubbing=!0,t.classList.add("scrubbing");const{time:d,pct:_,snap:m}=u(n);b(d,_),g(m),this.seek(d),n.stopPropagation()});const T=n=>{this._scrubbing&&(this._scrubbing=!1,t.classList.remove("scrubbing"),p.classList.remove("show"),g(null),this._resumeOnScrubEnd&&(this._resumeOnScrubEnd=!1,this.play()))};s.addEventListener("pointerup",T),s.addEventListener("pointercancel",T),t.addEventListener("keydown",n=>this._handleKeyDown(n)),this._controls={strip:t,playToggle:e,track:s,fill:a,cursor:r,scrubPill:p,chip:h,speed:l},this._syncControls()}_renderMarks(){if(!this._marks)return;const t=document.createDocumentFragment();for(const e of this._markers){const s=this._duration>0?Math.min(e.t/this._duration,1):0;if(e.kind==="transition"&&e.dur>0){const i=document.createElement("div");i.className="span",i.style.left=`${s*100}%`,i.style.width=`${Math.min(e.dur/this._duration,1-s)*100}%`,i.dataset.t=String(e.t),t.appendChild(i)}else{const i=document.createElement("div");i.className=e.kind==="scene"?"diamond":"tick",i.style.left=`${s*100}%`,i.dataset.t=String(e.t),t.appendChild(i)}}this._marks.replaceChildren(t)}_nextLandmark(t){const e=[];for(const a of this._markers)e.push(a.t),a.kind==="transition"&&a.dur>0&&e.push(a.t+a.dur);e.sort((a,r)=>a-r);const s=.001;return t>0?e.find(r=>r>this._time+s)??Math.min(this._time+1,this._duration):[...e].reverse().find(a=>a<this._time-s)??Math.max(this._time-1,0)}_syncControls(){const t=this._controls;if(!t)return;const e=Math.min(this._time,this._duration),s=this._duration>0?e/this._duration:0;if(t.fill.style.transform=`scaleX(${s})`,t.cursor.style.left=`${s*100}%`,this._playing?(t.playToggle.innerHTML=k.pause,t.playToggle.setAttribute("aria-label","Pause")):this._time>=this._duration&&!this._looping()?(t.playToggle.innerHTML=k.replay,t.playToggle.setAttribute("aria-label","Replay")):(t.playToggle.innerHTML=k.play,t.playToggle.setAttribute("aria-label","Play")),this._debugFrame){const i=this._renderScale??1;t.chip.textContent=`f${this.frame} / ${this.totalFrames}  ${e.toFixed(3)}s  @60fps  rs${i.toFixed(2)}`,t.chip.classList.add("dbg")}else t.chip.textContent=`${e.toFixed(1)} / ${this._duration.toFixed(1)}`,t.chip.classList.remove("dbg");t.track.setAttribute("aria-valuenow",e.toFixed(1))}_setupGestures(){this._canvas.tabIndex=0,this._canvas.addEventListener("pointerdown",t=>{if(!this._controls&&!this.hasAttribute("controls")&&this._isSealed())return;if(this._canvas.focus({preventScroll:!0}),t.pointerType==="mouse"){this.togglePlay(!0),t.preventDefault();return}const e=Date.now(),s=e-this._lastTapTime,i=Math.abs(t.clientX-this._lastTapX);if(s<280&&i<48){clearTimeout(this._singleTapTimer),this._lastTapTime=0;const a=this._canvas.getBoundingClientRect(),r=a.width>0?(t.clientX-a.left)/a.width:.5;if(r<.35){const o=this._nextLandmark(-1);this.seek(o),this._triggerSkipHud("\xAB Keyframe",!1)}else if(r>.65){const o=this._nextLandmark(1);this.seek(o),this._triggerSkipHud("Keyframe \xBB",!0)}else this.togglePlay(!0)}else this._lastTapTime=e,this._lastTapX=t.clientX,clearTimeout(this._singleTapTimer),this._singleTapTimer=setTimeout(()=>{this.togglePlay(!0)},280);t.preventDefault()}),this._canvas.addEventListener("keydown",t=>this._handleKeyDown(t))}_handleKeyDown(t){if(!(t.target&&(t.target.tagName==="INPUT"||t.target.tagName==="TEXTAREA"||t.target.isContentEditable))){if(t.key===" "||t.key==="k"||t.key==="K")t.preventDefault(),t.stopPropagation(),this.togglePlay(!0);else if(t.key==="ArrowLeft")if(t.preventDefault(),t.stopPropagation(),t.shiftKey)this.stepFrame(-1);else{const e=this._nextLandmark(-1);this.seek(e),this._triggerSkipHud("\xAB Keyframe",!1)}else if(t.key==="ArrowRight")if(t.preventDefault(),t.stopPropagation(),t.shiftKey)this.stepFrame(1);else{const e=this._nextLandmark(1);this.seek(e),this._triggerSkipHud("Keyframe \xBB",!0)}else if(t.key===","||t.key==="<")t.preventDefault(),t.stopPropagation(),this.stepFrame(-1);else if(t.key==="."||t.key===">")t.preventDefault(),t.stopPropagation(),this.stepFrame(1);else if(t.key==="j"||t.key==="J")t.preventDefault(),t.stopPropagation(),this.seek(Math.max(this._time-1,0)),this._triggerSkipHud("\xAB 1s",!1);else if(t.key==="l"||t.key==="L")t.preventDefault(),t.stopPropagation(),this.seek(Math.min(this._time+1,this._duration)),this._triggerSkipHud("1s \xBB",!0);else if(t.key==="Home"||t.key==="0")t.preventDefault(),t.stopPropagation(),this.seek(0),this._triggerSkipHud("0:00",!1);else if(t.key==="End")t.preventDefault(),t.stopPropagation(),this.seek(this._duration),this._triggerSkipHud("End",!0);else if(t.key==="d"||t.key==="D")t.preventDefault(),t.stopPropagation(),this._debugFrame=!this._debugFrame,this._syncControls();else if(t.key==="c"||t.key==="C"){t.preventDefault(),t.stopPropagation();const e=this.debugReport();navigator.clipboard?.writeText(e).catch(()=>{}),console.log(e),this._triggerSkipHud("Copied report",!0)}}}togglePlay(t=!0){this._playing?(this.pause(),t&&this._triggerHud(k.pause)):(this.play(),t&&this._triggerHud(k.play))}stepFrame(t){this._playing&&this.pause();const e=Math.min(Math.max(this._time+t/60,0),this._duration);this.seek(e);const s=Math.round(this._time*60);this._triggerSkipHud(t>0?`+1f (f${s})`:`-1f (f${s})`,t>0)}play(){this._state==="ready"&&this._player?.has_document()&&(this._time>=this._duration&&(this._time=0),this._playing=!0,this._applyRenderScale(),this._playbtn.classList.remove("show"),this._syncControls(),S.add(this),B())}pause(){this._playing=!1,S.delete(this),this._state==="ready"&&(!this._controls&&!this._isSealed()&&!this.hasAttribute("autoplay")&&this._playbtn.classList.add("show"),this._syncControls())}seek(t){this._state==="ready"&&(this._time=Math.min(Math.max(t,0),this._duration),this._restTime=this._time,this._renderScene(),this._syncControls())}get duration(){return this._duration}get time(){return Math.min(this._time,this._duration)}get frame(){return Math.round(this.time*60)}get totalFrames(){return Math.round(this._duration*60)}debugReport(){return`frame ${this.frame}/${this.totalFrames} @60fps, time ${this.time.toFixed(3)}s, render scale ${(this._renderScale??1).toFixed(2)}`}advance(t){if(!this._playing||!this._visible||this._scrubbing)return!1;const e=this._looping(),s=e?this._cycle:this._duration;if(this._time+=t*(this._rate??1),this._time>=s)if(e)this._time%=s;else return this._time=this._duration,this._restTime=this._duration,this.pause(),this._renderScene(),!1;return this._renderScene(),this._controls&&this._syncControls(),!0}_applyRenderScale(t=!0){const e=this._player;if(!e?.set_render_scale||!e.scene_width||!this.isConnected)return;const s=e.scene_width()||this._canvas.width,i=e.scene_height()||this._canvas.height,a=this._canvas.clientWidth||this._stage.clientWidth||0,r=this._canvas.clientHeight||this._stage.clientHeight||0,o=window.devicePixelRatio||1,p=Math.min(Math.max(1,o),2),h=a>0&&r>0&&s>0&&i>0?Math.min(p,a*o/s,r*o/i):1,l=Math.max(1,Math.round(s*h)),y=Math.max(1,Math.round(i*h)),g=this._canvas.width!==l||this._canvas.height!==y,u=()=>{if(this._resizeDebounceTimer=null,!this.isConnected||!this._player)return;const b=this._canvas.clientWidth||this._stage.clientWidth||0,T=this._canvas.clientHeight||this._stage.clientHeight||0,n=window.devicePixelRatio||1,d=Math.min(Math.max(1,n),2),_=b>0&&T>0&&s>0&&i>0?Math.min(d,b*n/s,T*n/i):1,m=Math.max(1,Math.round(s*_)),f=Math.max(1,Math.round(i*_));(this._canvas.width!==m||this._canvas.height!==f)&&(this._canvas.width=m,this._canvas.height=f);const v=Math.min(2,Math.max(.25,_*O[M]));this._renderScale=e.set_render_scale(v),!this._playing&&this._state==="ready"&&this._renderScene()};this._resizeDebounceTimer&&(clearTimeout(this._resizeDebounceTimer),this._resizeDebounceTimer=null),t||!g?u():this._resizeDebounceTimer=setTimeout(u,100)}_renderScene(){if(this._player)try{const t=this._playing?Math.min(this._time,this._duration):this._restTime;this._playing&&(this._restTime=t),this._player.render_frame(t),this._renderFailures=0;const e=this._loopAlpha();e!==this._lastAlpha&&(this._canvas.style.opacity=String(e),this._lastAlpha=e)}catch(t){this._renderFailures+=1,this._renderFailures===1&&console.warn("amx-player: render failed",t)}}}customElements.get("amx-player")||customElements.define("amx-player",X);export{X as AmxPlayerElement};
