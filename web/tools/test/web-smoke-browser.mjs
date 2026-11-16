export const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
export async function until(label, probe, timeout = 75000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await probe()) return;
    await sleep(150);
  }
  throw new Error(`Timed out waiting for ${label} (${timeout}ms)`);
}
export class Cdp {
  constructor(url) { this.ws = new WebSocket(url); this.nextId = 0; this.pending = new Map(); }
  async open() {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('CDP open timeout')), 10000);
      this.ws.addEventListener('open', () => { clearTimeout(timer); resolve(); }, {once: true});
      this.ws.addEventListener('error', () => { clearTimeout(timer); reject(new Error('CDP open failed')); }, {once:true});
    });
    this.ws.addEventListener('close', () => this.rejectPending('CDP closed'));
    this.ws.addEventListener('error', () => this.rejectPending('CDP transport failed'));
    this.ws.addEventListener('message', event => {
      const message = JSON.parse(event.data);
      if (message.method) this.onEvent?.(message);
      const entry = this.pending.get(message.id);
      if (!entry) return;
      clearTimeout(entry.timer); this.pending.delete(message.id);
      if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
      else entry.resolve(message.result);
    });
  }
  rejectPending(message) {
    for (const entry of this.pending.values()) { clearTimeout(entry.timer); entry.reject(new Error(message)); }
    this.pending.clear();
  }
  send(method, params = {}, timeout = 15000) {
    return new Promise((resolve, reject) => {
      const id = ++this.nextId;
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP ${method} timeout`)); }, timeout);
      this.pending.set(id, {resolve, reject, timer});
      this.ws.send(JSON.stringify({id, method, params}));
    });
  }
  async evaluate(expression) {
    const value = await this.send('Runtime.evaluate', {expression, awaitPromise:true, returnByValue:true});
    if (value.exceptionDetails) throw new Error(value.exceptionDetails.exception?.description || value.exceptionDetails.text);
    return value.result.value;
  }
  close() { this.rejectPending('CDP disposed'); this.ws.close(); }
}
// Injected before navigation: readiness is recorded even before a journey selects
// its player. No dependency on private component state or eager-load bypasses.
export const instrumentation = `
window.__smoke = {ready: new WeakSet(), events: [], errors: []};
addEventListener('amxready', e => {
  __smoke.ready.add(e.target);
  __smoke.events.push({kind:'ready',src:e.target.getAttribute('src'),ms:performance.now()});
}, true);
addEventListener('amxerror', e => __smoke.errors.push(e.detail?.message || 'amxerror'), true);
addEventListener('error', e => __smoke.errors.push(e.message || 'resource error'), true);
addEventListener('unhandledrejection', e => __smoke.errors.push(String(e.reason)), true);
`;

// Navigation destroys the previous execution context; retry only that race,
// only inside the bounded commit wait. Journey exceptions remain hard failures.
export async function navigationCommitted(cdp, url, timeout = 15000) {
  await until('navigation committed', async () => {
    try { return await cdp.evaluate(`location.href===${JSON.stringify(url)}`); }
    catch (error) {
      if (/Execution context was destroyed|Cannot find context with specified id|Inspected target navigated or closed/.test(error.message)) return false;
      throw error;
    }
  }, timeout);
}
