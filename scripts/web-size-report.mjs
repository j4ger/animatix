import { existsSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { verify } from '../web/tools/sdk.mjs';

let check = false;
let site = '_site';
for (let index = 2; index < process.argv.length; index += 1) {
  const arg = process.argv[index];
  if (arg === '--check') check = true;
  else if (arg === '--site' && process.argv[index + 1]) site = process.argv[++index];
  else throw new Error('usage: size-report.sh [--check] [--site DIR]');
}
const sdk = 'web/runtime';
if (!existsSync(join(sdk, 'runtime-manifest.json'))) {
  if (check) throw new Error('SDK manifest is missing; size verification cannot pass');
  process.stdout.write('SDK missing\n');
  process.exit(0);
}
const manifest = verify(sdk);
const size = file => manifest.files[file]?.size_bytes ?? (() => { throw new Error(`unlisted SDK file ${file}`); })();
const primary = size(manifest.wasm);
const transfer = size(manifest.files[manifest.wasm].gzip_url);
const wasmFiles = Object.keys(manifest.files).filter(file => file.endsWith('.wasm'));
const wasm = wasmFiles.reduce((sum, file) => sum + size(file), 0);
const fonts = manifest.fonts.faces.reduce((sum, face) => sum + size(face.url), 0);
const sdkBytes = Object.values(manifest.files).reduce((sum, file) => sum + file.size_bytes, 0)
  + statSync(join(sdk, 'runtime-manifest.json')).size;
// Preserve the raw module/font ceilings for the single linked program. The
// transfer allowance is calibrated against the verified 1.702 MB base module,
// with modest headroom; startup and frame performance have separate A/B checks.
const budgets = { primary: 5_400_000, primary_gzip: 1_750_000, wasm_total: 28_000_000, font_gzip: 2_400_000, sdk: 42_000_000, site: 58_000_000 };
const measured = { primary, primary_gzip: transfer, wasm_total: wasm, font_gzip: fonts, sdk: sdkBytes };
function directoryBytes(directory) {
  return readdirSync(directory).reduce((sum, name) => {
    const path = join(directory, name);
    return sum + (statSync(path).isDirectory() ? directoryBytes(path) : statSync(path).size);
  }, 0);
}
if (existsSync(site)) measured.site = directoryBytes(site);
for (const [label, bytes] of Object.entries(measured)) {
  process.stdout.write(`${label.padEnd(16)} ${String(bytes).padStart(10)} bytes (budget ${budgets[label]})\n`);
  if (check && bytes > budgets[label]) throw new Error(`size budget exceeded: ${label} ${bytes} > ${budgets[label]}`);
}
