import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,existsSync} from 'node:fs';
import {techniques} from '../src/lib/techniques.ts';
const src = path=>new URL(path,import.meta.url);
test('all techniques have unique canonical source and a teaching intent',()=>{
 assert.equal(techniques.length,4);assert.equal(new Set(techniques.map(x=>x.id)).size,4);
 for(const t of techniques){assert.ok(t.goal&&t.explanation&&t.exercise&&t.rule);assert.ok(existsSync(src(`../../content/scenes/${t.file}.amx`)));}
});
test('the artifact publishes current content, not legacy sources or developer tools',()=>{
 const script=readFileSync(src('../../../web/tools/site/site-artifact.sh'),'utf8');
 assert.match(script,/entries=\(logo\.svg assets content\)/);
 assert.match(script,/cp -R web\/app\/dist\/\./);
 assert.doesNotMatch(script,/entries=\([^)]*\b(?:demos|tour|recipes|scenes|dev)\b/);
});
test('each curated work has a provenance-bound recording',()=>{
 const source=readFileSync(src('../src/lib/content.ts'),'utf8').split('export const lessons')[0];
 const ids=[...source.matchAll(/id: '([^']+)'/g)].map(m=>m[1]);
 const manifest=JSON.parse(readFileSync(src('../../content/preview-manifest.json'),'utf8'));
 assert.deepEqual(manifest.works.map(w=>w.id).sort(),ids.sort());
 for(const w of manifest.works){assert.equal(w.width,1280);assert.equal(w.height,720);assert.equal(w.fps,'24/1');}
});
