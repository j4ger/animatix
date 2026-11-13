// Tiny fixture bytes test packaging; split-proof and browser gates run code.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync, symlinkSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { gzipSync } from 'node:zlib';
import { verify, digest, buildEnvironment, inputs } from '../web/tools/sdk.mjs';

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), 'animatix-sdk-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const files = {};
  function file(name, bytes) {
    bytes = Buffer.from(bytes);
    writeFileSync(join(dir, name), bytes);
    files[name] = { url: name, sha256: digest(bytes), size_bytes: bytes.length, content_type: 'application/octet-stream' };
    return bytes;
  }
  file('engine.js', 'export const version = "fixture";');
  const wasm = file('engine.wasm', [0, 97, 115, 109, 1, 0, 0, 0]);
  file('engine.wasm.gz', gzipSync(wasm));
  files['engine.wasm'].gzip_url = 'engine.wasm.gz';
  file('amx-player.js', 'export {};');
  file('LICENSE', 'fixture project license');
  file('THIRD-PARTY-NOTICES.txt', 'fixture upstream license notice');
  file('THIRD-PARTY-PACKAGES.json', JSON.stringify({ target: 'wasm32-unknown-unknown', packages: [{ name: 'fixture-runtime', version: '1.0.0' }], missingLicenseText: [] }));
  file('THIRD-PARTY-SBOM.spdx.json', JSON.stringify({ spdxVersion: 'SPDX-2.3', packages: [{ name: 'fixture-runtime', versionInfo: '1.0.0' }] }));
  for (const name of ['rich.wasm', 'media.wasm', 'encode.wasm']) file(name, wasm);
  const font = Buffer.from('fixture font table bytes');
  file('font.gz', gzipSync(font));
  const manifest = { schema_version: 1, version: '0.1.0', build_id: 'a'.repeat(64),
    build: { build_id: 'a'.repeat(64), profile: 'release', source_commit: 'b'.repeat(40), input_digest: 'c'.repeat(64) },
    entry: 'engine.js', wasm: 'engine.wasm', runtime_glue: 'engine.js', files,
    capabilities: { rich_text: { mask: 1, files: ['rich.wasm'] }, media: { mask: 6, files: ['media.wasm'] }, raster_encode: { mask: 8, files: ['encode.wasm'] } },
    fonts: { faces: [{ id: digest(font), url: 'font.gz', sha256: digest(font), size_bytes: font.length, family: 'Fixture', weight: 400, style: 'normal', axes: [], coverage: [[32, 127]] }], fallbacks: [digest(font)] },
  };
  const save = () => writeFileSync(join(dir, 'runtime-manifest.json'), JSON.stringify(manifest));
  save(); return { dir, manifest, save, file };
}

test('complete SDK validates against release, commit and version', t => {
  const { dir } = fixture(t);
  assert.equal(verify(dir, { release: true, commit: 'b'.repeat(40), version: '0.1.0' }).version, '0.1.0');
});

test('SDK archives retain license notices and parseable dependency inventories', t => {
  const { dir, manifest, save } = fixture(t);
  delete manifest.files['THIRD-PARTY-NOTICES.txt']; save();
  assert.throws(() => verify(dir), /missing THIRD-PARTY-NOTICES/);
  const second = fixture(t);
  second.file('THIRD-PARTY-PACKAGES.json', JSON.stringify({ target: 'native', packages: [], missingLicenseText: [] })); second.save();
  assert.throws(() => verify(second.dir), /invalid SDK license inventory/);
});

test('corrupt payload and unlisted development bytes fail closed', t => {
  const { dir } = fixture(t); writeFileSync(join(dir, 'engine.wasm'), 'tampered');
  assert.throws(() => verify(dir), /hash\/size mismatch/);
  const second = fixture(t); writeFileSync(join(second.dir, 'debug-input.amx'), 'not a runtime artifact');
  assert.throws(() => verify(second.dir), /unlisted/);
});

test('SDK payload links cannot depend on files outside the extracted package', t => {
  const { dir } = fixture(t);
  const external = mkdtempSync(join(tmpdir(), 'animatix-sdk-external-'));
  t.after(() => rmSync(external, { recursive: true, force: true }));
  const bytes = readFileSync(join(dir, 'engine.js'));
  writeFileSync(join(external, 'engine.js'), bytes);
  rmSync(join(dir, 'engine.js'));
  symlinkSync(join(external, 'engine.js'), join(dir, 'engine.js'));
  assert.throws(() => verify(dir), /symlink/);
});

test('missing split chunk fails even when other capabilities are complete', t => {
  const { dir } = fixture(t); rmSync(join(dir, 'media.wasm'));
  assert.throws(() => verify(dir), /ENOENT/);
  const second = fixture(t); delete second.manifest.files['media.wasm']; second.save();
  assert.throws(() => verify(second.dir), /unlisted|capability/);
});

test('debug profile, schema, build identity, version and commit mismatches are rejected', t => {
  const { dir, manifest, save } = fixture(t);
  manifest.build.profile = 'dev'; save(); assert.throws(() => verify(dir, { release: true }), /development/);
  manifest.build.profile = 'release'; manifest.schema_version = 99; save(); assert.throws(() => verify(dir), /schema/);
  manifest.schema_version = 1; manifest.build.build_id = 'd'.repeat(64); save(); assert.throws(() => verify(dir), /identity mismatch/);
  manifest.build.build_id = manifest.build_id; save();
  assert.throws(() => verify(dir, { commit: 'e'.repeat(40) }), /commit/);
  assert.throws(() => verify(dir, { version: '0.2.0' }), /version mismatch/);
});

test('runtime glue cannot escape the SRI checked entry module', t => {
  const { dir, manifest, save } = fixture(t);
  manifest.runtime_glue = 'unverified-glue.js'; save();
  assert.throws(() => verify(dir), /runtime glue must be bundled/);
});

test('missing capability, wrong bit mask, omitted gzip and escaping paths are rejected', t => {
  const { dir, manifest, save } = fixture(t);
  delete manifest.capabilities.media; save(); assert.throws(() => verify(dir), /missing capability/);
  manifest.capabilities.media = { mask: 4, files: ['media.wasm'] }; save(); assert.throws(() => verify(dir), /missing capability/);
  manifest.capabilities.media.mask = 6; manifest.files['engine.wasm'].gzip_url = '../outside.gz'; save();
  assert.throws(() => verify(dir), /gzip path/);
  manifest.files['engine.wasm'].gzip_url = 'engine.wasm.gz';
  manifest.files['../escape'] = { url: '../escape', size_bytes: 0, sha256: digest('') }; save();
  assert.throws(() => verify(dir), /escapes package/);
});

test('valid gzip with wrong font bytes does not pass payload checks', t => {
  const { dir, manifest, save } = fixture(t); const changed = gzipSync(Buffer.from('a different font'));
  writeFileSync(join(dir, 'font.gz'), changed);
  manifest.files['font.gz'].sha256 = digest(changed); manifest.files['font.gz'].size_bytes = changed.length; save();
  assert.throws(() => verify(dir), /font content mismatch/);
});

test('truncated gzip is rejected even with an internally consistent file hash', t => {
  const { dir, manifest, save } = fixture(t); const broken = readFileSync(join(dir, 'engine.wasm.gz')).subarray(0, 7);
  writeFileSync(join(dir, 'engine.wasm.gz'), broken);
  manifest.files['engine.wasm.gz'].sha256 = digest(broken); manifest.files['engine.wasm.gz'].size_bytes = broken.length; save();
  assert.throws(() => verify(dir), /unexpected end|buffer error|invalid/i);
});

test('unknown capabilities and invalid font identities cannot enter an artifact', t => {
  const { dir, manifest, save } = fixture(t);
  manifest.capabilities.unknown = { mask: 16, files: ['rich.wasm'] }; save();
  assert.throws(() => verify(dir), /unknown.*capability/);
  delete manifest.capabilities.unknown;
  const font = structuredClone(manifest.fonts.faces[0]);
  manifest.fonts.faces[0].url = '../outside.gz'; save(); assert.throws(() => verify(dir), /font descriptor/);
  manifest.fonts.faces[0] = { ...font, size_bytes: font.size_bytes + 1 }; save();
  assert.throws(() => verify(dir), /font content mismatch/);
  manifest.fonts.faces[0] = font; manifest.fonts.fallbacks = ['d'.repeat(64)]; save();
  assert.throws(() => verify(dir), /fallback/);
  manifest.fonts.fallbacks = [font.id]; manifest.fonts.faces.push(font); save();
  assert.throws(() => verify(dir), /font descriptor/);
});

test('SDK identity captures build-affecting environment without unrelated secrets', () => {
  const env = { TOKEN: 'private', RUSTFLAGS: '-C opt-level=2', CARGO_PROFILE_WASM_RELEASE_LTO: 'fat', CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS: '-C target-feature=+simd128' };
  assert.deepEqual(buildEnvironment(env), { CARGO_PROFILE_WASM_RELEASE_LTO: 'fat', CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS: '-C target-feature=+simd128', RUSTFLAGS: '-C opt-level=2' });
  assert.notDeepEqual(buildEnvironment(env), buildEnvironment({ ...env, RUSTFLAGS: '-C opt-level=3' }));
  assert.notDeepEqual(buildEnvironment(env), buildEnvironment({ ...env, RUSTC: '/other/rustc' }));
  assert.notDeepEqual(buildEnvironment(env), buildEnvironment({ ...env, CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER: '/other/linker' }));
  assert.notDeepEqual(buildEnvironment(env), buildEnvironment({ ...env, CARGO_INCREMENTAL: '1' }));
});

test('SDK identity includes ignored Cargo config, the shipped license, and Unicode source paths', t => {
  const root = mkdtempSync(join(tmpdir(), 'animatix-sdk-inputs-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  execFileSync('git', ['init', '--quiet', root]);
  writeFileSync(join(root, '.gitignore'), '.cargo/\n');
  writeFileSync(join(root, 'LICENSE'), 'initial license');
  const initial = inputs(root);
  writeFileSync(join(root, 'LICENSE'), 'updated license');
  const licensed = inputs(root); assert.notEqual(licensed, initial);
  mkdirSync(join(root, '.cargo'));
  writeFileSync(join(root, '.cargo/config.toml'), '[build]\njobs = 2\n');
  const configured = inputs(root); assert.notEqual(configured, licensed);
  writeFileSync(join(root, '.cargo/config.toml'), '[build]\njobs = 1\n');
  const reconfigured = inputs(root); assert.notEqual(reconfigured, configured);
  mkdirSync(join(root, 'crates'));
  writeFileSync(join(root, 'crates/Δ.rs'), 'pub const VALUE: u32 = 1;\n');
  const unicode = inputs(root); assert.notEqual(unicode, reconfigured);
  writeFileSync(join(root, 'crates/Δ.rs'), 'pub const VALUE: u32 = 2;\n');
  assert.notEqual(inputs(root), unicode);
});
