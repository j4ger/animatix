import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const LICENSE_NAME = /^((?:un)?license|licence|copying|notice)(?:[._ -].*)?$/i;
const SPDX_REFERENCE_RELEASE = {
  name: 'SPDX License List data',
  repository: 'https://github.com/spdx/license-list-data',
  tag: 'v3.29.0',
  revision: '31ba1a50e5397e00a304dbadc76531740e89ee48',
};
const SPDX_REFERENCE_LICENSES = {
  MIT: {
    path: 'texts/MIT.txt',
    source_url: `https://raw.githubusercontent.com/spdx/license-list-data/${SPDX_REFERENCE_RELEASE.revision}/text/MIT.txt`,
    sha256: 'b05785f9f18e6716bab63424b11454513b9943a222595b70411009202fc592b5',
  },
  'Apache-2.0': {
    path: 'texts/Apache-2.0.txt',
    source_url: `https://raw.githubusercontent.com/spdx/license-list-data/${SPDX_REFERENCE_RELEASE.revision}/text/Apache-2.0.txt`,
    sha256: '074e6e32c86a4c0ef8b3ed25b721ca23aca83df277cd88106ef7177c354615ff',
  },
};
const SPDX_REFERENCE_PACKAGES = [
  {
    name: 'fax', version: '0.2.6', license_expression: 'MIT',
    repository: 'https://github.com/pdf-rs/fax', revision: '40b226eb97b391355e971fd95a8dd532a4c127a0',
    path_in_vcs: '', registry_checksum: 'f05de7d48f37cd6730705cbca900770cab77a89f413d23e100ad7fad7795a0ab',
    authors: ['Sebastian K <s3bk@protonmail.com>'], spdx_ids: ['MIT'],
  },
  {
    name: 'fax_derive', version: '0.2.0', license_expression: 'MIT',
    repository: 'https://github.com/pdf-rs/fax', revision: '40b226eb97b391355e971fd95a8dd532a4c127a0',
    path_in_vcs: 'derive', registry_checksum: 'a0aca10fb742cb43f9e7bb8467c91aa9bcb8e3ffbc6a6f7389bb93ffc920577d',
    authors: ['Sebastian K <s3bk@protonmail.com>'], spdx_ids: ['MIT'],
  },
  {
    name: 'glidesort', version: '0.1.2', license_expression: 'MIT OR Apache-2.0',
    repository: 'https://github.com/orlp/glidesort', revision: 'c4df9518ec23a424a867b3d50967f0aaafe3c0fd',
    path_in_vcs: '', registry_checksum: 'f2e102e6eb644d3e0b186fc161e4460417880a0a0b87d235f2e5b8fb30f2e9e0',
    authors: ['Orson Peters <orsonpeters@gmail.com>'], spdx_ids: ['MIT', 'Apache-2.0'],
  },
];

const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const matchesPinnedRecord = (value, pinned) => value && Object.keys(value).length === Object.keys(pinned).length
  && Object.entries(pinned).every(([key, expected]) => JSON.stringify(value[key]) === JSON.stringify(expected));
const run = (command, args, cwd = ROOT) => execFileSync(command, args, {
  cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
}).trim();
const sorted = (values) => [...values].sort((a, b) => a.localeCompare(b, 'en'));
const isRegularFile = (file) => {
  try { return lstatSync(file).isFile(); } catch { return false; }
};

function packageDirectory(pkg) {
  return dirname(pkg.manifest_path);
}

function isWithin(directory, file) {
  try {
    const base = realpathSync(directory);
    const target = realpathSync(file);
    return target === base || target.startsWith(`${base}/`);
  } catch {
    return false;
  }
}

function licenseCandidates(pkg) {
  const dir = packageDirectory(pkg);
  const files = [];
  if (pkg.license_file) {
    const explicit = resolve(dir, pkg.license_file);
    if (isRegularFile(explicit) && isWithin(dir, explicit)) files.push(explicit);
  }
  for (const name of sorted(readdirSync(dir))) {
    const file = join(dir, name);
    if (LICENSE_NAME.test(name) && isRegularFile(file) && !files.includes(file)) files.push(file);
  }
  return files;
}

function normalizedRepository(value) {
  if (!value) return null;
  try {
    const url = new URL(value.replace(/\/tree\/[^/]+(?:\/.*)?$/, '').replace(/\/$/, ''));
    if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash) return null;
    return `${url.protocol}//${url.host}${url.pathname.replace(/\/$/, '')}`;
  } catch {
    return null;
  }
}

function licenseFallbacks(root) {
  const manifestPath = join(root, 'web/tools/licenses/fallbacks.json');
  if (!isRegularFile(manifestPath)) return [];
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (manifest.schema_version !== 1 || !Array.isArray(manifest.fallbacks)) throw new Error('Unsupported license fallback manifest');
  return manifest.fallbacks;
}

function verifiedLicenseFallback(pkg, root) {
  const dir = packageDirectory(pkg);
  const vcsPath = join(dir, '.cargo_vcs_info.json');
  if (!isRegularFile(vcsPath)) return [];
  const vcs = JSON.parse(readFileSync(vcsPath, 'utf8'));
  const candidate = licenseFallbacks(root).find((entry) => entry.name === pkg.name && entry.version === pkg.version);
  if (!candidate || candidate.repository !== normalizedRepository(pkg.repository)
      || candidate.revision !== vcs.git?.sha1
      || candidate.path_in_vcs !== vcs.path_in_vcs) return [];
  return candidate.files.map((entry) => {
    const source = new URL(entry.source_url);
    const repo = new URL(candidate.repository);
    const expectedPrefix = `/${repo.pathname.split('/').filter(Boolean).join('/')}/${candidate.revision}/`;
    if (source.protocol !== 'https:' || source.hostname !== 'raw.githubusercontent.com'
        || !source.pathname.startsWith(expectedPrefix)
        || !entry.path || entry.path.startsWith('/') || entry.path.split(/[\\/]/).includes('..')) return null;
    const file = join(root, 'web/tools/licenses', entry.path);
    if (!isRegularFile(file) || !isWithin(join(root, 'web/tools/licenses'), file)) return null;
    const bytes = readFileSync(file);
    if (digest(bytes) !== entry.sha256) throw new Error(`License fallback hash mismatch for ${pkg.name}: ${entry.path}`);
    return { file, source: `verified upstream ${candidate.repository}@${candidate.revision} (${entry.source_url})` };
  }).filter(Boolean);
}

function spdxReferenceConfig(root) {
  const directory = join(root, 'web/tools/licenses/spdx-reference');
  const manifestPath = join(directory, 'manifest.json');
  if (!isRegularFile(manifestPath)) return { packageRecords: [], texts: new Map() };
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (manifest.schema_version !== 1 || !matchesPinnedRecord(manifest.source, SPDX_REFERENCE_RELEASE)) {
    throw new Error('Unsupported or unpinned SPDX reference manifest');
  }
  if (!Array.isArray(manifest.licenses) || manifest.licenses.length !== Object.keys(SPDX_REFERENCE_LICENSES).length) {
    throw new Error('SPDX reference manifest has an unexpected license set');
  }
  const texts = new Map();
  for (const [spdxId, pinned] of Object.entries(SPDX_REFERENCE_LICENSES)) {
    const entry = manifest.licenses.find((license) => license.spdx_id === spdxId);
    if (!entry || entry.path !== pinned.path || entry.source_url !== pinned.source_url || entry.sha256 !== pinned.sha256) {
      throw new Error(`Invalid or unpinned SPDX reference entry: ${spdxId}`);
    }
    const file = join(directory, entry.path);
    if (!isRegularFile(file) || !isWithin(directory, file)) throw new Error(`Missing or escaping SPDX reference text: ${entry.path}`);
    const bytes = readFileSync(file);
    if (digest(bytes) !== pinned.sha256) throw new Error(`SPDX reference text hash mismatch: ${spdxId}`);
    texts.set(spdxId, {
      spdxId,
      name: `${spdxId}.txt`,
      path: entry.path,
      sha256: pinned.sha256,
      sourceUrl: pinned.source_url,
      source: `SPDX License List ${manifest.source.tag} reference; not an upstream package file`,
      text: bytes.toString('utf8'),
    });
  }
  if (!Array.isArray(manifest.packages) || manifest.packages.length !== SPDX_REFERENCE_PACKAGES.length) {
    throw new Error('SPDX reference manifest has an unexpected package set');
  }
  for (const pinned of SPDX_REFERENCE_PACKAGES) {
    const entry = manifest.packages.find(({ name, version }) => name === pinned.name && version === pinned.version);
    if (!matchesPinnedRecord(entry, pinned)) {
      throw new Error(`Invalid or unpinned SPDX package reference: ${pinned.name}@${pinned.version}`);
    }
  }
  return { packageRecords: manifest.packages, texts };
}

function cargoRegistryChecksum(lockfile, name, version, source) {
  const entry = lockfile.split(/\n(?=\[\[package\]\]\n)/).find((block) => {
    const field = (key) => block.match(new RegExp(`^${key} = "([^"]*)"$`, 'm'))?.[1];
    return field('name') === name && field('version') === version && field('source') === source;
  });
  return entry?.match(/^checksum = "([0-9a-f]{64})"$/m)?.[1] ?? null;
}

function verifiedSpdxReferences(pkg, referenceConfig, root) {
  const pinned = referenceConfig.packageRecords.find(({ name, version }) => name === pkg.name && version === pkg.version);
  if (!pinned) return [];
  if (pkg.source !== 'registry+https://github.com/rust-lang/crates.io-index'
      || pkg.license !== pinned.license_expression
      || normalizedRepository(pkg.repository) !== pinned.repository
      || JSON.stringify(pkg.authors ?? []) !== JSON.stringify(pinned.authors)) return [];
  const directory = packageDirectory(pkg);
  const vcsPath = join(directory, '.cargo_vcs_info.json');
  if (!isRegularFile(vcsPath) || !isWithin(directory, vcsPath)) return [];
  let lockfile;
  let vcs;
  try {
    lockfile = readFileSync(join(root, 'Cargo.lock'), 'utf8');
    vcs = JSON.parse(readFileSync(vcsPath, 'utf8'));
  } catch {
    return [];
  }
  const checksum = cargoRegistryChecksum(lockfile, pkg.name, pkg.version, pkg.source);
  if (checksum !== pinned.registry_checksum || vcs.git?.sha1 !== pinned.revision
      || vcs.path_in_vcs !== pinned.path_in_vcs) return [];
  return pinned.spdx_ids.map((spdxId) => {
    const reference = referenceConfig.texts.get(spdxId);
    if (!reference) throw new Error(`Unknown SPDX reference identifier: ${spdxId}`);
    return {
      name: reference.name,
      spdxId,
      sourceType: 'spdx-standard-reference',
      sha256: reference.sha256,
      source: reference.source,
      sourceUrl: reference.sourceUrl,
      release: SPDX_REFERENCE_RELEASE.tag,
      notUpstreamCopy: true,
    };
  });
}

function runtimePackages(metadata, rootManifestPath) {
  if (!metadata.resolve?.nodes) throw new Error('cargo metadata has no resolved dependency graph');
  const rootManifest = resolve(rootManifestPath);
  const root = metadata.packages.find((pkg) => resolve(pkg.manifest_path) === rootManifest);
  if (!root) throw new Error(`cargo metadata did not include ${rootManifest}`);
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const workspace = new Set(metadata.workspace_members ?? []);
  const reachable = new Set();
  const visit = (id) => {
    if (reachable.has(id)) return;
    reachable.add(id);
    for (const dependency of nodes.get(id)?.deps ?? []) {
      if (dependency.dep_kinds?.some(({ kind }) => kind === null)) visit(dependency.pkg);
    }
  };
  visit(root.id);
  return [...reachable]
    .filter((id) => !workspace.has(id))
    .map((id) => packages.get(id))
    .filter(Boolean)
    .sort((a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en') || a.id.localeCompare(b.id, 'en'));
}

function rustStdPackage(root, environment) {
  let sysroot;
  let version;
  try {
    sysroot = run(environment.RUSTC || 'rustc', ['--print', 'sysroot'], root);
    version = run(environment.RUSTC || 'rustc', ['-V'], root).match(/^rustc\s+([^\s]+)/)?.[1] ?? 'unknown';
  } catch (error) {
    return { name: 'rust-std', version: 'unknown', license: 'MIT OR Apache-2.0', files: [], missing: true, source: 'Rust toolchain (rustc --print sysroot unavailable)' };
  }
  const search = [sysroot, join(sysroot, 'share/doc/rust'), join(sysroot, 'lib/rustlib')];
  const files = [];
  for (const dir of search) {
    if (!existsSync(dir)) continue;
    for (const name of sorted(readdirSync(dir))) {
      const file = join(dir, name);
      if (LICENSE_NAME.test(name) && isRegularFile(file) && !files.includes(file)) files.push(file);
    }
  }
  return { name: 'rust-std', version, license: 'MIT OR Apache-2.0', files, missing: files.length === 0, source: 'rustc sysroot' };
}

function spdxId(value) {
  return `SPDXRef-Package-${createHash('sha256').update(value).digest('hex').slice(0, 16)}`;
}

function cargoLicenseExpression(value) {
  if (!value) return 'NOASSERTION';
  // Check SPDX expression structure without guessing what an unknown license identifier means.
  const tokens = value.match(/\(|\)|AND|OR|WITH|[A-Za-z0-9][A-Za-z0-9.+-]*/g) ?? [];
  if (tokens.join(' ') !== value.trim().replace(/\s+/g, ' ')) return 'NOASSERTION';
  let depth = 0;
  let expectsTerm = true;
  for (const token of tokens) {
    if (token === '(') {
      if (!expectsTerm) return 'NOASSERTION';
      depth += 1;
    } else if (token === ')') {
      if (expectsTerm || depth === 0) return 'NOASSERTION';
      depth -= 1;
      expectsTerm = false;
    } else if (['AND', 'OR', 'WITH'].includes(token)) {
      if (expectsTerm) return 'NOASSERTION';
      expectsTerm = true;
    } else {
      if (!expectsTerm) return 'NOASSERTION';
      expectsTerm = false;
    }
  }
  return tokens.length && !expectsTerm && depth === 0 ? value.trim() : 'NOASSERTION';
}

export function buildLicenseArtifacts(metadata, {
  root = ROOT,
  rootManifestPath = join(root, 'crates/animatix-web/Cargo.toml'),
  environment = process.env,
} = {}) {
  const packages = runtimePackages(metadata, rootManifestPath);
  const fallbacks = licenseFallbacks(root);
  const spdxReferences = spdxReferenceConfig(root);
  const licenseTexts = new Map();
  const referenceTexts = new Map();
  const inventory = packages.map((pkg) => {
    let files = licenseCandidates(pkg).map((file) => ({ file, source: 'package source' }));
    let provenance = 'package source';
    if (!files.length) {
      files = verifiedLicenseFallback(pkg, root);
      if (files.length) provenance = files[0].source;
    }
    const licenseFiles = files.map(({ file, source }) => {
      const bytes = readFileSync(file);
      const hash = digest(bytes);
      if (!licenseTexts.has(hash)) {
        licenseTexts.set(hash, { id: `LicenseText-${hash.slice(0, 16)}`, hash, name: file.split(/[\\/]/).at(-1), text: bytes.toString('utf8') });
      }
      return { name: file.split(/[\\/]/).at(-1), sha256: hash, source };
    });
    const referenceLicenseFiles = verifiedSpdxReferences(pkg, spdxReferences, root);
    for (const file of referenceLicenseFiles) {
      const text = spdxReferences.texts.get(file.spdxId);
      if (!referenceTexts.has(file.sha256)) referenceTexts.set(file.sha256, text);
    }
    const referencePackage = referenceLicenseFiles.length
      ? spdxReferences.packageRecords.find(({ name, version }) => name === pkg.name && version === pkg.version)
      : null;
    return {
      // Cargo path package IDs can contain the absolute checkout path; publish only a stable digest.
      id: `CargoPackage-${createHash('sha256').update(pkg.id).digest('hex').slice(0, 16)}`,
      name: pkg.name,
      version: pkg.version,
      authors: pkg.authors ?? [],
      license: pkg.license ?? null,
      source: pkg.source ?? 'local path dependency',
      repository: pkg.repository ?? null,
      licenseFiles,
      referenceLicenseFiles,
      missingLicenseText: licenseFiles.length === 0,
      ...(referencePackage ? {
        registryChecksum: referencePackage.registry_checksum,
        vcsRevision: referencePackage.revision,
        vcsPath: referencePackage.path_in_vcs,
      } : {}),
    };
  });
  const rustStd = rustStdPackage(root, environment);
  let rustFileSources = rustStd.files.map((file) => ({ file, source: 'Rust sysroot' }));
  if (!rustFileSources.length) {
    const rustFallback = fallbacks.find((entry) => entry.name === 'rust-std' && entry.version === rustStd.version);
    let compilerCommit = '';
    try { compilerCommit = run(environment.RUSTC || 'rustc', ['-Vv'], root).match(/commit-hash: ([0-9a-f]+)/)?.[1] ?? ''; } catch {}
    if (rustFallback && compilerCommit === rustFallback.revision) {
      rustFileSources = rustFallback.files.map((entry) => {
        const source = new URL(entry.source_url);
        const repo = new URL(rustFallback.repository);
        const expectedPrefix = `/${repo.pathname.split('/').filter(Boolean).join('/')}/${rustFallback.revision}/`;
        if (source.protocol !== 'https:' || source.hostname !== 'raw.githubusercontent.com'
            || !source.pathname.startsWith(expectedPrefix)
            || !entry.path || entry.path.startsWith('/') || entry.path.split(/[\\/]/).includes('..')) return null;
        const file = join(root, 'web/tools/licenses', entry.path);
        if (!isRegularFile(file) || !isWithin(join(root, 'web/tools/licenses'), file)) return null;
        const bytes = readFileSync(file);
        if (digest(bytes) !== entry.sha256) throw new Error(`Rust license fallback hash mismatch: ${entry.path}`);
        return { file, source: `verified Rust source ${rustFallback.repository}@${rustFallback.revision} (${entry.source_url})` };
      }).filter(Boolean);
      if (rustFileSources.length) rustStd.source = `Rust source ${rustFallback.repository}@${rustFallback.revision}`;
    }
  }
  const rustFiles = rustFileSources.map(({ file, source }) => {
    const bytes = readFileSync(file);
    const hash = digest(bytes);
    if (!licenseTexts.has(hash)) licenseTexts.set(hash, { id: `LicenseText-${hash.slice(0, 16)}`, hash, name: file.split(/[\\/]/).at(-1), text: bytes.toString('utf8') });
    return { name: file.split(/[\\/]/).at(-1), sha256: hash, source };
  });
  inventory.push({
    id: 'rust-std', name: 'rust-std', version: rustStd.version, license: rustStd.license,
    authors: [], repository: null, source: rustStd.source, licenseFiles: rustFiles,
    referenceLicenseFiles: [], missingLicenseText: rustFiles.length === 0,
  });
  inventory.sort((a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en') || String(a.id).localeCompare(String(b.id), 'en'));

  const missing = inventory.filter((pkg) => pkg.missingLicenseText).map(({ name, version, license, source }) => ({ name, version, license, source }));
  const notices = [
    'Animatix Web SDK — Rust runtime dependency notices',
    'Generated from Cargo metadata filtered for wasm32-unknown-unknown and normal dependencies of animatix-web.',
    'License expressions and files are copied from package metadata and source trees. Missing source files are listed explicitly; this inventory is not a legal compliance certification.',
    'SPDX standard-reference texts are included separately for exact declared SPDX identifiers. They are not upstream package copies and do not change the missing upstream-text status; unknown copyright years or holders remain unasserted.',
    '',
    'Runtime package inventory',
    ...inventory.map((pkg) => {
      const lines = [`- ${pkg.name} ${pkg.version} — Cargo license: ${pkg.license ?? 'unspecified'}; source: ${pkg.source}; authors: ${pkg.authors.join(', ') || 'unspecified'}; repository: ${pkg.repository ?? 'unspecified'}`];
      if (pkg.licenseFiles.length) lines.push(`  License/notice text: ${pkg.licenseFiles.map((file) => `${file.name} (SHA-256 ${file.sha256}, ${file.source})`).join('; ')}`);
      else lines.push('  License/notice text: MISSING in available package/sysroot files.');
      if (pkg.referenceLicenseFiles.length) {
        lines.push(`  SPDX reference text (not upstream copy): ${pkg.referenceLicenseFiles.map((file) => `${file.spdxId} ${file.name} (SHA-256 ${file.sha256}; ${file.source}; URL ${file.sourceUrl})`).join('; ')}`);
        lines.push(`  Registry checksum: ${pkg.registryChecksum}; upstream VCS revision/path: ${pkg.vcsRevision}/${pkg.vcsPath || '(repository root)'}`);
      }
      return lines.join('\n');
    }),
    '',
    'Missing license or notice text',
    ...(missing.length ? missing.map((pkg) => `- ${pkg.name} ${pkg.version} — ${pkg.license ?? 'license expression unspecified'}; source: ${pkg.source}`) : ['- none']),
    '',
    'License and notice texts (deduplicated by SHA-256)',
    ...[...licenseTexts.values()].sort((a, b) => a.hash.localeCompare(b.hash, 'en'))
      .flatMap((license) => [`===== ${license.name} — SHA-256 ${license.hash} =====`, license.text.trimEnd(), '']),
    'SPDX standard reference texts (not upstream package files)',
    ...[...referenceTexts.values()].sort((a, b) => a.spdxId.localeCompare(b.spdxId, 'en'))
      .flatMap((license) => [`===== ${license.name} — ${license.spdxId} — SHA-256 ${license.sha256} — ${license.sourceUrl} =====`, license.text.trimEnd(), '']),
  ].join('\n') + '\n';

  const epoch = Number(environment.SOURCE_DATE_EPOCH ?? 0);
  const created = new Date(epoch * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
  const documentId = 'SPDXRef-DOCUMENT';
  const spdxPackages = inventory.map((pkg) => {
    const id = spdxId(`${pkg.name}@${pkg.version}:${pkg.id}`);
    const comments = [
      `Cargo license expression: ${pkg.license ?? 'unspecified'}`,
      `Cargo source: ${pkg.source}`,
      pkg.licenseFiles.length ? `License/notice files: ${pkg.licenseFiles.map((file) => `${file.name} sha256:${file.sha256}`).join(', ')}` : 'No license/notice text file was found in the available source tree.',
      pkg.referenceLicenseFiles.length ? `SPDX standard reference texts: ${pkg.referenceLicenseFiles.map((file) => `${file.spdxId} sha256:${file.sha256} (not an upstream package file)`).join(', ')}` : '',
    ].join('; ');
    return {
      SPDXID: id,
      name: pkg.name,
      versionInfo: pkg.version,
      downloadLocation: pkg.source.startsWith('registry+')
        ? `https://crates.io/crates/${pkg.name}/${pkg.version}`
        : 'NOASSERTION',
      filesAnalyzed: false,
      licenseConcluded: 'NOASSERTION',
      licenseDeclared: cargoLicenseExpression(pkg.license),
      licenseInfoInFiles: ['NOASSERTION'],
      licenseComments: comments,
      copyrightText: 'NOASSERTION',
      externalRefs: [{ referenceCategory: 'PACKAGE-MANAGER', referenceType: 'purl', referenceLocator: `pkg:cargo/${encodeURIComponent(pkg.name)}@${encodeURIComponent(pkg.version)}` }],
    };
  });
  const spdx = {
    SPDXID: documentId,
    spdxVersion: 'SPDX-2.3',
    dataLicense: 'CC0-1.0',
    name: 'Animatix Web SDK wasm32 runtime dependency inventory',
    documentNamespace: `https://animatix.dev/spdx/web-sdk/${digest(Buffer.from(JSON.stringify(inventory)))}`,
    creationInfo: { creators: ['Tool: Animatix third-party license inventory'], created },
    packages: spdxPackages,
    relationships: spdxPackages.map((pkg) => ({ spdxElementId: documentId, relationshipType: 'DESCRIBES', relatedSpdxElement: pkg.SPDXID })),
  };
  return {
    notices,
    spdx,
    inventory: { target: 'wasm32-unknown-unknown', rootPackage: 'animatix-web', packages: inventory, missingLicenseText: missing },
  };
}

export function writeThirdPartyLicenses(directory, options = {}) {
  const root = options.root ?? ROOT;
  const metadata = options.metadata ?? JSON.parse(run('cargo', [
    'metadata', '--locked', '--format-version', '1', '--filter-platform', 'wasm32-unknown-unknown',
    '--manifest-path', join(root, 'crates/animatix-web/Cargo.toml'),
  ], root));
  const artifacts = buildLicenseArtifacts(metadata, { ...options, root });
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, 'THIRD-PARTY-NOTICES.txt'), artifacts.notices);
  writeFileSync(join(directory, 'THIRD-PARTY-SBOM.spdx.json'), `${JSON.stringify(artifacts.spdx, null, 2)}\n`);
  writeFileSync(join(directory, 'THIRD-PARTY-PACKAGES.json'), `${JSON.stringify(artifacts.inventory, null, 2)}\n`);
  for (const pkg of artifacts.inventory.missingLicenseText) {
    const referenceIds = pkg.referenceLicenseFiles?.map(({ spdxId }) => spdxId) ?? [];
    const detail = referenceIds.length
      ? `; separate SPDX reference text included (${referenceIds.join(', ')}), exact upstream text unverified`
      : '';
    process.stderr.write(`Missing exact upstream license text for wasm runtime package ${pkg.name}@${pkg.version} (${pkg.license ?? 'license expression unspecified'})${detail}.\n`);
  }
  return artifacts.inventory;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const output = process.argv[2];
  if (!output) throw new Error('usage: node third-party-licenses.mjs <sdk-directory>');
  const inventory = writeThirdPartyLicenses(resolve(output));
  const withReferences = inventory.packages.filter((pkg) => pkg.referenceLicenseFiles?.length).length;
  process.stdout.write(`Wrote license notices and package inventory for ${inventory.packages.length - 1} Cargo packages; ${inventory.missingLicenseText.length} packages lack exact upstream license text; SPDX reference texts are separately included for ${withReferences} packages.\n`);
}
