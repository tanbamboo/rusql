#!/usr/bin/env node
/**
 * zh-CN / en-US user-guide section header parity check.
 */
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const enPath = join(root, 'docs/en/user-guide.md');
const zhPath = join(root, 'docs/zh-CN/user-guide.md');
let errors = 0;

function fail(msg) {
  console.error(`FAIL: ${msg}`);
  errors++;
}

function ok(msg) {
  console.log(`OK: ${msg}`);
}

function sectionCount(text) {
  return (text.match(/^##\s+/gm) ?? []).length;
}

function hasMarkers(text, markers) {
  return markers.every((m) => text.includes(m));
}

const REQUIRED_MARKERS = ['harness-validate', 'compat', 'rusql-server'];

if (errors === 0) {
  const enText = readFileSync(enPath, 'utf8');
  const zhText = readFileSync(zhPath, 'utf8');
  const enN = sectionCount(enText);
  const zhN = sectionCount(zhText);
  if (Math.abs(enN - zhN) > 2) {
    fail(`section count diverged: en=${enN} zh=${zhN} (tolerance 2)`);
  } else {
    ok(`section count within tolerance (en=${enN}, zh=${zhN})`);
  }
  for (const markers of [
    [enText, 'en'],
    [zhText, 'zh'],
  ]) {
    if (!hasMarkers(markers[0], REQUIRED_MARKERS)) {
      fail(`${markers[1]} user-guide missing required markers`);
    }
  }
  ok('required content markers present in both locales');
}

if (errors > 0) {
  console.error(`\nDoc parity failed with ${errors} error(s).`);
  process.exit(1);
}

console.log('\nDoc parity passed.');
process.exit(0);
