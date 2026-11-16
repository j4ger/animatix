import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {fileURLToPath} from 'node:url';
import {join, resolve} from 'node:path';
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

// Evaluate only the runner's actual path configuration: no SDK or browser needed.
test('transaction defaults resolve from the checkout, with independent overrides', () => {
  const runnerUrl = new URL('./web-transactions.mjs', import.meta.url);
  const source = readFileSync(runnerUrl, 'utf8');
  const config = source.match(/const ROOT = [^\n]+\nconst SITE = [^\n]+\nconst EVIDENCE = [^\n]+/)[0];
  const root = fileURLToPath(new URL('../../../', runnerUrl));
  const evaluate = env => runInNewContext(
    config.replaceAll('import.meta.url', JSON.stringify(runnerUrl.href)) + '\n({ROOT, SITE, EVIDENCE})',
    {URL, fileURLToPath, join, resolve, process: {env}},
  );
  const defaults = evaluate({});
  assert.equal(defaults.ROOT, root);
  assert.equal(defaults.SITE, join(root, 'web'));
  assert.equal(defaults.EVIDENCE, join(root, 'target/perf/distribution-refactor/web-transactions'));
  for (const env of [
    {ANIMATIX_WEB_ROOT: '/tmp/custom-site'},
    {ANIMATIX_WEB_EVIDENCE: '/tmp/custom-evidence'},
    {ANIMATIX_WEB_ROOT: '/tmp/custom-site', ANIMATIX_WEB_EVIDENCE: '/tmp/custom-evidence'},
  ]) {
    const paths = evaluate(env);
    assert.equal(paths.ROOT, root);
    assert.equal(paths.SITE, env.ANIMATIX_WEB_ROOT || defaults.SITE);
    assert.equal(paths.EVIDENCE, env.ANIMATIX_WEB_EVIDENCE || defaults.EVIDENCE);
  }
});
