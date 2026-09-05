#!/usr/bin/env node
'use strict';

// 🔴 npm 12 BLOCKS `postinstall` BY DEFAULT, AND `postinstall` IS THE ONLY THING THAT FETCHES THE
// NATIVE BINARY. `npm i -g @fatelabs/estelle` then exits 0 and leaves a CLI that cannot start.
// Measured 2026-09-04 on one box varying only the npm major: npm 10.9.9 ok, npm 11.19.1 ok,
// npm 12.0.2 broken with `spawnSync .../vendor/<target>/estelle ENOENT`. npm 12 ships with current
// Node, so that is the DEFAULT first experience, not an edge case.
//
// So this launcher does two things a bare `spawnSync` cannot:
//   1. If the vendored binary is absent it performs the SAME checksum-verified install the
//      postinstall would have performed, bounded and announced, then runs it. This is what makes
//      the install actually work rather than merely explain itself.
//   2. If that recovery cannot run (offline, proxy, 403, read-only prefix) it prints a diagnostic
//      that NAMES the cause and PRINTS the exact repair command, instead of `ENOENT`.

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const {
  PACKAGE_VERSION,
  RELEASE_BASE,
  fallbackPackageDir,
  install,
  nativeBinaryPath,
  targetFor,
} = require('../install.js');

const PACKAGE_NAME = '@fatelabs/estelle';
const PACKAGE_DIR = path.resolve(__dirname, '..');
const PROGRESS_INTERVAL_MILLIS = 400;
const STDERR_WRITE_ATTEMPTS = 100;

// `console.error` on a pipe is ASYNCHRONOUS, and `process.exit` does not wait for it. The whole
// point of this launcher is a message the customer actually reads, so the message that matters is
// written with a synchronous write instead. The bounded retry covers EAGAIN on a non-blocking fd.
function emitRaw(text) {
  const payload = Buffer.from(text, 'utf8');
  for (let attempt = 0; attempt < STDERR_WRITE_ATTEMPTS; attempt += 1) {
    try {
      fs.writeSync(2, payload);
      return;
    } catch (error) {
      if (error && error.code === 'EAGAIN') continue;
      return; // EPIPE and friends: the reader is gone, and there is nothing better to do.
    }
  }
}

function emit(text) {
  emitRaw(`${text}\n`);
}

// Returns the runnable native binary under `packageDir`, or null. Never throws for a missing or
// unreadable path: "absent" and "present but not executable" both mean the caller must install.
function existingBinary(packageDir, target) {
  if (typeof packageDir !== 'string' || packageDir.length === 0) return null;
  let candidate;
  try {
    candidate = nativeBinaryPath(packageDir, target);
  } catch {
    return null;
  }
  try {
    if (!fs.statSync(candidate).isFile()) return null;
    fs.accessSync(candidate, fs.constants.X_OK);
  } catch {
    return null;
  }
  return candidate;
}

function cacheDirOrNull() {
  try {
    return fallbackPackageDir();
  } catch {
    return null;
  }
}

// The package directory when the running user may write it, else a per-user, per-version cache.
// A `sudo npm i -g` leaves root-owned files, and the customer who then runs `estelle` is not root.
//
// ⚠️ SAY THE TRADE-OFF OUT LOUD. Both paths are checksum-verified against the release's SHA256SUMS
// at INSTALL time and neither is re-verified on every run — that was already true of `vendor/`.
// What is new is that on a root-owned prefix the executed binary now sits somewhere the running
// user can write. That crosses no privilege boundary: anything able to write `~/.cache` as this
// user can already edit their shell rc or `~/.local/bin`. It is still strictly more surface than
// a root-owned `vendor/`, and the honest alternative — refusing, and telling a `sudo`-installed
// customer to re-run with `sudo … --allow-scripts` — leaves that whole population broken today.
function writableInstallRoot() {
  try {
    fs.accessSync(PACKAGE_DIR, fs.constants.W_OK);
    return PACKAGE_DIR;
  } catch {
    const cache = cacheDirOrNull();
    if (cache === null) throw new Error('no writable directory: the package directory is read-only and no home cache exists');
    fs.mkdirSync(cache, { recursive: true });
    fs.accessSync(cache, fs.constants.W_OK);
    return cache;
  }
}

function reportProgress() {
  let last = 0;
  return (size, declared) => {
    const now = Date.now();
    if (now - last < PROGRESS_INTERVAL_MILLIS) return;
    last = now;
    const megabytes = (size / (1024 * 1024)).toFixed(1);
    const total = declared > 0 ? ` of ${(declared / (1024 * 1024)).toFixed(1)} MB` : '';
    const line = `estelle: downloading native CLI ${megabytes} MB${total}`;
    // Same synchronous fd-2 writer as `emit`, so progress can never interleave behind it.
    if (process.stderr.isTTY) emitRaw(`\r${line}   `);
    else emit(line);
  };
}

function diagnose(target, reason) {
  const cache = cacheDirOrNull();
  const lines = [
    `estelle: the native CLI for ${target} is not installed, so there is nothing to run.`,
    '',
    '  Cause: npm 12 blocks package install scripts by default, and this package\'s',
    `         postinstall (\`node install.js\`) is the only step that fetches the native`,
    '         binary. The install reports success and leaves no `vendor/` directory.',
    `  Automatic recovery failed: ${reason}`,
    '',
    '  Repair with any one of these:',
    `    npm install -g ${PACKAGE_NAME} --allow-scripts=${PACKAGE_NAME}`,
    `    node ${path.join(PACKAGE_DIR, 'install.js')}`,
    "    curl --proto '=https' --tlsv1.2 -fsSL \\",
    '      https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh',
    '',
    `  This build expects release v${PACKAGE_VERSION}: ${RELEASE_BASE}`,
    `  Searched: ${nativeBinaryPathOrUnknown(PACKAGE_DIR, target)}`,
  ];
  if (cache !== null) lines.push(`            ${nativeBinaryPathOrUnknown(cache, target)}`);
  emit(lines.join('\n'));
}

function nativeBinaryPathOrUnknown(packageDir, target) {
  try {
    return nativeBinaryPath(packageDir, target);
  } catch {
    return `<unresolvable path under ${packageDir}>`;
  }
}

function run(binary) {
  const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
  if (result.error) {
    emit(`estelle: could not start the verified native CLI at ${binary}: ${result.error.message}`);
    return 1;
  }
  if (result.signal) {
    emit(`estelle: native CLI terminated by ${result.signal}`);
    return 1;
  }
  return result.status ?? 1;
}

async function main() {
  let target;
  try {
    target = targetFor(process.platform, process.arch);
  } catch (error) {
    emit(`estelle: ${error.message}`);
    return 1;
  }

  const cache = cacheDirOrNull();
  const ready = existingBinary(PACKAGE_DIR, target) || (cache && existingBinary(cache, target));
  if (ready) return run(ready);

  if (process.env.ESTELLE_SKIP_DOWNLOAD) {
    diagnose(target, 'ESTELLE_SKIP_DOWNLOAD is set, so no download was attempted');
    return 1;
  }

  try {
    const destination = writableInstallRoot();
    emit(
      `estelle: the native binary is missing — npm blocked this package's install script.\n`
      + `estelle: fetching the checksum-verified v${PACKAGE_VERSION} release for ${target} into ${destination} …`,
    );
    const binary = await install({ packageDir: destination, onProgress: reportProgress() });
    if (process.stderr.isTTY) emitRaw('\n');
    emit(`estelle: verified and installed ${binary}`);
    return run(binary);
  } catch (error) {
    if (process.stderr.isTTY) emitRaw('\n');
    diagnose(target, error && error.message ? error.message : String(error));
    return 1;
  }
}

main().then(
  (code) => process.exit(code),
  (error) => {
    emit(`estelle: ${error && error.message ? error.message : String(error)}`);
    process.exit(1);
  },
);
