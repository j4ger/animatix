import assert from 'node:assert/strict';
import test from 'node:test';
import {sourceCells} from '../src/lib/source-cells.ts';

test('only tokenizer keyframes start cells and blank rows keep their place', () => {
  const source = 'let message = $$\n#4s\n$$\n\n#0s\n  actor.color = red\n\n#+2s\n  actor.color = blue\n';
  const first = source.indexOf('#0s');
  const second = source.indexOf('#+2s');
  const ranges = [[14, first - 2, 'string'], [first, first + 3, 'keyframe'], [second, second + 4, 'keyframe']];
  assert.deepEqual(sourceCells(source.split('\n'), ranges), [
    {start: 0, end: 3, next: 4, keyframe: false},
    {start: 4, end: 6, next: 7, keyframe: true},
    {start: 7, end: 9, next: 10, keyframe: true},
  ]);
});

test('a keyframe on the first line does not create a blank definitions cell', () => {
  assert.deepEqual(sourceCells(['#0s', ''], [[0, 3, 'keyframe']]), [
    {start: 0, end: 1, next: 2, keyframe: true},
  ]);
});
