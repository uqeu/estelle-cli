'use strict';

// A stand-in for the native CLI that runs on every platform the launcher supports.
//
// On POSIX it is a two-line `sh` script that execs node; on Windows it is a byte copy of node.exe
// saved as `estelle.exe`, because Windows will only start a real PE image — a `#!/bin/sh` file
// named `.exe` fails with `UNKNOWN`/`ENOEXEC`, which is exactly the bug a Windows run must be able
// to see. Either way the launcher passes argv through untouched, so the FIRST argument names the
// JavaScript file the stand-in runs. That keeps one test body for both platforms instead of a
// POSIX suite plus a Windows suite that could drift into testing different contracts.

const fs = require('node:fs');
const path = require('node:path');

function writeStandIn(binary) {
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  if (process.platform === 'win32') {
    fs.copyFileSync(process.execPath, binary);
  } else {
    fs.writeFileSync(binary, `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} "$@"\n`, { mode: 0o755 });
  }
  if (!fs.statSync(binary).isFile()) throw new Error(`stand-in was not written: ${binary}`);
  return binary;
}

// Writes a script that prints `text` and exits with `status`, and returns its absolute path.
function writeScript(dir, name, text, status = 0) {
  if (!Number.isInteger(status) || status < 0 || status > 255) throw new Error('status must be a byte');
  const file = path.join(dir, `${name}.js`);
  fs.writeFileSync(
    file,
    `process.stdout.write(${JSON.stringify(text)} + process.argv.slice(2).join(' ') + '\\n');\n`
      + `process.exitCode = ${status};\n`,
  );
  return file;
}

module.exports = { writeStandIn, writeScript };
