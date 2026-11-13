import { readFileSync, writeFileSync, mkdirSync, readdirSync, copyFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { gzipSync, gunzipSync } from 'node:zlib';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const source = join(root, 'crates/animatix-text/assets/fonts');
const output = join(root, 'web/fonts');
const policy = JSON.parse(readFileSync(join(output, 'manifest.json')));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const check = process.argv[2] === '--check';
if (policy.schema_version !== 2 || !Array.isArray(policy.sources)) throw new Error('invalid font catalog policy');
mkdirSync(output, { recursive: true });
for (const file of policy.sources) {
  if (resolve(source, file) !== join(source, file) || file.includes('/')) throw new Error('invalid font source');
  const raw = readFileSync(join(source, file));
  const path = join(output, `${file}.gz`);
  if (check) {
    if (hash(gunzipSync(readFileSync(path))) !== hash(raw)) throw new Error(`font content mismatch: ${file}`);
  } else writeFileSync(path, gzipSync(raw, { level: 9 }));
}
if (!check) {
  for (const file of readdirSync(source).filter(file => file.startsWith('LICENSE'))) copyFileSync(join(source, file), join(output, file));
}
process.stdout.write(`font pack ${check ? 'verified' : 'synced'}: ${policy.sources.length} faces\n`);
