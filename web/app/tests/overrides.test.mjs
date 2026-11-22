import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import {withOverride,overridesFor} from '../src/lib/overrides.ts';

test('a configured download changes exactly the canonical scalar default',()=>{
 const source=fs.readFileSync(new URL('../../content/scenes/ripple-and-cascade.amx',import.meta.url),'utf8');
 const configured=withOverride(source,'start_pattern',1);
 assert.equal(configured,source.replace(/^let start_pattern = 0$/m,'let start_pattern = 1'));
 assert.equal(withOverride(configured,'start_pattern',0),source);
 for (const query of ['start_pattern=1','preset=1']) {
  assert.deepEqual(overridesFor('ripple-and-cascade',new URLSearchParams(query)),[['start_pattern',1]]);
 }
});
test('phase presets arrive in the editor as a source change',()=>{
 const source=fs.readFileSync(new URL('../../content/scenes/when-waves-meet.amx',import.meta.url),'utf8');
 for(const n of [0,90,180]){
  const [pair]=overridesFor('when-waves-meet',new URLSearchParams(`phase_degrees=${n}`));
  assert.deepEqual(pair,['phase_degrees',n]);
  assert.match(withOverride(source,...pair),new RegExp(`^let phase_degrees = ${n}$`,'m'));
 }
});
test('query strings cannot inject unsupported scene variables or expressions',()=>{
 for(const [id,q] of [['first-move','preset=1'],['ripple-and-cascade','preset=2'],['when-waves-meet','phase_degrees=NaN'],['when-waves-meet','phase_degrees=0%3Balert(1)']])assert.deepEqual(overridesFor(id,new URLSearchParams(q)),[]);
 assert.throws(()=>withOverride('let start_pattern = 0','unknown',1));
 assert.throws(()=>withOverride('let start_pattern = 0','start_pattern',Infinity));
 assert.throws(()=>withOverride('let phase = 0','start_pattern',1));
});
test('the journey carries all three choices into editable source',()=>{
 const original=fs.readFileSync(new URL('../../content/scenes/give-it-a-beat.amx',import.meta.url),'utf8');
 const choices=overridesFor('give-it-a-beat',new URLSearchParams('stress=2&beat=0.20&diagonal=1&onion_skin=1'));
 const edited=choices.reduce((source,[key,value])=>withOverride(source,key,value),original);
 assert.match(edited,/let beat = 0.2\nlet diagonal = 1\nlet stress = 2/);
 assert.match(edited,/let onion_skin = 0/);
});
