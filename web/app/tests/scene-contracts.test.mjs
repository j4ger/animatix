import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {insertionSteps, sourceCues} from '../src/lib/insertion-steps.ts';

const source = readFileSync(new URL('../../content/scenes/insertion-sort.amx', import.meta.url), 'utf8');

test('every inspectable move identifies its original AMX line and cue', () => {
  const steps = insertionSteps(source);
  assert.deepEqual(steps.map(step => [step.id, step.start, step.duration]), [
    ['five', 3.7, 1.25], ['four', 6.95, 1.25], ['key', 8.7, 1.5],
  ]);
  for (const step of steps) {
    assert.equal(source.split('\n')[step.line - 1].trim(), step.code);
  }
});

test('editing a move in the Playground follows the new cue and duration', () => {
  const edited = source.replace('#6.95s', '#7s').replace(/four\.at[^\n]+/, line => line.replace('1.25s', '1.6s'));
  const move = insertionSteps('// Added line\n' + edited).find(step => step.id === 'four');
  assert.equal(move.start, 7);
  assert.equal(move.duration, 1.6);
  assert.equal(move.line, insertionSteps(source)[1].line + 1);
});

test('the source navigator marks every absolute timestamp and reads edited millisecond cues', () => {
  const cues = sourceCues(source);
  assert.equal(cues.length, source.match(/^#/gm).length);
  assert.deepEqual(sourceCues('// #4s\n#0s\n#750ms // reveal\n#1.2s').map(({line, time}) => [line, time]), [
    [2, 0], [3, 0.75], [4, 1.2],
  ]);
});
