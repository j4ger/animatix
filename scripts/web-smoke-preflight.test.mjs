import test from 'node:test';
import assert from 'node:assert/strict';
import {deflateSync} from 'node:zlib';
import {visualPreflight} from './web-smoke-preflight.mjs';
import {chromiumLaunch} from './web-smoke-launch.mjs';
function solidPng(rgba) {
 const chunk=(name,data)=>{const body=Buffer.concat([Buffer.from(name),data]);let crc=0xffffffff;
  for(const b of body){crc^=b;for(let i=0;i<8;i++)crc=crc&1?(crc>>>1)^0xedb88320:crc>>>1;}
  const out=Buffer.alloc(body.length+8);out.writeUInt32BE(data.length);body.copy(out,4);out.writeUInt32BE((crc^0xffffffff)>>>0,out.length-4);return out;};
 const ihdr=Buffer.alloc(13);ihdr.writeUInt32BE(64);ihdr.writeUInt32BE(64,4);ihdr[8]=8;ihdr[9]=6;
 const raw=Buffer.alloc(64*257);for(let y=0;y<64;y++)for(let x=0;x<64;x++)raw.set(rgba,y*257+1+x*4);
 return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',ihdr),chunk('IDAT',deflateSync(raw)),chunk('IEND',Buffer.alloc(0))]);
}
function fakeCdp(png){const expressions=[];return {expressions,evaluate:async s=>{expressions.push(s);return [];},send:async method=>{assert.equal(method,'Page.captureScreenshot');return {data:png.toString('base64')};}};}
test('launch owns Xvfb and uses the validated software compositor combination',()=>{
 const launch=chromiumLaunch('/chromium','/tmp/profile',9222);
 assert.equal(launch.command,'xvfb-run');assert.deepEqual(launch.args.slice(0,4),['-a','-s','-screen 0 1600x1200x24','/chromium']);
 for(const flag of ['--enable-gpu','--use-angle=swiftshader','--enable-unsafe-swiftshader','--enable-features=Vulkan','--use-vulkan=swiftshader'])assert.ok(launch.args.includes(flag));
 assert.ok(!launch.args.includes('--disable-vulkan-surface'));assert.ok(!launch.args.includes('--use-angle=vulkan'));
});
test('preflight requires real compositor color and submits only once',async()=>{
 const cdp=fakeCdp(solidPng([101,217,232,255]));let saved=0;
 const result=await visualPreflight(cdp,()=>saved++);assert.equal(result.coverage,1);assert.equal(saved,1);
 assert.equal(cdp.expressions.filter(s=>s.includes('queue.submit')).length,1);
 assert.ok(cdp.expressions.at(-1).includes('device.destroy'));
 assert.doesNotMatch(cdp.expressions.join('\n'),/toDataURL|\.seek\(/);
});
test('a blank compositor fails even if adapter/device/submission succeeded',async()=>{
 const cdp=fakeCdp(solidPng([0,0,0,0]));
 await assert.rejects(visualPreflight(cdp,()=>{}),/compositor preflight failed/);
 assert.equal(cdp.expressions.filter(s=>s.includes('queue.submit')).length,1);
});
