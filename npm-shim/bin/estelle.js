#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const path = require('node:path');
const { nativeBinaryPath, targetFor } = require('../install.js');

let binary;
try {
  binary = nativeBinaryPath(__dirname + '/..', targetFor(process.platform, process.arch));
} catch (error) {
  console.error(`estelle: ${error.message}`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error(`estelle: could not start the verified native CLI: ${result.error.message}`);
  process.exit(1);
}
if (result.signal) {
  console.error(`estelle: native CLI terminated by ${result.signal}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
