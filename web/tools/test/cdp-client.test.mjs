import assert from 'node:assert/strict';
import test from 'node:test';
import { Cdp } from './cdp-client.mjs';

test('evaluate forwards explicit suite budget without changing quick-call default', async () => {
  const client = Object.create(Cdp.prototype);
  const calls = [];
  client.send = async (...args) => { calls.push(args); return { result: { value: 42 } }; };
  assert.equal(await client.evaluate('longAsyncSuite()', 1_200_000), 42);
  assert.equal(calls[0][2], 1_200_000);
  assert.equal(calls[0][1].awaitPromise, true);
  await client.evaluate('document.readyState');
  assert.equal(calls[1][2], 30_000);
});

test('evaluation still reports browser exceptions', async () => {
  const client = Object.create(Cdp.prototype);
  client.send = async () => ({ exceptionDetails: { text: 'browser assertion failed' } });
  await assert.rejects(client.evaluate('bad()'), /browser assertion failed/);
});

test('CDP transport enforces the supplied timeout and removes its pending entry', async () => {
  const client = Object.create(Cdp.prototype);
  client.next = 0; client.pending = new Map(); client.socket = { send() {} };
  await assert.rejects(client.send('Runtime.evaluate', {}, 5), /timed out after 5ms/);
  assert.equal(client.pending.size, 0);
});
