#!/usr/bin/env node
/**
 * Optional differential compat: compare rusql wire results vs MySQL in Docker.
 * Skips gracefully when Docker or mysql client is unavailable.
 */
import { spawnSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const fixturePath = join(root, 'crates/rusql-server/compat/basic.json');

function hasCmd(cmd) {
  const r = spawnSync(cmd, ['--version'], { shell: true, encoding: 'utf8' });
  return r.status === 0;
}

function dockerMysqlUp() {
  const run = spawnSync(
    'docker',
    [
      'run',
      '--rm',
      '-d',
      '-e',
      'MYSQL_ALLOW_EMPTY_PASSWORD=yes',
      '-p',
      '3308:3306',
      'mysql:8.0',
    ],
    { encoding: 'utf8' }
  );
  if (run.status !== 0) return null;
  const id = run.stdout.trim();
  spawnSync('docker', ['exec', id, 'bash', '-c', 'until mysqladmin ping -h localhost --silent; do sleep 1; done'], {
    encoding: 'utf8',
    timeout: 120_000,
  });
  return id;
}

function dockerStop(id) {
  if (id) spawnSync('docker', ['stop', id], { encoding: 'utf8' });
}

console.log('mysql-diff: optional differential compat (Docker MySQL 8.0)');

if (!existsSync(fixturePath)) {
  console.log('SKIP: compat fixture missing');
  process.exit(0);
}

if (!hasCmd('docker')) {
  console.log('SKIP: docker not available');
  process.exit(0);
}

if (!hasCmd('mysql')) {
  console.log('SKIP: mysql client not available');
  process.exit(0);
}

const container = dockerMysqlUp();
if (!container) {
  console.log('SKIP: could not start mysql:8.0 container');
  process.exit(0);
}

try {
  const data = JSON.parse(readFileSync(fixturePath, 'utf8'));
  let ran = 0;
  let skipped = 0;
  for (const suite of data.suites ?? []) {
    for (const step of suite.steps ?? []) {
      const sql = step.sql;
      const r = spawnSync('mysql', ['-h', '127.0.0.1', '-P', '3308', '-u', 'root', '-e', sql], {
        encoding: 'utf8',
      });
      if (r.status === 0) {
        ran++;
      } else {
        skipped++;
        console.log(`SKIP SQL on MySQL: ${sql.slice(0, 60)}...`);
      }
    }
  }
  console.log(`OK: mysql-diff smoke ran ${ran} steps (${skipped} skipped on MySQL)`);
  console.log('NOTE: full rusql vs MySQL row diff not yet automated — use compat_suite for rusql');
} finally {
  dockerStop(container);
}

process.exit(0);
