import assert from 'node:assert/strict';
import test from 'node:test';
import {beforeLanguageSwitch, takeEditorTransfer, transferEditor} from '../src/lib/editor-transfer.ts';

function fakePage(t, sessionStorage) {
  const oldWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const oldStorage = Object.getOwnPropertyDescriptor(globalThis, 'sessionStorage');
  Object.defineProperty(globalThis, 'window', {value: new EventTarget(), configurable: true});
  Object.defineProperty(globalThis, 'sessionStorage', {value: sessionStorage, configurable: true});
  t.after(() => {
    if (oldWindow) Object.defineProperty(globalThis, 'window', oldWindow);
    else delete globalThis.window;
    if (oldStorage) Object.defineProperty(globalThis, 'sessionStorage', oldStorage);
    else delete globalThis.sessionStorage;
  });
}

const draft = {
  scene: 'insertion-sort', source: 'incomplete edit', compiled: '#0s',
  time: 6.95, selection: [3, 8], scroll: 40, sourceOpen: true,
};

test('language switching carries both the editable draft and the last good preview once', t => {
  const stored = new Map();
  fakePage(t, {
    setItem: (...pair) => stored.set(...pair),
    getItem: key => stored.get(key),
    removeItem: key => stored.delete(key),
  });
  let ready = false;
  transferEditor('home', () => draft, () => { ready = true; });
  assert.equal(beforeLanguageSwitch(), true);
  assert.equal(ready, true);
  assert.deepEqual(takeEditorTransfer('home'), draft);
  assert.equal(takeEditorTransfer('home'), undefined);
});

test('language switching is cancelled when the draft cannot be stored', t => {
  fakePage(t, {setItem() { throw new Error('Storage unavailable'); }});
  transferEditor('playground', () => draft, () => assert.fail('Did not preserve the draft'));
  assert.equal(beforeLanguageSwitch(), false);
});
