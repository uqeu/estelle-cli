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
  binaryName,
  defaultCacheRoot,
  expectedChecksum,
  install,
  nativeBinaryPath,
  tarCommand,
  targetFor,
  validateDownloadUrl,
} = require('../install.js');
const { writeStandIn, writeScript } = require('./standin.js');

const TARGETS = [
  ['darwin', 'arm64', 'aarch64-apple-darwin'],
  ['darwin', 'x64', 'x86_64-apple-darwin'],
  ['linux', 'x64', 'x86_64-unknown-linux-gnu'],
  ['linux', 'arm64', 'aarch64-unknown-linux-gnu'],
  ['win32', 'x64', 'x86_64-pc-windows-msvc'],
];

// `member` is the archive's single entry: `estelle`, or `estelle.exe` for the Windows target.
function fixtureArchive(root, member = 'estelle') {
  const payload = path.join(root, 'payload');
  fs.mkdirSync(payload);
  writeStandIn(path.join(payload, member));
  const archive = path.join(root, 'estelle.tar.gz');
  // The same tar the installer runs, so a Windows run packs with the bsdtar it unpacks with.
  const result = spawnSync(tarCommand(), ['-czf', archive, '-C', payload, member], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.existsSync(archive), true);
  return fs.readFileSync(archive);
}

test('the five release platforms map to the five published targets', () => {
  assert.equal(TARGETS.length, 5);
  for (const [platform, arch, target] of TARGETS) assert.equal(targetFor(platform, arch), target);
  assert.throws(() => targetFor('freebsd', 'x64'), /unsupported operating system/);
  assert.throws(() => targetFor('linux', 'ia32'), /unsupported architecture/);
  // No aarch64-pc-windows-msvc build is published, so ARM64 Windows is refused BY NAME rather than
  // mapped to a target whose archive does not exist (which would surface as an opaque HTTP 404).
  assert.throws(() => targetFor('win32', 'arm64'), /unsupported architecture on Windows: arm64/);
  assert.throws(() => targetFor('win32', 'ia32'), /unsupported architecture/);
});

test('Windows installs estelle.exe; every other target installs estelle', () => {
  const root = path.resolve('/pkg');
  for (const [, , target] of TARGETS) {
    const expected = target === 'x86_64-pc-windows-msvc' ? 'estelle.exe' : 'estelle';
    assert.equal(binaryName(target), expected);
    assert.equal(path.basename(nativeBinaryPath(root, target)), expected);
    assert.equal(path.basename(path.dirname(nativeBinaryPath(root, target))), target);
  }
  assert.throws(() => binaryName('aarch64-pc-windows-msvc'), /invalid release target/);
  assert.throws(() => nativeBinaryPath(root, 'x86_64-pc-windows-gnu'), /invalid release target/);
  assert.throws(() => nativeBinaryPath('relative', 'x86_64-pc-windows-msvc'), /absolute/);
});

test('on Windows the installer runs System32 bsdtar, never whatever tar is first on PATH', () => {
  assert.equal(tarCommand('linux', {}), 'tar');
  assert.equal(tarCommand('darwin', { SystemRoot: 'C:\\Windows' }), 'tar');
  assert.equal(tarCommand('win32', { SystemRoot: 'D:\\WINNT' }), 'D:\\WINNT\\System32\\tar.exe');
  assert.equal(tarCommand('win32', {}), 'C:\\Windows\\System32\\tar.exe');
});

test('the Windows cache lives under LOCALAPPDATA, elsewhere under XDG_CACHE_HOME', () => {
  assert.equal(
    defaultCacheRoot('win32', { LOCALAPPDATA: 'C:\\Users\\u\\AppData\\Local' }),
    'C:\\Users\\u\\AppData\\Local\\estelle\\cache',
  );
  assert.equal(defaultCacheRoot('linux', { XDG_CACHE_HOME: '/x/cache' }), path.join('/x/cache', 'estelle'));
  assert.equal(path.basename(defaultCacheRoot('win32', {})), 'estelle', 'no LOCALAPPDATA falls back to ~/.cache');
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
  // Windows has no POSIX mode bits to assert; there the profile ACL is the only access control.
  if (process.platform !== 'win32') assert.equal(fs.statSync(binary).mode & 0o777, 0o755);
  if (process.platform !== 'win32') {
    const script = writeScript(root, 'native', 'native-estelle');
    assert.equal(spawnSync(binary, [script], { encoding: 'utf8' }).stdout.trim(), 'native-estelle');
  }
  assert.deepEqual(requests.map((request) => request[1]), [MAX_MANIFEST_BYTES, MAX_ARCHIVE_BYTES]);
});

// The archive for the Windows target holds `estelle.exe`, and the installer must expose exactly that
// name. On a Windows host the installed file is also STARTED, which is the only proof that what was
// unpacked is a runnable PE image rather than merely a file with the right name.
async function installWindowsArchive(t, member) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-npm-win.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const archive = fixtureArchive(root, member);
  const target = targetFor('win32', 'x64');
  const hash = crypto.createHash('sha256').update(archive).digest('hex');
  const manifest = Buffer.from(`${hash}  estelle-${target}.tar.gz\n`);
  const urls = [];
  const download = async (url) => {
    urls.push(url);
    return url.endsWith('SHA256SUMS') ? manifest : archive;
  };
  return { root, target, urls, run: () => install({ platform: 'win32', arch: 'x64', packageDir: root, download }) };
}

test('the Windows archive installs vendor/x86_64-pc-windows-msvc/estelle.exe', async (t) => {
  const fixture = await installWindowsArchive(t, 'estelle.exe');
  const binary = await fixture.run();
  assert.equal(binary, nativeBinaryPath(fixture.root, fixture.target));
  assert.match(binary, /estelle\.exe$/);
  assert.equal(fixture.urls[1].endsWith('/estelle-x86_64-pc-windows-msvc.tar.gz'), true, fixture.urls[1]);
  if (process.platform === 'win32') {
    const script = writeScript(fixture.root, 'win', 'windows-estelle');
    const result = spawnSync(binary, [script], { encoding: 'utf8' });
    assert.equal(result.status, 0, String(result.error || result.stderr));
    assert.equal(result.stdout.trim(), 'windows-estelle');
  }
});

test('a Windows archive whose member is not estelle.exe installs nothing', async (t) => {
  const fixture = await installWindowsArchive(t, 'estelle');
  await assert.rejects(fixture.run(), /exactly one estelle\.exe binary/);
  assert.equal(fs.existsSync(nativeBinaryPath(fixture.root, fixture.target)), false);
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
