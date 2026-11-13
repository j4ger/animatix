import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, rmSync, cpSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { buildLicenseArtifacts } from './third-party-licenses.mjs';

test('license inventory follows normal runtime dependencies and verifies pinned source fallbacks', () => {
  const root = mkdtempSync(join(tmpdir(), 'animatix-license-inventory-'));
  try {
    const pkg = (name, version, relative) => {
      const dir = join(root, relative);
      mkdirSync(dir, { recursive: true });
      const manifest = join(dir, 'Cargo.toml');
      writeFileSync(manifest, `[package]\nname = "${name}"\nversion = "${version}"\n`);
      return { name, version, id: `${name}@${version}`, manifest_path: manifest, license: 'MIT OR Apache-2.0', license_file: null, source: 'registry+https://github.com/rust-lang/crates.io-index' };
    };
    const rootPkg = pkg('animatix-web', '0.1.0', 'crates/animatix-web');
    const direct = pkg('has-license', '1.0.0', 'registry/has-license-1.0.0');
    writeFileSync(join(root, 'registry/has-license-1.0.0/LICENSE-MIT'), 'MIT license text\n');
    const split = pkg('wasm-splitter', '0.8.0-alpha.3', 'registry/wasm-splitter-0.8.0-alpha.3');
    split.repository = 'https://github.com/DioxusLabs/dioxus/';
    writeFileSync(join(root, 'registry/wasm-splitter-0.8.0-alpha.3/.cargo_vcs_info.json'), JSON.stringify({
      git: { sha1: 'cb27832398e8a3fa30a0fc8d6917c6d8a86fa71a' },
      path_in_vcs: 'packages/wasm-split/wasm-split',
    }));
    const fallback = join(root, 'web/tools/licenses/texts');
    mkdirSync(fallback, { recursive: true });
    const mit = 'MIT fallback text\n';
    const apache = 'Apache fallback text\n';
    writeFileSync(join(fallback, 'LICENSE-MIT'), mit);
    writeFileSync(join(fallback, 'LICENSE-APACHE'), apache);
    const sha256 = (text) => createHash('sha256').update(text).digest('hex');
    const revision = 'cb27832398e8a3fa30a0fc8d6917c6d8a86fa71a';
    mkdirSync(join(root, 'web/tools/licenses'), { recursive: true });
    writeFileSync(join(root, 'web/tools/licenses/fallbacks.json'), JSON.stringify({ schema_version: 1, fallbacks: [{
      name: 'wasm-splitter', version: '0.8.0-alpha.3', repository: 'https://github.com/DioxusLabs/dioxus', revision,
      path_in_vcs: 'packages/wasm-split/wasm-split', files: [
        { path: 'texts/LICENSE-MIT', name: 'LICENSE-MIT', source_url: `https://raw.githubusercontent.com/DioxusLabs/dioxus/${revision}/LICENSE-MIT`, sha256: sha256(mit) },
        { path: 'texts/LICENSE-APACHE', name: 'LICENSE-APACHE', source_url: `https://raw.githubusercontent.com/DioxusLabs/dioxus/${revision}/LICENSE-APACHE`, sha256: sha256(apache) },
      ],
    }] }));
    const missing = pkg('missing-license', '2.0.0', 'registry/missing-license-2.0.0');
    missing.license_file = '../../../external-license.txt';
    writeFileSync(join(root, 'external-license.txt'), 'External symlink text must not ship\n');
    symlinkSync(join(root, 'external-license.txt'), join(root, 'registry/missing-license-2.0.0/LICENSE'));
    const dev = pkg('dev-only', '3.0.0', 'registry/dev-only-3.0.0');
    writeFileSync(join(root, 'registry/dev-only-3.0.0/LICENSE'), 'Must not appear\n');
    const build = pkg('build-only', '4.0.0', 'registry/build-only-4.0.0');
    writeFileSync(join(root, 'registry/build-only-4.0.0/LICENSE'), 'Build dependency text\n');
    const metadata = {
      packages: [rootPkg, direct, split, missing, dev],
      workspace_members: [rootPkg.id],
      resolve: { nodes: [
        { id: rootPkg.id, deps: [
          { pkg: direct.id, dep_kinds: [{ kind: null, target: null }] },
          { pkg: split.id, dep_kinds: [{ kind: null, target: 'cfg(target_arch = "wasm32")' }] },
          { pkg: missing.id, dep_kinds: [{ kind: null, target: null }] },
          { pkg: dev.id, dep_kinds: [{ kind: 'dev', target: null }] },
          { pkg: build.id, dep_kinds: [{ kind: 'build', target: null }] },
        ] },
        { id: direct.id, deps: [] }, { id: split.id, deps: [] },
        { id: missing.id, deps: [] }, { id: dev.id, deps: [] }, { id: build.id, deps: [] },
      ] },
    };
    const options = {
      root,
      rootManifestPath: rootPkg.manifest_path,
      environment: { SOURCE_DATE_EPOCH: '123', RUSTC: process.env.RUSTC ?? 'rustc' },
    };
    const first = buildLicenseArtifacts(metadata, options);
    const second = buildLicenseArtifacts(metadata, options);
    assert.equal(first.notices, second.notices, 'notice output is deterministic');
    assert.deepEqual(first.spdx, second.spdx, 'SPDX output is deterministic');
    assert.deepEqual(first.inventory.packages.map(({ name }) => name).sort(), ['has-license', 'missing-license', 'rust-std', 'wasm-splitter']);
    assert.equal(first.inventory.missingLicenseText.some(({ name }) => name === 'missing-license'), true);
    assert.equal(first.notices.includes('External symlink text must not ship'), false);
    assert.equal(first.inventory.missingLicenseText.some(({ name }) => name === 'dev-only'), false);
    const splitRecord = first.inventory.packages.find(({ name }) => name === 'wasm-splitter');
    assert.equal(splitRecord.licenseFiles.length, 2);
    assert.match(splitRecord.licenseFiles[0].source, /cb27832398e8a3fa30a0fc8d6917c6d8a86fa71a/);
    assert.match(first.notices, /missing-license 2\.0\.0/);
    assert.equal(first.spdx.packages.length, 4);
    const missingSpdx = first.spdx.packages.find(({ name }) => name === 'missing-license');
    assert.equal(missingSpdx.licenseDeclared, 'MIT OR Apache-2.0');
    assert.equal(first.notices.includes(root), false, 'generated text does not expose checkout paths');
    writeFileSync(join(fallback, 'LICENSE-MIT'), 'tampered license text\n');
    assert.throws(() => buildLicenseArtifacts(metadata, options), /License fallback hash mismatch for wasm-splitter/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function spdxReferenceFixture() {
  const root = mkdtempSync(join(tmpdir(), 'animatix-spdx-reference-'));
  const packages = [
    { name: 'fax', version: '0.2.6', license: 'MIT', authors: ['Sebastian K <s3bk@protonmail.com>'], repository: 'https://github.com/pdf-rs/fax', revision: '40b226eb97b391355e971fd95a8dd532a4c127a0', path: '', checksum: 'f05de7d48f37cd6730705cbca900770cab77a89f413d23e100ad7fad7795a0ab' },
    { name: 'fax_derive', version: '0.2.0', license: 'MIT', authors: ['Sebastian K <s3bk@protonmail.com>'], repository: 'https://github.com/pdf-rs/fax', revision: '40b226eb97b391355e971fd95a8dd532a4c127a0', path: 'derive', checksum: 'a0aca10fb742cb43f9e7bb8467c91aa9bcb8e3ffbc6a6f7389bb93ffc920577d' },
    { name: 'glidesort', version: '0.1.2', license: 'MIT OR Apache-2.0', authors: ['Orson Peters <orsonpeters@gmail.com>'], repository: 'https://github.com/orlp/glidesort', revision: 'c4df9518ec23a424a867b3d50967f0aaafe3c0fd', path: '', checksum: 'f2e102e6eb644d3e0b186fc161e4460417880a0a0b87d235f2e5b8fb30f2e9e0' },
  ];
  const rootPkgDir = join(root, 'crates/animatix-web');
  mkdirSync(rootPkgDir, { recursive: true });
  const rootPkg = { name: 'animatix-web', version: '0.1.0', id: 'animatix-web 0.1.0 (path+file:///animatix-web)', manifest_path: join(rootPkgDir, 'Cargo.toml'), source: null, license: null };
  writeFileSync(rootPkg.manifest_path, '[package]\nname = "animatix-web"\nversion = "0.1.0"\n');
  writeFileSync(join(root, 'Cargo.lock'), packages.map((item) => [
    '[[package]]', `name = "${item.name}"`, `version = "${item.version}"`,
    'source = "registry+https://github.com/rust-lang/crates.io-index"', `checksum = "${item.checksum}"`,
  ].join('\n')).join('\n\n'));
  cpSync(join(new URL('./licenses/spdx-reference/', import.meta.url).pathname), join(root, 'web/tools/licenses/spdx-reference'), { recursive: true });
  const metadataPackages = [rootPkg];
  const deps = [];
  for (const item of packages) {
    const directory = join(root, `registry/${item.name}-${item.version}`);
    mkdirSync(directory, { recursive: true });
    const pkg = {
      name: item.name,
      version: item.version,
      id: `${item.name} ${item.version} (registry+https://github.com/rust-lang/crates.io-index)`,
      manifest_path: join(directory, 'Cargo.toml'),
      source: 'registry+https://github.com/rust-lang/crates.io-index',
      license: item.license,
      authors: item.authors,
      repository: item.repository,
    };
    writeFileSync(pkg.manifest_path, `[package]\nname = "${item.name}"\nversion = "${item.version}"\nlicense = "${item.license}"\n`);
    writeFileSync(join(directory, '.cargo-checksum.json'), JSON.stringify({ package: item.checksum, files: {} }));
    writeFileSync(join(directory, '.cargo_vcs_info.json'), JSON.stringify({ git: { sha1: item.revision }, path_in_vcs: item.path }));
    metadataPackages.push(pkg);
    deps.push({ pkg: pkg.id, dep_kinds: [{ kind: null, target: null }] });
  }
  return {
    root,
    metadata: {
      packages: metadataPackages,
      workspace_members: [rootPkg.id],
      resolve: { nodes: [{ id: rootPkg.id, deps }, ...metadataPackages.slice(1).map((pkg) => ({ id: pkg.id, deps: [] }))] },
    },
    options: { root, rootManifestPath: rootPkg.manifest_path, environment: { RUSTC: '/missing-rustc-for-fixture', SOURCE_DATE_EPOCH: '123' } },
    packages,
  };
}

test('SPDX standard references are additive and never clear missing upstream license text', () => {
  const fixture = spdxReferenceFixture();
  try {
    const result = buildLicenseArtifacts(fixture.metadata, fixture.options);
    for (const item of fixture.packages) {
      const record = result.inventory.packages.find(({ name }) => name === item.name);
      assert.ok(record, `package ${item.name} is included`);
      assert.equal(record.licenseFiles.length, 0, 'SPDX reference must not be presented as upstream license text');
      assert.equal(record.missingLicenseText, true, 'SPDX reference must not clear the missing upstream text status');
      assert.deepEqual(record.authors, item.authors);
      assert.equal(record.repository, item.repository);
      assert.equal(record.registryChecksum, item.checksum);
      assert.equal(record.vcsRevision, item.revision);
      assert.equal(record.vcsPath, item.path);
      assert.ok(record.referenceLicenseFiles.length);
      assert.ok(record.referenceLicenseFiles.every(({ notUpstreamCopy }) => notUpstreamCopy === true));
    }
    const glide = result.inventory.packages.find(({ name }) => name === 'glidesort');
    assert.deepEqual(glide.referenceLicenseFiles.map(({ spdxId }) => spdxId).sort(), ['Apache-2.0', 'MIT']);
    assert.match(result.notices, /SPDX standard-reference texts are included separately/);
    assert.match(result.notices, /not upstream copy/);
    assert.match(result.notices, /Copyright \(c\) <year> <copyright holders>/);
    assert.match(result.notices, /Copyright \[yyyy\] \[name of copyright owner\]/);
    for (const item of fixture.packages) {
      const spdx = result.spdx.packages.find(({ name }) => name === item.name);
      assert.equal(spdx.licenseDeclared, item.license);
      assert.deepEqual(spdx.licenseInfoInFiles, ['NOASSERTION']);
      assert.equal(spdx.copyrightText, 'NOASSERTION');
    }
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test('SPDX references fail closed on package identity, reference id, and text hash mismatch', () => {
  const fixture = spdxReferenceFixture();
  try {
    const wrongPackage = fixture.metadata.packages.find(({ name }) => name === 'glidesort');
    wrongPackage.license = 'MIT';
    let result = buildLicenseArtifacts(fixture.metadata, fixture.options);
    let glide = result.inventory.packages.find(({ name }) => name === 'glidesort');
    assert.deepEqual(glide.referenceLicenseFiles, [], 'wrong package expression must not receive standard text');
    assert.equal(glide.missingLicenseText, true);
    wrongPackage.license = 'MIT OR Apache-2.0';

    const lockfilePath = join(fixture.root, 'Cargo.lock');
    const originalLockfile = readFileSync(lockfilePath, 'utf8');
    writeFileSync(lockfilePath, originalLockfile.replace('f2e102e6eb644d3e0b186fc161e4460417880a0a0b87d235f2e5b8fb30f2e9e0', '0'.repeat(64)));
    result = buildLicenseArtifacts(fixture.metadata, fixture.options);
    glide = result.inventory.packages.find(({ name }) => name === 'glidesort');
    assert.deepEqual(glide.referenceLicenseFiles, [], 'wrong registry checksum must not receive standard text');
    assert.equal(glide.missingLicenseText, true);
    writeFileSync(lockfilePath, originalLockfile);

    const manifestPath = join(fixture.root, 'web/tools/licenses/spdx-reference/manifest.json');
    const originalManifest = readFileSync(manifestPath, 'utf8');
    const changedId = JSON.parse(originalManifest);
    changedId.licenses[0].spdx_id = 'MIT-FORGED';
    writeFileSync(manifestPath, JSON.stringify(changedId));
    assert.throws(() => buildLicenseArtifacts(fixture.metadata, fixture.options), /Invalid or unpinned SPDX reference entry/);

    writeFileSync(manifestPath, originalManifest);
    writeFileSync(join(fixture.root, 'web/tools/licenses/spdx-reference/texts/MIT.txt'), 'tampered SPDX license reference\n');
    assert.throws(() => buildLicenseArtifacts(fixture.metadata, fixture.options), /SPDX reference text hash mismatch/);
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});
