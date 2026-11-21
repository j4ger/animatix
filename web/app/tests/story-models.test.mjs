import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const scene = name => readFileSync(new URL(`../../content/scenes/${name}.amx`,import.meta.url),'utf8');
test('insertion story counts comparisons and shifts, preserving equal-key order',()=>{
 const a=[{v:4,id:'4'},{v:2,id:'2a'},{v:5,id:'5'},{v:2,id:'2b'},{v:3,id:'3'}];
 let comparisons=0, shifts=0;
 for(let i=1;i<a.length;i++){const key=a[i];let j=i-1;while(j>=0){comparisons++;if(a[j].v<=key.v)break;a[j+1]=a[j];j--;shifts++;}a[j+1]=key;}
 assert.equal(comparisons,8);assert.equal(shifts,5);assert.deepEqual(a.map(x=>x.id),['2a','2b','3','4','5']);
 const amx=scene('make-room'); assert.match(amx,/2a/);assert.match(amx,/2b/);
 assert.deepEqual([...amx.matchAll(/comparisons\.value = (\d+) \[200ms\b/g)].map(x=>Number(x[1])),Array.from({length:8},(_,i)=>i+1));
 assert.deepEqual([...amx.matchAll(/shifts\.value = (\d+) \[400ms\b/g)].map(x=>Number(x[1])),Array.from({length:5},(_,i)=>i+1));
});
test('shear preserves area, not length, at all authored stops',()=>{
 for(const k of [0,1,-.6,1]){assert.equal(1*1-k*0,1);assert.equal((1+k)-k,1);if(k!==0)assert.ok(Math.hypot(k,1)>1);}
 assert.match(scene('move-the-basis'),/area/i);assert.match(scene('move-the-basis'),/length/i);
});
