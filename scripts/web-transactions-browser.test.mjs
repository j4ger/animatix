import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {transactionLaunch, transactionPreflight, stopBrowser} from './web-transactions-browser.mjs';
import {chromiumLaunch} from './web-smoke-launch.mjs';

test('software transactions share the exact smoke launch; hardware remains opt-in', () => {
  assert.deepEqual(transactionLaunch('/chrome', '/profile'), chromiumLaunch('/chrome', '/profile', 0));
  const hardware = transactionLaunch('/chrome', '/profile', true);
  assert.equal(hardware.command, '/chrome');
  assert.ok(hardware.args.includes('--use-vulkan=native'));
  assert.ok(!hardware.args.some(flag => /swiftshader|headless/.test(flag)));
});
test('adapter absence is terminal and never proceeds to paint or transactions', async () => {
  let calls = 0;
  await assert.rejects(transactionPreflight({evaluate: async () => {calls++; return null;}}, () => {}, () => {throw Error('must not paint');}), /Environment preflight.*adapter unavailable/);
  assert.equal(calls, 1);
});
test('adapter alone is insufficient and compositor error is not retried', async () => {
  let paints = 0;
  await assert.rejects(transactionPreflight({evaluate: async () => ({vendor:'test'})}, () => {}, async () => {paints++; throw Error('blank compositor');}), /blank compositor/);
  assert.equal(paints, 1);
});
test('cleanup signals the entire detached group including an exited wrapper', async () => {
  const signals=[];
  await stopBrowser({pid:123,exitCode:0}, (pid,signal)=>signals.push([pid,signal]));
  assert.deepEqual(signals, [[123,'SIGTERM'],[123,'SIGKILL']]);
  await stopBrowser({}, ()=>assert.fail('spawn failure has no process group'));
});
test('runner preflights before unchanged transaction suite and records failure stage', () => {
  const runner=readFileSync(new URL('./web-transactions.mjs',import.meta.url),'utf8');
  assert.ok(runner.indexOf('preflight = await transactionPreflight') < runner.indexOf('report = await Promise.race'));
  assert.match(runner, /retryable: false/);
  assert.match(runner, /await stopBrowser\(chrome\)/);
  assert.doesNotMatch(runner, /--use-angle=vulkan|--disable-vulkan-surface/);
});
