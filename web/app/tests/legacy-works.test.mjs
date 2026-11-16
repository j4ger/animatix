import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {legacyWorks,legacyRevision} from '../src/lib/legacy-works.ts';

test('every former showcase has one explicit disposition', () => {
  // Frozen public URL contract; do not keep an obsolete site's manifest alive.
  assert.deepEqual(legacyWorks.map(w=>w.id).sort(), ['epicycles','gradient','hash','kinetic','matrix','morph','sorting','topology','transformer']);
  assert.equal(legacyRevision, 'aa4d8d1a01633ba84d7810329735f3286096d5e1');
  assert.equal(new Set(legacyWorks.map(w=>w.id)).size,legacyWorks.length);
  for (const work of legacyWorks) {
    assert.ok(work.decision && work.reason && work.caveat && work.nextLabel);
    assert.ok(!work.next.startsWith('/') && !work.next.includes('..'));
    assert.ok(work.source && !work.source.startsWith('/') && !work.source.includes('..'));
  }
});
test('legacy landing pages never embed an unreviewed player', () => {
  for (const path of ['src/pages/demos/[id].astro','src/pages/archive.astro','src/pages/demos/index.astro']) {
    const source=readFileSync(new URL(`../${path}`,import.meta.url),'utf8');
    assert.doesNotMatch(source,/<amx-player|<Scene\b|autoplay/);
    assert.match(source,/url\(/);
  }
});

test('archive source links point at the immutable historical revision', () => {
  const source = readFileSync(new URL('../src/pages/demos/[id].astro', import.meta.url), 'utf8');
  assert.match(source, /tree\/\$\{legacyRevision\}\/web\/demos\//);
});
