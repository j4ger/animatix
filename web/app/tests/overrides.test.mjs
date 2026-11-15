import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import {withOverride,presetFor} from '../src/lib/overrides.ts';

test('a configured download changes exactly the canonical scalar default',()=>{
 const source=fs.readFileSync(new URL('../../content/scenes/one-rule-many-forms.amx',import.meta.url),'utf8');
 const configured=withOverride(source,'preset',1);
 assert.equal(configured,source.replace(/^let preset = 0$/m,'let preset = 1'));
 assert.equal(withOverride(configured,'preset',0),source);
});
test('phase presets arrive in the editor as a source change',()=>{
 const source=fs.readFileSync(new URL('../../content/scenes/when-waves-meet.amx',import.meta.url),'utf8');
 for(const n of [0,90,180]){
  const pair=presetFor('when-waves-meet',new URLSearchParams(`phase_degrees=${n}`));
  assert.deepEqual(pair,['phase_degrees',n]);
  assert.match(withOverride(source,...pair),new RegExp(`^let phase_degrees = ${n}$`,'m'));
 }
});
test('query strings cannot inject unsupported scene variables or expressions',()=>{
 for(const [id,q] of [['first-move','preset=1'],['one-rule-many-forms','preset=2'],['when-waves-meet','phase_degrees=NaN'],['when-waves-meet','phase_degrees=0%3Balert(1)']])assert.equal(presetFor(id,new URLSearchParams(q)),null);
 assert.throws(()=>withOverride('let preset = 0','unknown',1));
 assert.throws(()=>withOverride('let preset = 0','preset',Infinity));
 assert.throws(()=>withOverride('let phase = 0','preset',1));
});
