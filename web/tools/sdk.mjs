// One manifest owns SDK identity, completeness, compression and package layout.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync, lstatSync, mkdirSync, writeFileSync, copyFileSync, renameSync, rmSync } from 'node:fs';
import { join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync, gunzipSync } from 'node:zlib';
import { bundleEngine } from './bundle-engine.mjs';
import { writeThirdPartyLicenses } from './third-party-licenses.mjs';

export const ROOT = fileURLToPath(new URL('../../', import.meta.url));
export const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const run = (command, args) => execFileSync(command, args, { cwd: ROOT, encoding: 'utf8' }).trim();

export function inputs(root = ROOT) {
  // Cargo reads these configuration files even when a developer ignores them.
  const explicit = ['LICENSE', '.cargo/config', '.cargo/config.toml'];
  const paths = execFileSync('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', ...explicit, 'crates', 'vendor', 'tools', 'web/embed/src', 'web/demos/lib', 'web/fonts/manifest.json', 'web/tools', 'Cargo.toml', 'Cargo.lock', 'flake.nix', 'flake.lock', 'scripts'], { cwd: root, encoding: 'utf8' }).split('\0');
  const hash = createHash('sha256');
  for (const file of [...new Set([...paths, ...explicit])].filter(Boolean).sort()) {
    if (!existsSync(join(root, file)) || !statSync(join(root, file)).isFile()) continue;
    const bytes = readFileSync(join(root, file));
    hash.update(`${file.length}:${file}:${bytes.length}:`); hash.update(bytes);
  }
  return hash.digest('hex');
}

export function buildEnvironment(environment = process.env) {
  const relevant = /^(?:RUSTFLAGS|RUSTC|RUSTC_BOOTSTRAP|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTUP_TOOLCHAIN|CARGO_INCREMENTAL|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_.*|CARGO_PROFILE_.*|CARGO_TARGET_.*|CC(?:_.*)?|CXX(?:_.*)?|AR(?:_.*)?|CFLAGS(?:_.*)?|CXXFLAGS(?:_.*)?|ARFLAGS(?:_.*)?|SOURCE_DATE_EPOCH)$/;
  return Object.fromEntries(Object.entries(environment).filter(([key]) => relevant.test(key)).sort(([a], [b]) => a.localeCompare(b)));
}

export function identity(profile = 'release') {
  const data = {
    schema_version: 1, profile, input_digest: inputs(),
    source_commit: run('git', ['rev-parse', 'HEAD']),
    source_dirty: run('git', ['status', '--porcelain', '--untracked-files=normal']) !== '',
    tools: {
      rustc: run(process.env.RUSTC || 'rustc', ['-vV']), bindgen: run('wasm-bindgen', ['--version']),
      optimizer: run('wasm-opt', ['--version']), splitter: '0.8.0-alpha.3+animatix.1',
      // The flake pins the compressor; fingerprint its executable as the CLI
      // does not expose a version command. Only the base transfer needs its
      // slower build-time search to preserve the startup download budget.
      zopfli_sha256: digest(readFileSync(run('sh', ['-c', 'command -v zopfli']))),
      node: process.version, zlib: process.versions.zlib,
    },
    flags: { simd128: true, lto: 'fat', codegen_units: 1, relocation_graph: true, wasm_bindgen_no_demangle: true, base_gzip: 'zopfli --i20 --gzip -c', environment_digest: digest(JSON.stringify(buildEnvironment())) },
  };
  // The embedded engine_commit changes with HEAD; dirtiness itself is only
  // provenance, while every relevant edited byte participates in input_digest.
  const { source_dirty, ...content } = data;
  return { ...data, build_id: digest(JSON.stringify(content)) };
}

function allFiles(directory, prefix = '') {
  return readdirSync(directory).sort().flatMap(name => {
    const path = join(directory, name); const relative = `${prefix}${name}`;
    const info = lstatSync(path);
    if (info.isSymbolicLink()) throw new Error(`SDK contains a symlink: ${relative}`);
    return info.isDirectory() ? allFiles(path, `${relative}/`) : [relative];
  });
}

function versionModules(directory, id) {
  const mapping = {};
  for (const file of allFiles(directory).filter(file => /\.(wasm|js)$/.test(file))) {
    mapping[file] = file.replace(/\.(wasm|js)$/, `.${id}.$1`);
  }
  for (const file of allFiles(directory).filter(file => /\.(js|json)$/.test(file))) {
    let source = readFileSync(join(directory, file), 'utf8');
    for (const [oldName, newName] of Object.entries(mapping)) {
      if (oldName.endsWith('.js')) {
        // Module specifiers change, but Wasm import namespace keys must retain
        // the exact original string encoded in the linked binary.
        source = source.replaceAll(`from './${oldName}'`, `from './${newName}'`)
          .replaceAll(`from "./${oldName}"`, `from "./${newName}"`);
      } else source = source.replaceAll(oldName, newName);
    }
    writeFileSync(join(directory, file), source);
  }
  for (const [oldName, newName] of Object.entries(mapping)) renameSync(join(directory, oldName), join(directory, newName));
  return mapping;
}

export async function assemble(directory, metadataPath, buildIdentity) {
  const source = join(ROOT, 'crates/animatix-text/assets/fonts');
  const policy = JSON.parse(readFileSync(join(ROOT, 'web/fonts/manifest.json')));
  const metadata = JSON.parse(readFileSync(metadataPath));
  await bundleEngine(directory, buildIdentity.build_id);
  // The bundled module includes the split glue; retaining the source glue as a
  // package file would invite consumers to bypass the integrity-checked entry.
  const splitGlue = join(directory, '__wasm_split.js');
  if (existsSync(splitGlue)) rmSync(splitGlue);
  const versioned = versionModules(directory, buildIdentity.build_id);
  const split = JSON.parse(readFileSync(join(directory, 'split-manifest.json')));
  for (const [name, capability] of Object.entries(split.capabilities)) {
    const selected = new Set();
    const collect = id => {
      const chunk = split.chunks.find(chunk => chunk.id === id);
      if (!chunk) throw new Error(`capability dependency is missing: ${id}`);
      if (selected.has(chunk.file)) return;
      selected.add(chunk.file);
      chunk.dependencies.forEach(collect);
    };
    collect(name);
    capability.files = [...selected].sort();
    delete capability.dependencies;
  }
  const version = readFileSync(join(ROOT, 'Cargo.toml'), 'utf8').match(/\[workspace.package\][\s\S]*?version = "([^"]+)"/)[1];
  writeThirdPartyLicenses(directory);
  mkdirSync(join(directory, 'fonts'), { recursive: true });
  const faces = policy.sources.map(file => {
    const info = metadata.find(face => face.file === file);
    if (!info) throw new Error(`no parsed metadata for ${file}`);
    const raw = readFileSync(join(source, file));
    const url = `fonts/${file}.${digest(raw)}.gz`;
    const compressed = gzipSync(raw, { level: 9 });
    writeFileSync(join(directory, url), compressed);
    return { ...info, id: digest(raw), url, sha256: digest(raw), size_bytes: raw.length, transfer_bytes: compressed.length };
  });
  for (const file of readdirSync(source).filter(file => file.startsWith('LICENSE') || file === 'README.md')) copyFileSync(join(source, file), join(directory, 'fonts', file));
  copyFileSync(join(ROOT, 'LICENSE'), join(directory, 'LICENSE'));
  copyFileSync(join(ROOT, 'web/embed/amx-player.js'), join(directory, 'amx-player.js'));
  for (const file of allFiles(directory).filter(file => /\.(?:wasm|js)$/.test(file))) {
    const compressed = file === versioned['animatix_web_bg.wasm']
      ? execFileSync('zopfli', ['--i20', '--gzip', '-c', join(directory, file)], { maxBuffer: 32 * 1024 * 1024 })
      : gzipSync(readFileSync(join(directory, file)), { level: 9 });
    writeFileSync(join(directory, `${file}.gz`), compressed);
  }
  const files = {};
  for (const file of allFiles(directory)) {
    const bytes = readFileSync(join(directory, file));
    files[file] = { url: file, sha256: digest(bytes), size_bytes: bytes.length,
      content_type: file.endsWith('.wasm') ? 'application/wasm' : file.endsWith('.js') ? 'text/javascript' : file.endsWith('.gz') ? 'application/gzip' : 'application/octet-stream',
      ...(existsSync(join(directory, `${file}.gz`)) ? { gzip_url: `${file}.gz` } : {}),
    };
  }
  const manifest = { schema_version: 1, version, build_id: buildIdentity.build_id, build: buildIdentity,
    entry: versioned['animatix_web.js'], wasm: versioned['animatix_web_bg.wasm'], runtime_glue: versioned['animatix_web.js'], files, capabilities: split.capabilities,
    fonts: { faces, fallbacks: policy.fallbacks.map(file => faces.find(face => face.file === file).id) },
  };
  writeFileSync(join(directory, 'runtime-manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  verify(directory);
  return manifest;
}

export function verify(directory, { release = false, current = false, commit, version } = {}) {
  const manifest = JSON.parse(readFileSync(join(directory, 'runtime-manifest.json')));
  if (manifest.schema_version !== 1 || !/^[0-9a-f]{64}$/.test(manifest.build_id)) throw new Error('unsupported SDK identity/schema');
  if (!manifest.build || manifest.build.build_id !== manifest.build_id) throw new Error('SDK build identity mismatch');
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(manifest.version)) throw new Error('invalid SDK version');
  if (version && manifest.version !== version) throw new Error('SDK version mismatch');
  if (!manifest.files || Array.isArray(manifest.files) || !manifest.capabilities || !Array.isArray(manifest.fonts?.faces) || !Array.isArray(manifest.fonts?.fallbacks)) throw new Error('incomplete SDK manifest');
  if (manifest.runtime_glue !== manifest.entry) throw new Error('SDK runtime glue must be bundled into the integrity-checked entry');
  if (release && manifest.build.profile !== 'release') throw new Error('development SDK cannot be published');
  if (current && manifest.build.input_digest !== inputs()) throw new Error('SDK inputs differ from current sources');
  if (commit && manifest.build.source_commit !== commit) throw new Error('SDK commit does not match artifact checkout');
  for (const file of [manifest.entry, manifest.wasm, 'amx-player.js', 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'THIRD-PARTY-PACKAGES.json', 'THIRD-PARTY-SBOM.spdx.json']) if (!manifest.files[file]) throw new Error(`SDK is missing ${file}`);
  for (const [file, info] of Object.entries(manifest.files)) {
    const path = resolve(directory, file);
    if (relative(resolve(directory), path).startsWith('..')) throw new Error('SDK manifest path escapes package');
    if (!info || info.url !== file || !/^[0-9a-f]{64}$/.test(info.sha256) || !Number.isSafeInteger(info.size_bytes) || info.size_bytes < 0) throw new Error(`invalid SDK file descriptor: ${file}`);
    if (info.gzip_url && (relative(resolve(directory), resolve(directory, info.gzip_url)).startsWith('..') || !manifest.files[info.gzip_url])) throw new Error('gzip path escapes package or is missing');
    const bytes = readFileSync(path);
    if (digest(bytes) !== info.sha256 || bytes.length !== info.size_bytes) throw new Error(`SDK file hash/size mismatch: ${file}`);
    if (info.gzip_url && !gunzipSync(readFileSync(join(directory, info.gzip_url))).equals(bytes)) throw new Error(`gzip twin differs: ${file}`);
  }
  if (allFiles(directory).filter(file => file !== 'runtime-manifest.json').some(file => !manifest.files[file])) throw new Error('SDK contains unlisted files');
  for (const [name, mask] of Object.entries({ rich_text: 1, media: 6, raster_encode: 8 })) {
    const capability = manifest.capabilities[name];
    if (!capability || capability.mask !== mask || !Array.isArray(capability.files) || !capability.files.length || capability.files.some(file => !manifest.files[file])) throw new Error(`missing capability: ${name}`);
  }
  if (Object.keys(manifest.capabilities).sort().join(',') !== 'media,raster_encode,rich_text') throw new Error('unknown SDK capability');
  const faceIds = new Set();
  for (const face of manifest.fonts.faces) {
    if (!face || !/^[0-9a-f]{64}$/.test(face.id) || faceIds.has(face.id) || face.id !== face.sha256 || !manifest.files[face.url] || !Number.isSafeInteger(face.size_bytes) || face.size_bytes < 0) throw new Error('invalid font descriptor');
    faceIds.add(face.id);
    const raw = gunzipSync(readFileSync(join(directory, face.url)));
    if (digest(raw) !== face.sha256 || raw.length !== face.size_bytes) throw new Error(`font content mismatch: ${face.file}`);
  }
  if (manifest.fonts.fallbacks.some(id => !faceIds.has(id))) throw new Error('unknown fallback font');
  const inventory = JSON.parse(readFileSync(join(directory, 'THIRD-PARTY-PACKAGES.json')));
  if (inventory.target !== 'wasm32-unknown-unknown' || !Array.isArray(inventory.packages) || !inventory.packages.length || !Array.isArray(inventory.missingLicenseText)) throw new Error('invalid SDK license inventory');
  const sbom = JSON.parse(readFileSync(join(directory, 'THIRD-PARTY-SBOM.spdx.json')));
  if (sbom.spdxVersion !== 'SPDX-2.3' || !Array.isArray(sbom.packages) || !sbom.packages.length) throw new Error('invalid SDK SPDX inventory');
  return manifest;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...args] = process.argv.slice(2);
  if (command === 'identity') process.stdout.write(`${JSON.stringify(identity(args[0] || 'release'), null, 2)}\n`);
  else if (command === 'assemble') await assemble(args[0], args[1], JSON.parse(readFileSync(args[2])));
  else if (command === 'verify') {
    const manifest = verify(args[0], { release: args.includes('--release'), current: args.includes('--current'), commit: args.includes('--commit') ? args[args.indexOf('--commit') + 1] : undefined, version: args.includes('--version') ? args[args.indexOf('--version') + 1] : undefined });
    process.stdout.write(`SDK verified: ${manifest.build_id}\n`);
  } else throw new Error('usage: sdk.mjs identity [profile] | assemble DIR FONTS IDENTITY | verify DIR [--release --current --commit SHA --version VERSION]');
}
