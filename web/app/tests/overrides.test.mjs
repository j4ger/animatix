import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import {withOverride,overridesFor} from '../src/lib/overrides.ts';

test('a configured download changes exactly the canonical scalar default',()=>{
 const source=fs.readFileSync(new URL('../../content/scenes/one-rule-many-forms.amx',import.meta.url),'utf8');
 const configured=withOverride(source,'preset',1);
 assert.equal(configured,source.replace(/^let preset = 0$/m,'let preset = 1'));
 assert.equal(withOverride(configured,'preset',0),source);
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
 for(const [id,q] of [['first-move','preset=1'],['one-rule-many-forms','preset=2'],['when-waves-meet','phase_degrees=NaN'],['when-waves-meet','phase_degrees=0%3Balert(1)']])assert.deepEqual(overridesFor(id,new URLSearchParams(q)),[]);
 assert.throws(()=>withOverride('let preset = 0','unknown',1));
 assert.throws(()=>withOverride('let preset = 0','preset',Infinity));
 assert.throws(()=>withOverride('let phase = 0','preset',1));
});
test('the journey carries all three choices into editable source',()=>{
 const original=fs.readFileSync(new URL('../../content/scenes/give-it-a-beat.amx',import.meta.url),'utf8');
 const choices=overridesFor('give-it-a-beat',new URLSearchParams('stress=2&beat=0.20&diagonal=1&onion_skin=1'));
 const edited=choices.reduce((source,[key,value])=>withOverride(source,key,value),original);
 assert.match(edited,/let beat = 0.2\nlet diagonal = 1\nlet stress = 2/);
 assert.match(edited,/let onion_skin = 0/);
});
