'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const {
  MAX_ARCHIVE_BYTES,
  MAX_MANIFEST_BYTES,
  expectedChecksum,
  install,
  nativeBinaryPath,
  targetFor,
  validateDownloadUrl,
} = require('../install.js');

const TARGETS = [
  ['darwin', 'arm64', 'aarch64-apple-darwin'],
  ['darwin', 'x64', 'x86_64-apple-darwin'],
  ['linux', 'x64', 'x86_64-unknown-linux-gnu'],
  ['linux', 'arm64', 'aarch64-unknown-linux-gnu'],
];

function fixtureArchive(root) {
  const payload = path.join(root, 'payload');
  fs.mkdirSync(payload);
  fs.writeFileSync(path.join(payload, 'estelle'), '#!/bin/sh\nprintf "native-estelle\\n"\n', { mode: 0o755 });
  const archive = path.join(root, 'estelle.tar.gz');
  const result = spawnSync('tar', ['-czf', archive, '-C', payload, 'estelle'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.existsSync(archive), true);
  return fs.readFileSync(archive);
}

test('the four release platforms map to the four published targets', () => {
  assert.equal(TARGETS.length, 4);
  for (const [platform, arch, target] of TARGETS) assert.equal(targetFor(platform, arch), target);
  assert.throws(() => targetFor('win32', 'x64'), /unsupported operating system/);
  assert.throws(() => targetFor('linux', 'ia32'), /unsupported architecture/);
});

test('release URLs remain HTTPS and GitHub-owned across redirects', () => {
  assert.equal(validateDownloadUrl('https://github.com/uqeu/estelle-cli').hostname, 'github.com');
  assert.equal(
    validateDownloadUrl('https://release-assets.githubusercontent.com/example').hostname,
    'release-assets.githubusercontent.com',
  );
  assert.throws(() => validateDownloadUrl('http://github.com/uqeu/estelle-cli'), /HTTPS/);
  assert.throws(() => validateDownloadUrl('https://attacker.example/estelle'), /left GitHub/);
});

test('the checksum parser requires one exact archive entry', () => {
  const hash = 'a'.repeat(64);
  assert.equal(expectedChecksum(Buffer.from(`${hash}  estelle-x.tar.gz\n`), 'estelle-x.tar.gz'), hash);
  assert.throws(
    () => expectedChecksum(Buffer.from(`${hash}  estelle-x.tar.gz\n${hash}  estelle-x.tar.gz\n`), 'estelle-x.tar.gz'),
    /exactly once/,
  );
  assert.throws(() => expectedChecksum(Buffer.from(`${hash} *estelle-x.tar.gz\n`), 'estelle-x.tar.gz'), /exactly once/);
});

test('install verifies the archive before atomically exposing the native binary', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-npm-test.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const archive = fixtureArchive(root);
  const target = targetFor('linux', 'x64');
  const archiveName = `estelle-${target}.tar.gz`;
  const hash = crypto.createHash('sha256').update(archive).digest('hex');
  const manifest = Buffer.from(`${hash}  ${archiveName}\n`);
  const requests = [];
  const download = async (url, limit) => {
    requests.push([url, limit]);
    return url.endsWith('SHA256SUMS') ? manifest : archive;
  };

  const binary = await install({ platform: 'linux', arch: 'x64', packageDir: root, download });
  assert.equal(binary, nativeBinaryPath(root, target));
  assert.equal(fs.statSync(binary).mode & 0o777, 0o755);
  assert.equal(spawnSync(binary, [], { encoding: 'utf8' }).stdout.trim(), 'native-estelle');
  assert.deepEqual(requests.map((request) => request[1]), [MAX_MANIFEST_BYTES, MAX_ARCHIVE_BYTES]);
});

test('a checksum mutant installs no executable', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-npm-mutant.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const archive = fixtureArchive(root);
  const target = targetFor('linux', 'x64');
  const archiveName = `estelle-${target}.tar.gz`;
  const manifest = Buffer.from(`${'0'.repeat(64)}  ${archiveName}\n`);
  const download = async (url) => (url.endsWith('SHA256SUMS') ? manifest : archive);

  await assert.rejects(install({ platform: 'linux', arch: 'x64', packageDir: root, download }), /checksum mismatch/);
  assert.equal(fs.existsSync(nativeBinaryPath(root, target)), false);
});
