import test from 'node:test';
import assert from 'node:assert/strict';
import { combinePrimaryVerdict, comparePaired } from './perf-statistics.mjs';
import { contentType, parseScenes } from '../../scripts/web-runtime-perf.mjs';

const compare = (oldValues, newValues, absoluteThreshold, options = {}) => comparePaired(oldValues, newValues, {
  threshold: 1.05,
  absoluteThreshold,
  required: true,
  ...options,
});

test('a small but stable ten percent change below the absolute floor is informational', () => {
  const result = compare([100, 100, 100, 100, 100, 100], [110, 110, 110, 110, 110, 110], 50);
  assert.equal(result.status, 'no-regression');
  assert.equal(result.estimate_ratio, 1.1);
  assert.deepEqual(result.ratio_ci95, [1.1, 1.1]);
  assert.deepEqual(result.delta_ci95, [10, 10]);
  assert.equal(combinePrimaryVerdict([result]), 'no-regression');
});

test('a material startup slowdown clears both relative and absolute floors', () => {
  const result = compare([1000, 1000, 1000, 1000, 1000, 1000], [1100, 1100, 1100, 1100, 1100, 1100], 50);
  assert.equal(result.status, 'regression');
  assert.ok(result.ratio_ci95[0] > 1.05);
  assert.ok(result.delta_ci95[0] > 50);
});

test('a material queue-depth-one frame slowdown clears its frame-budget floor', () => {
  const result = compare([10, 10, 10, 10, 10, 10], [10.8, 10.8, 10.8, 10.8, 10.8, 10.8], 1000 / 60 / 100);
  assert.equal(result.status, 'regression');
  assert.ok(result.ratio_ci95[0] > 1.05);
  assert.ok(result.delta_ci95[0] > 1000 / 60 / 100);
});

test('uncertainty overlapping both floors remains inconclusive', () => {
  const result = compare([10, 10, 10, 10, 10, 10], [10, 12, 9, 12, 9, 12], 1000 / 60 / 100);
  assert.equal(result.status, 'inconclusive');
  assert.ok(result.ratio_ci95[0] <= 1.05 && result.ratio_ci95[1] > 1.05);
  assert.ok(result.delta_ci95[0] <= 1000 / 60 / 100 && result.delta_ci95[1] > 1000 / 60 / 100);
  assert.equal(combinePrimaryVerdict([result]), 'inconclusive');
});

test('missing required primary samples cannot pass the combined verdict', () => {
  const missing = compare([null, null, null], [null, null, null], 50);
  assert.equal(missing.status, 'inconclusive');
  assert.equal(missing.n, 0);
  assert.equal(missing.expected_n, 3);
  assert.deepEqual(missing.skipped_indices, [0, 1, 2]);
  assert.equal(combinePrimaryVerdict([missing]), 'inconclusive');
});

test('one or two exact pairs are undersampled rather than a false no-regression', () => {
  for (const count of [1, 2]) {
    const values = Array(count).fill(100);
    const required = compare(values, values, 50);
    const optional = comparePaired(values, values, { required: false });
    assert.equal(required.status, 'inconclusive');
    assert.equal(optional.status, 'unverified');
    assert.equal(required.n, count);
    assert.equal(required.minimum_samples, 3);
    assert.ok(required.reason.includes('valid pairs'));
    assert.ok(combinePrimaryVerdict([required]) !== 'no-regression');
  }
});

test('incomplete required pairs remain visible and cannot pass', () => {
  const incomplete = compare([100, 100, null, 100], [100, 100, 130, 100], 10);
  assert.equal(incomplete.status, 'inconclusive');
  assert.equal(incomplete.n, 3);
  assert.equal(incomplete.expected_n, 4);
  assert.deepEqual(incomplete.skipped_indices, [2]);
  assert.equal(combinePrimaryVerdict([incomplete]), 'inconclusive');
});

test('unsupported optional measurements are reported unverified without joining primary verdicts', () => {
  const optional = comparePaired([null, undefined], [null, 12], { required: false });
  const primary = compare([100, 100, 100], [100, 100, 100], 50);
  assert.equal(optional.status, 'unverified');
  assert.deepEqual(optional.skipped_indices, [0, 1]);
  assert.equal(combinePrimaryVerdict([primary]), 'no-regression');
});

test('higher-is-better metrics express absolute loss as a positive cost delta', () => {
  const result = comparePaired([100, 100, 100, 100, 100, 100], [80, 80, 80, 80, 80, 80], {
    threshold: 1.05,
    absoluteThreshold: 10,
    higherIsBetter: true,
    required: true,
  });
  assert.equal(result.status, 'regression');
  assert.ok(result.ratio_ci95[0] > 1.05);
  assert.deepEqual(result.delta_ci95, [20, 20]);
});

test('paired arrays and configured margins are validated', () => {
  assert.throws(() => comparePaired([1], [1, 2]), /equal lengths/);
  assert.throws(() => comparePaired([1], [1], { threshold: 0.9 }), /at least 1/);
  assert.throws(() => comparePaired([1], [1], { absoluteThreshold: -1 }), /non-negative/);
});

test('empty Web performance scene lists fail closed and bounded scenes are retained', () => {
  assert.equal(parseScenes().length, 3);
  assert.deepEqual(parseScenes('english.amx, cjk.amx'), ['english.amx', 'cjk.amx']);
  assert.throws(() => parseScenes(', ,'), /at least one scene/);
  assert.throws(() => parseScenes(Array(11).fill('scene.amx').join(',')), /at most 10 scenes/);
});

test('Web performance server serves generated HTML pages as documents', () => {
  assert.equal(contentType('/old-page.html'), 'text/html; charset=utf-8');
  assert.equal(contentType('/fresh/runtime/amx-player.js'), 'text/javascript');
  assert.equal(contentType('/fresh/runtime/runtime.wasm'), 'application/wasm');
});
