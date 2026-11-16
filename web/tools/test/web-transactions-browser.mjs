import {chromiumLaunch} from './web-smoke-launch.mjs';
import {visualPreflight} from './web-smoke-preflight.mjs';

export function transactionLaunch(chrome, profile, hardware = false) {
  // CI shares the exact smoke compositor, not a second software GPU recipe.
  if (!hardware) return chromiumLaunch(chrome, profile, 0);
  // Explicit local hardware acceptance retains its real desktop/GPU selection.
  return {command: chrome, args: [
    '--no-sandbox', '--disable-dev-shm-usage', '--disable-gpu-sandbox', '--enable-unsafe-webgpu',
    '--use-vulkan=native', '--enable-features=Vulkan,WebGPUService', '--ignore-gpu-blocklist',
    '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-extensions',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
  ]};
}

export async function transactionPreflight(cdp, save, paint = visualPreflight) {
  const adapter = await cdp.evaluate(`(async()=>{
    const a=await navigator.gpu?.requestAdapter();
    return a?{vendor:a.info?.vendor,architecture:a.info?.architecture,device:a.info?.device,description:a.info?.description}:null;
  })()`);
  if (!adapter) throw Error('Environment preflight: WebGPU adapter unavailable before transactions');
  return {adapter, compositorPixels: await paint(cdp, save)};
}

export async function stopBrowser(child, signalGroup = (pid, signal) => process.kill(-pid, signal)) {
  if (!child.pid) return; // Spawn failed; no process group was created.
  // Signal the detached group even if its wrapper has exited: descendants can
  // remain. SIGTERM lets xvfb-run remove its Xauthority/temp directory first.
  try { signalGroup(child.pid, 'SIGTERM'); } catch { return; }
  await new Promise(resolve => setTimeout(resolve, 300));
  try { signalGroup(child.pid, 'SIGKILL'); } catch { /* group already exited */ }
}
