import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {legacyWorks} from '../src/lib/legacy-works.ts';

const web = new URL('../../', import.meta.url);
test('every former showcase has one explicit disposition', () => {
  const old = JSON.parse(readFileSync(new URL('works.json',web),'utf8')).works;
  assert.deepEqual(legacyWorks.map(w=>w.id).sort(),old.map(w=>w.id).sort());
  assert.equal(new Set(legacyWorks.map(w=>w.id)).size,legacyWorks.length);
  for (const work of legacyWorks) {
    assert.ok(work.decision && work.reason && work.caveat && work.nextLabel);
    assert.ok(!work.next.startsWith('/') && !work.next.includes('..'));
    assert.ok(existsSync(fileURLToPath(new URL(`demos/${work.id}/${work.source}`,web))),work.id);
  }
});
test('legacy landing pages never embed an unreviewed player', () => {
  for (const path of ['src/pages/demos/[id].astro','src/pages/archive.astro','src/pages/demos/index.astro']) {
    const source=readFileSync(new URL(`../${path}`,import.meta.url),'utf8');
    assert.doesNotMatch(source,/<amx-player|<Scene\b|autoplay/);
    assert.match(source,/url\(/);
  }
});
