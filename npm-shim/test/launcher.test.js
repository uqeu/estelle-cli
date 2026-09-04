'use strict';

// The launcher contract, driven the way a customer drives it: `node bin/estelle.js` inside a real
// package directory. These tests exist because npm 12 blocks `postinstall` by default, so the
// launcher — not the postinstall — is the last thing standing between a customer and `ENOENT`.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const SHIM_DIR = path.resolve(__dirname, '..');
const LAUNCHER_SOURCE = fs.readFileSync(path.join(SHIM_DIR, 'bin', 'estelle.js'));
const INSTALL_SOURCE = fs.readFileSync(path.join(SHIM_DIR, 'install.js'));
const MANIFEST_SOURCE = fs.readFileSync(path.join(SHIM_DIR, 'package.json'));
const { targetFor, nativeBinaryPath } = require('../install.js');

// A package directory shaped exactly like the published tarball: bin/estelle.js, install.js,
// package.json. `installJs` may be replaced by a stub so the lazy-download path is observable
// without touching the network.
function fakePackage(root, installJs = INSTALL_SOURCE) {
  fs.mkdirSync(path.join(root, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(root, 'bin', 'estelle.js'), LAUNCHER_SOURCE);
  fs.writeFileSync(path.join(root, 'install.js'), installJs);
  fs.writeFileSync(path.join(root, 'package.json'), MANIFEST_SOURCE);
  return path.join(root, 'bin', 'estelle.js');
}

function writeNativeBinary(packageDir, target, body) {
  const binary = nativeBinaryPath(packageDir, target);
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  fs.writeFileSync(binary, body, { mode: 0o755 });
  return binary;
}

function runLauncher(launcher, args, env = {}) {
  return spawnSync(process.execPath, [launcher, ...args], {
    encoding: 'utf8',
    env: { ...process.env, ESTELLE_CACHE_DIR: path.join(path.dirname(path.dirname(launcher)), 'cache'), ...env },
  });
}

const TARGET = targetFor(process.platform, process.arch);

test('the launcher runs the vendored binary and passes arguments through', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-ok.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const launcher = fakePackage(root);
  writeNativeBinary(root, TARGET, '#!/bin/sh\nprintf "native %s\\n" "$*"\n');

  const result = runLauncher(launcher, ['--version', '--extra']);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), 'native --version --extra');
  // The happy path must not mention downloading; a customer with a working install sees nothing.
  assert.equal(/downloading|fetching/i.test(result.stderr), false, result.stderr);
});

test('the exit status of the native binary is the exit status of the launcher', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-status.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const launcher = fakePackage(root);
  writeNativeBinary(root, TARGET, '#!/bin/sh\nexit 42\n');

  assert.equal(runLauncher(launcher, []).status, 42);
});

test('a missing binary with downloads disabled names npm 12 and prints a working repair command', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-diag.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const launcher = fakePackage(root);

  const result = runLauncher(launcher, ['--version'], { ESTELLE_SKIP_DOWNLOAD: '1' });
  assert.equal(result.status, 1);
  // The bar is Claude Code's launcher: name the cause, then print the exact repair.
  assert.match(result.stderr, /npm 12 blocks package install scripts by default/);
  assert.match(result.stderr, /postinstall/);
  assert.match(result.stderr, /npm install -g @fatelabs\/estelle --allow-scripts=@fatelabs\/estelle/);
  assert.match(result.stderr, /install\.sh/);
  // The searched path is named, so a reader can check it themselves.
  assert.match(result.stderr, new RegExp(`vendor.${TARGET}.estelle`));
  // And the old, uninformative failure must be gone.
  assert.equal(/ENOENT/.test(result.stderr), false, result.stderr);
});

test('a missing binary triggers the verified install, then runs what it installed', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-lazy.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const marker = path.join(root, 'install-was-called');
  // A stub install.js with the launcher's exact import surface. If the launcher stops calling
  // install(), or asks for a name this module does not export, this test goes red.
  const stub = `'use strict';
const fs = require('node:fs');
const path = require('node:path');
const real = ${JSON.stringify(path.join(SHIM_DIR, 'install.js'))};
const upstream = require(real);
module.exports = {
  ...upstream,
  install: async (options) => {
    fs.writeFileSync(${JSON.stringify(marker)}, JSON.stringify({ packageDir: options.packageDir }));
    const binary = upstream.nativeBinaryPath(options.packageDir, upstream.targetFor(process.platform, process.arch));
    fs.mkdirSync(path.dirname(binary), { recursive: true });
    fs.writeFileSync(binary, '#!/bin/sh\\nprintf "lazily-installed\\\\n"\\n', { mode: 0o755 });
    return binary;
  },
};
`;
  const launcher = fakePackage(root, Buffer.from(stub));

  const result = runLauncher(launcher, ['--version']);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), 'lazily-installed');
  assert.equal(fs.existsSync(marker), true, 'the launcher must call install() when the binary is absent');
  // macOS puts the temp root behind a /var -> /private/var symlink; compare resolved paths.
  assert.equal(
    fs.realpathSync(JSON.parse(fs.readFileSync(marker, 'utf8')).packageDir),
    fs.realpathSync(root),
  );
  assert.match(result.stderr, /npm blocked this package's install script/);
});

test('a failing install falls through to the diagnostic rather than a raw stack', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-fail.'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const stub = `'use strict';
const upstream = require(${JSON.stringify(path.join(SHIM_DIR, 'install.js'))});
module.exports = { ...upstream, install: async () => { throw new Error('release download returned HTTP 403'); } };
`;
  const launcher = fakePackage(root, Buffer.from(stub));

  const result = runLauncher(launcher, ['--version']);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Automatic recovery failed: release download returned HTTP 403/);
  assert.match(result.stderr, /npm install -g @fatelabs\/estelle --allow-scripts=@fatelabs\/estelle/);
  assert.equal(/at Object\.<anonymous>/.test(result.stderr), false, 'no raw stack trace');
});

test('an unwritable package directory installs into the per-user cache instead', (t) => {
  if (typeof process.getuid === 'function' && process.getuid() === 0) {
    t.skip('root can write a 0o555 directory, so this case is unobservable as root');
    return;
  }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-cache.'));
  t.after(() => {
    fs.chmodSync(root, 0o700);
    fs.rmSync(root, { recursive: true, force: true });
  });
  const stub = `'use strict';
const fs = require('node:fs');
const path = require('node:path');
const upstream = require(${JSON.stringify(path.join(SHIM_DIR, 'install.js'))});
module.exports = {
  ...upstream,
  install: async (options) => {
    const binary = upstream.nativeBinaryPath(options.packageDir, upstream.targetFor(process.platform, process.arch));
    fs.mkdirSync(path.dirname(binary), { recursive: true });
    fs.writeFileSync(binary, '#!/bin/sh\\nprintf "from-cache\\\\n"\\n', { mode: 0o755 });
    return binary;
  },
};
`;
  const launcher = fakePackage(root, Buffer.from(stub));
  const cache = fs.mkdtempSync(path.join(os.tmpdir(), 'estelle-launcher-cachedir.'));
  t.after(() => fs.rmSync(cache, { recursive: true, force: true }));
  fs.chmodSync(root, 0o555);

  const result = runLauncher(launcher, ['--version'], { ESTELLE_CACHE_DIR: cache });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), 'from-cache');
  const cached = path.join(cache, `v${JSON.parse(MANIFEST_SOURCE.toString('utf8')).version}`);
  assert.equal(fs.existsSync(nativeBinaryPath(cached, TARGET)), true, 'the cache copy must exist');

  // And a second run must reuse the cache without installing again.
  const again = runLauncher(launcher, ['--version'], { ESTELLE_CACHE_DIR: cache });
  assert.equal(again.stdout.trim(), 'from-cache');
  assert.equal(/npm blocked/.test(again.stderr), false, again.stderr);
});
