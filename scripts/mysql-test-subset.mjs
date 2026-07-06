#!/usr/bin/env node
/**
 * Run Oracle mysql-test inspired wire subset (20+ cases) via rusql internal test client.
 */
import { spawnSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

console.log('mysql-test-subset: cargo test -p rusql-server mysql_test_subset');

const r = spawnSync('cargo', ['test', '-p', 'rusql-server', 'mysql_test_subset', '--', '--nocapture'], {
  cwd: root,
  stdio: 'inherit',
});

if (r.status !== 0) {
  process.exit(r.status ?? 1);
}

console.log('OK: mysql-test subset passed');
process.exit(0);
