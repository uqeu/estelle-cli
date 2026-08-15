#!/usr/bin/env node
'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const https = require('node:https');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const PACKAGE_VERSION = require('./package.json').version;
if (!/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?$/.test(PACKAGE_VERSION)) {
  throw new Error('npm shim version must be exact SemVer');
}
const RELEASE_BASE = `https://github.com/uqeu/estelle-cli/releases/download/v${PACKAGE_VERSION}`;
const MAX_REDIRECTS = 5;
const MAX_MANIFEST_BYTES = 64 * 1024;
const MAX_ARCHIVE_BYTES = 512 * 1024 * 1024;
const ALLOWED_DOWNLOAD_HOSTS = new Set(['github.com', 'release-assets.githubusercontent.com']);

function targetFor(platform, arch) {
  const platforms = { darwin: 'apple-darwin', linux: 'unknown-linux-gnu' };
  const arches = { arm64: 'aarch64', x64: 'x86_64' };
  if (!platforms[platform]) throw new Error(`unsupported operating system: ${platform}`);
  if (!arches[arch]) throw new Error(`unsupported architecture: ${arch}`);
  return `${arches[arch]}-${platforms[platform]}`;
}

function nativeBinaryPath(packageDir, target) {
  if (!path.isAbsolute(packageDir)) throw new Error('package directory must be absolute');
  if (!/^(aarch64|x86_64)-(apple-darwin|unknown-linux-gnu)$/.test(target)) {
    throw new Error(`invalid release target: ${target}`);
  }
  return path.join(packageDir, 'vendor', target, 'estelle');
}

function validateDownloadUrl(rawUrl) {
  const url = new URL(rawUrl);
  if (url.protocol !== 'https:') throw new Error('release download must use HTTPS');
  if (!ALLOWED_DOWNLOAD_HOSTS.has(url.hostname)) {
    throw new Error(`release redirect left GitHub: ${url.hostname}`);
  }
  return url;
}

function requestBuffer(rawUrl, limit, redirects = 0) {
  if (!Number.isSafeInteger(limit) || limit <= 0) throw new Error('download limit must be positive');
  if (!Number.isSafeInteger(redirects) || redirects < 0 || redirects > MAX_REDIRECTS) {
    throw new Error('release download exceeded its redirect bound');
  }
  const url = validateDownloadUrl(rawUrl);
  return new Promise((resolve, reject) => {
    const request = https.get(url, (response) => {
      const location = response.headers.location;
      if ([301, 302, 303, 307, 308].includes(response.statusCode) && location) {
        response.resume();
        requestBuffer(new URL(location, url).toString(), limit, redirects + 1).then(resolve, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`release download returned HTTP ${response.statusCode}`));
        return;
      }
      const declared = Number(response.headers['content-length'] || 0);
      if (declared > limit) {
        response.destroy(new Error(`release download exceeded ${limit} bytes`));
        return;
      }
      const chunks = [];
      let size = 0;
      response.on('data', (chunk) => {
        size += chunk.length;
        if (size > limit) response.destroy(new Error(`release download exceeded ${limit} bytes`));
        else chunks.push(chunk);
      });
      response.on('end', () => resolve(Buffer.concat(chunks)));
      response.on('error', reject);
    });
    request.setTimeout(30_000, () => request.destroy(new Error('release download timed out')));
    request.on('error', reject);
  });
}

function expectedChecksum(manifest, archiveName) {
  if (!/^[A-Za-z0-9._-]+\.tar\.gz$/.test(archiveName)) throw new Error('invalid archive name');
  const lines = manifest.toString('utf8').split(/\r?\n/);
  const matches = lines.filter((line) => line.endsWith(`  ${archiveName}`));
  if (matches.length !== 1) throw new Error(`checksum manifest must name ${archiveName} exactly once`);
  const checksum = matches[0].slice(0, 64);
  if (!/^[0-9a-f]{64}$/.test(checksum) || matches[0] !== `${checksum}  ${archiveName}`) {
    throw new Error(`checksum manifest has an invalid entry for ${archiveName}`);
  }
  return checksum;
}

function runTar(args) {
  const result = spawnSync('tar', args, { encoding: 'utf8', maxBuffer: MAX_MANIFEST_BYTES });
  if (result.error) throw new Error(`tar could not run: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`tar rejected the release archive: ${result.stderr.trim()}`);
  return result.stdout;
}

function installVerifiedArchive(packageDir, target, manifest, archive) {
  const archiveName = `estelle-${target}.tar.gz`;
  const expected = expectedChecksum(manifest, archiveName);
  const actual = crypto.createHash('sha256').update(archive).digest('hex');
  if (actual !== expected) throw new Error(`checksum mismatch for ${archiveName}; nothing was installed`);

  const staging = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-npm-install.'));
  try {
    const archivePath = path.join(staging, archiveName);
    fs.writeFileSync(archivePath, archive, { mode: 0o600, flag: 'wx' });
    if (runTar(['-tzf', archivePath]).trim() !== 'estelle') {
      throw new Error('release archive must contain exactly one estelle binary');
    }
    runTar(['-xzf', archivePath, '-C', staging]);
    const unpacked = path.join(staging, 'estelle');
    const metadata = fs.lstatSync(unpacked);
    if (!metadata.isFile() || metadata.isSymbolicLink()) throw new Error('release binary is not a regular file');

    const destination = nativeBinaryPath(packageDir, target);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    const temporary = `${destination}.installing-${process.pid}`;
    fs.copyFileSync(unpacked, temporary, fs.constants.COPYFILE_EXCL);
    fs.chmodSync(temporary, 0o755);
    fs.renameSync(temporary, destination);
    assert.equal(fs.lstatSync(destination).isFile(), true, 'installed binary must remain a regular file');
    assert.equal(fs.lstatSync(destination).isSymbolicLink(), false, 'installed binary must never be a symlink');
    return destination;
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}

async function install(options = {}) {
  const platform = options.platform || process.platform;
  const arch = options.arch || process.arch;
  const packageDir = options.packageDir || __dirname;
  const download = options.download || requestBuffer;
  const target = targetFor(platform, arch);
  const archiveName = `estelle-${target}.tar.gz`;
  const manifest = await download(`${RELEASE_BASE}/SHA256SUMS`, MAX_MANIFEST_BYTES);
  const archive = await download(`${RELEASE_BASE}/${archiveName}`, MAX_ARCHIVE_BYTES);
  if (!Buffer.isBuffer(manifest) || !Buffer.isBuffer(archive)) throw new Error('release download returned non-bytes');
  return installVerifiedArchive(packageDir, target, manifest, archive);
}

module.exports = {
  MAX_ARCHIVE_BYTES,
  MAX_MANIFEST_BYTES,
  expectedChecksum,
  install,
  installVerifiedArchive,
  nativeBinaryPath,
  requestBuffer,
  targetFor,
  validateDownloadUrl,
};

if (require.main === module) {
  install()
    .then((binary) => console.log(`Installed verified native Estelle CLI to ${binary}`))
    .catch((error) => {
      console.error(`estelle install: ${error.message}`);
      process.exitCode = 1;
    });
}
