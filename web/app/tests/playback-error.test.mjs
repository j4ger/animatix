import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {diagnosticsFrom, playbackError, readableError} from '../src/lib/errors.ts';
import {bundle, translator} from '../src/lib/fluent.ts';

function catalog(locale) {
  const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
  return bundle(locale,
    read('../src/locales/' + locale + '/site.ftl'),
    read('../../embed/src/locales/' + locale + '.ftl'));
}

const en = catalog('en');
const translate = [translator(en, en), translator(catalog('zh-Hans'), en)];
const graphics = ['This browser could not start WebGPU or WebGL2.', '此浏览器无法启动 WebGPU 或 WebGL2。'];

test('localized graphics explanation handles typed events and older runtimes', () => {
  const original = new Error('no supported graphics backend');
  const typed = playbackError({kind: 'capability', message: original.message, error: original}, 'fallback');
  assert.equal(typed.kind, 'capability');
  assert.equal(typed.cause, original);
  assert.deepEqual(translate.map(t => readableError(typed, t)), graphics);
  assert.deepEqual(translate.map(t => readableError(original, t)), graphics);
});

test('build diagnostics reach the editor with their original identity and coordinates', () => {
  const diagnostics = [{line: 3, col: 4, code: 'invalid-config-value', message: 'canonical'}];
  const typed = playbackError({src: '/content/scene.amx', kind: 'build', message: 'canonical', diagnostics}, 'fallback');
  assert.equal(typed.kind, 'build');
  assert.equal(typed.src, '/content/scene.amx');
  assert.equal(diagnosticsFrom(typed), diagnostics);
  assert.equal(readableError(typed, translate[1]), 'canonical');
});

test('a non-graphics capability failure gets its own localized explanation', () => {
  const typed = playbackError({kind: 'capability', message: 'gzip Wasm decoding is unavailable'}, 'fallback');
  assert.equal(readableError(typed, translate[1]), '此浏览器无法启动播放器所需的功能。 gzip Wasm decoding is unavailable');
  assert.equal(readableError(playbackError({message: 'old error'}, 'fallback'), translate[1]), 'old error');
});
