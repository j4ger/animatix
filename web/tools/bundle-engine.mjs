import { writeFileSync, renameSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import * as esbuild from 'esbuild';

// Make one integrity-checked module. Keeping the output at the SDK root makes
// import.meta.url in wasm-bindgen glue continue to resolve the adjacent Wasm.
export async function bundleEngine(directory, buildId) {
  const wrapper = join(directory, '.animatix-sdk-entry.mjs');
  const output = join(directory, '.animatix-sdk-entry.bundle.mjs');
  const entry = join(directory, 'animatix_web.js');
  const registration = `\nimport * as engine from './animatix_web.js';\n` +
    `globalThis[Symbol.for('animatix.sdk.engine.' + ${JSON.stringify(buildId)})] = engine;\n`;
  writeFileSync(wrapper, registration);
  try {
    await esbuild.build({
      entryPoints: [wrapper], outfile: output, bundle: true, format: 'esm',
      platform: 'browser', target: 'es2022', sourcemap: false, metafile: true,
      minify: true, legalComments: 'eof',
    });
    renameSync(output, entry);
  } finally {
    rmSync(wrapper, { force: true });
    rmSync(output, { force: true });
  }
}
