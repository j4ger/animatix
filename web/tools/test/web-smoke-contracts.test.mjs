import test from 'node:test';
import assert from 'node:assert/strict';
import {contracts} from './web-smoke-contracts.mjs';
import {readFileSync} from 'node:fs';
test('SDK fixtures have explicit fonts, count and motion contracts',()=>{
 const cases=contracts('sdk');assert.equal(cases.length,5);
 assert.ok(cases.every(c=>c.path.startsWith('/dev/smoke/')));
 assert.equal(cases.find(c=>c.name==='shared').players,2);
 assert.equal(cases.find(c=>c.name==='text').fonts,true);
 assert.deepEqual(cases.find(c=>c.name==='imports').modules,['palette.amx','tokens.amx']);
});
test('real site covers the homepage preview, static docs and scene editing',()=>{
 const cases=contracts('site');assert.equal(cases.find(c=>c.path==='/').action,'preview');
 assert.equal(cases.find(c=>c.path==='/docs/').action,'static');
 assert.ok(cases.some(c=>c.action==='edit'));assert.ok(cases.some(c=>c.motion&&c.action==='load'));
 assert.ok(contracts('site','deep').length>cases.length);
});
test('unknown configuration fails closed',()=>{
 assert.throws(()=>contracts('legacy'));assert.throws(()=>contracts('site','typo'));
});
test('journeys use public playback API and do not force lazy loading',()=>{
 const source=readFileSync(new URL('./web-smoke-journeys.mjs',import.meta.url),'utf8');
 assert.doesNotMatch(source, /\._state|\._loadedBundle|\.loadNow\(/);
 assert.match(source,/\.seek\(/);assert.match(source,/\.click\(/);
});

test('project base prefixes contracts and links, never silently tests root',async()=>{
 const {siteBase,withBase}=await import('./web-smoke-contracts.mjs');
 assert.equal(siteBase('/animatix/'),'/animatix');assert.equal(siteBase('/'),'');
 const home=withBase(contracts('site')[0],siteBase('/animatix/'));
 assert.equal(home.path,'/animatix/');assert.deepEqual(home.links,['/animatix/journey/','/animatix/showcase/move-the-basis/','/animatix/docs/','/animatix/playground/']);
 for(const base of ['https://example.com','../x','/a//b','/%2e','/a?b'])assert.throws(()=>siteBase(base));
});
test('documentation has direct destinations and showcase checks presets',()=>{
 for(const path of ['/docs/','/docs/quickstart/'])assert.ok(contracts('site').find(c=>c.path===path).links.length>=2);
 assert.equal(contracts('site').find(c=>c.path==='/showcase/ripple-and-cascade/').preset,true);
 assert.ok(contracts('site').some(c=>c.backend==='webgl2'&&c.action==='load'));
 assert.deepEqual(contracts('site').filter(c=>c.action==='recording'&&c.recording).map(c=>c.locale), ['en','zh-Hans']);
 assert.equal(contracts('site').filter(c=>c.action==='direct-embed').length, 2);
 assert.ok(contracts('site').some(c=>c.action==='direct-embed'&&c.forceGraphicsFailure));
});
test('navigation retries only destroyed execution contexts within a deadline',async()=>{
 const {navigationCommitted}=await import('./web-smoke-browser.mjs');let calls=0;
 await navigationCommitted({evaluate:async()=>{if(calls++===0)throw Error('Execution context was destroyed.');return true;}},'https://example.test',1000);
 assert.equal(calls,2);
 await assert.rejects(navigationCommitted({evaluate:async()=>{throw Error('ReferenceError: broken');}},'https://example.test',1000),/ReferenceError/);
 await assert.rejects(navigationCommitted({evaluate:async()=>{throw Error('Execution context was destroyed.');}},'https://example.test',10),/Timed out/);
});

test('deep covers each current work, lesson and technique alongside the fallback journeys',()=>{
 const content=readFileSync(new URL('../../app/src/lib/content.ts',import.meta.url),'utf8');
 const techniques=readFileSync(new URL('../../app/src/lib/techniques.ts',import.meta.url),'utf8');
 const works=content.split('export const works = [')[1].split('];')[0];
 const lessons=content.split('export const lessons = [')[1].split('];')[0];
 const ids=section=>[...section.matchAll(/\bid: '([^']+)'/g)].map(m=>m[1]);
 const expected=[...ids(works).map(id=>`/showcase/${id}/`),...ids(lessons).map(id=>`/learn/${id}/`),
  ...ids(techniques).map(id=>`/learn/techniques/${id}/`)];
 const actual=contracts('site','deep').filter(c=>c.action==='load'&&!c.backend).map(c=>c.path);
 assert.deepEqual(actual.sort(),expected.sort());
 assert.equal(new Set(actual).size,actual.length);
 assert.equal(contracts('site','deep').at(-1).action,'edit');
});
