import {assertPaint} from './web-smoke-pixels.mjs';
import {until} from './web-smoke-browser.mjs';

// Called on a manifest-only same-origin document before any SDK is imported.
// One GPU submission, then only compositor captures. Never retries the draw or
// uses canvas readback, which can be transparent even when presentation works.
export async function visualPreflight(cdp, save) {
  await cdp.evaluate(`(async()=>{
    document.body.replaceChildren();
    const canvas=document.createElement('canvas');canvas.width=64;canvas.height=64;
    canvas.style.cssText='position:fixed;left:0;top:0;width:64px;height:64px';
    document.body.append(canvas);
    const adapter=await navigator.gpu.requestAdapter();if(!adapter)throw Error('Visual preflight: adapter disappeared');
    const device=await adapter.requestDevice();window.__visualPreflight={device,errors:[]};
    device.addEventListener('uncapturederror',e=>__visualPreflight.errors.push(e.error.message));
    const context=canvas.getContext('webgpu');
    context.configure({device,format:'rgba8unorm',alphaMode:'premultiplied'});
    const encoder=device.createCommandEncoder();
    encoder.beginRenderPass({colorAttachments:[{view:context.getCurrentTexture().createView(),clearValue:{r:101/255,g:217/255,b:232/255,a:1},loadOp:'clear',storeOp:'store'}]}).end();
    device.queue.submit([encoder.finish()]);
  })()`);
  let pixels, last;
  await until('WebGPU compositor pixels',async()=>{
    const errors=await cdp.evaluate('window.__visualPreflight.errors');
    if(errors.length)throw Error(`Visual preflight GPU validation: ${errors.join('; ')}`);
    const shot=await cdp.send('Page.captureScreenshot',{format:'png',clip:{x:0,y:0,width:64,height:64,scale:1},captureBeyondViewport:false});
    const bytes=Buffer.from(shot.data,'base64');save(bytes);
    try {
      pixels=assertPaint(bytes,{color:'#65D9E8',x:0.5,y:0.5,tolerance:0.03});
      if(pixels.coverage<0.95)throw Error(`Only ${pixels.coverage} of preflight canvas is cyan`);
      return true;
    }catch(error){last=error;return false;}
  },5000).catch(error=>{throw Error(`WebGPU compositor preflight failed (not a scene failure): ${error.message}; ${last?.message}`);});
  await cdp.evaluate('window.__visualPreflight.device.destroy();delete window.__visualPreflight');
  return pixels;
}
