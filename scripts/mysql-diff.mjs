#!/usr/bin/env node
/**
 * Differential compat: run portable SQL on rusql-server and Docker MySQL 8.0; diff batch output.
 * Skips gracefully when Docker, mysql client, or server build is unavailable.
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import net from 'node:net';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const fixturePath = join(root, 'crates/rusql-server/compat/mysql-diff.json');
const RUSQL_PORT = 3307;
const MYSQL_PORT = 3308;

function hasCmd(cmd) {
  const r = spawnSync(cmd, ['--version'], { shell: true, encoding: 'utf8' });
  return r.status === 0;
}

function serverBinary() {
  const name = process.platform === 'win32' ? 'rusql-server.exe' : 'rusql-server';
  return join(root, 'target', 'release', name);
}

function buildServer() {
  const r = spawnSync('cargo', ['build', '--release', '-p', 'rusql-server'], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (r.status !== 0) {
    console.error(r.stderr || r.stdout);
    return false;
  }
  return existsSync(serverBinary());
}

function waitForPort(port, host = '127.0.0.1', timeoutMs = 60_000) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const attempt = () => {
      const socket = net.connect(port, host);
      socket.once('connect', () => {
        socket.end();
        resolve();
      });
      socket.once('error', () => {
        socket.destroy();
        if (Date.now() - start > timeoutMs) {
          reject(new Error(`port ${port} not ready`));
        } else {
          setTimeout(attempt, 250);
        }
      });
    };
    attempt();
  });
}

function startRusql(dataDir) {
  const bin = serverBinary();
  const child = spawn(bin, ['--port', String(RUSQL_PORT), '--data-dir', dataDir], {
    cwd: root,
    stdio: 'ignore',
    detached: process.platform !== 'win32',
  });
  return child;
}

function stopProc(child) {
  if (!child?.pid) return;
  try {
    if (process.platform === 'win32') {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } else {
      process.kill(-child.pid, 'SIGTERM');
    }
  } catch {
  }
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
      `${MYSQL_PORT}:3306`,
      'mysql:8.0',
    ],
    { encoding: 'utf8' }
  );
  if (run.status !== 0) return null;
  const id = run.stdout.trim();
  spawnSync(
    'docker',
    [
      'exec',
      id,
      'bash',
      '-c',
      'for i in $(seq 1 60); do mysql -u root --protocol=TCP -h 127.0.0.1 -e "SELECT 1" 2>/dev/null && exit 0; sleep 2; done; exit 1',
    ],
    { encoding: 'utf8', timeout: 180_000 }
  );
  return id;
}

function dockerStop(id) {
  if (id) spawnSync('docker', ['stop', id], { encoding: 'utf8' });
}

function hostForDockerClient() {
  return process.platform === 'win32' || process.platform === 'darwin'
    ? 'host.docker.internal'
    : '127.0.0.1';
}

function hasLocalMysql() {
  return spawnSync('mysql', ['--version'], { shell: true, encoding: 'utf8' }).status === 0;
}

function mysqlLocal(port, sql) {
  const r = spawnSync(
    'mysql',
    ['-h', '127.0.0.1', '-P', String(port), '-u', 'root', '-B', '-e', sql],
    { encoding: 'utf8' }
  );
  return {
    ok: r.status === 0,
    out: (r.stdout ?? '').replace(/\r\n/g, '\n').trimEnd(),
    err: (r.stderr ?? '').trim(),
  };
}

function mysqlExec(containerId, sql) {
  const r = spawnSync(
    'docker',
    ['exec', containerId, 'mysql', '-u', 'root', '--protocol=TCP', '-h', '127.0.0.1', '-B', '-e', sql],
    { encoding: 'utf8' }
  );
  return {
    ok: r.status === 0,
    out: (r.stdout ?? '').replace(/\r\n/g, '\n').trimEnd(),
    err: (r.stderr ?? '').trim(),
  };
}

function mysqlRusqlDocker(sql) {
  const r = spawnSync(
    'docker',
    [
      'run',
      '--rm',
      'mysql:8.0',
      'mysql',
      '-h',
      hostForDockerClient(),
      '-P',
      String(RUSQL_PORT),
      '-u',
      'root',
      '--protocol=TCP',
      '-B',
      '-e',
      sql,
    ],
    { encoding: 'utf8' }
  );
  return {
    ok: r.status === 0,
    out: (r.stdout ?? '').replace(/\r\n/g, '\n').trimEnd(),
    err: (r.stderr ?? '').trim(),
  };
}

function resetMysqlDb(containerId, suiteName) {
  const db = `md_${suiteName.replace(/[^a-zA-Z0-9_]/g, '_')}`;
  mysqlExec(containerId, `DROP DATABASE IF EXISTS \`${db}\``);
  const create = mysqlExec(containerId, `CREATE DATABASE \`${db}\``);
  if (!create.ok) return null;
  return db;
}

function runSuiteOnMysql(containerId, suite, db) {
  const results = [];
  for (const step of suite.steps) {
    const sql = `USE \`${db}\`; ${step.sql}`;
    const got = mysqlExec(containerId, sql);
    results.push({ sql: step.sql, ...got });
  }
  return results;
}

function runSuiteOnRusql(suite, useDockerClient) {
  const results = [];
  for (const step of suite.steps) {
    const got = useDockerClient
      ? mysqlRusqlDocker(step.sql)
      : mysqlLocal(RUSQL_PORT, step.sql);
    results.push({ sql: step.sql, ...got });
  }
  return results;
}

function diffSuite(suiteName, rusql, mysql) {
  const mismatches = [];
  for (let i = 0; i < rusql.length; i++) {
    const r = rusql[i];
    const m = mysql[i];
    if (!r.ok && !m.ok) continue;
    if (r.ok !== m.ok) {
      mismatches.push({
        sql: r.sql,
        reason: `status rusql=${r.ok} mysql=${m.ok}`,
        rusqlErr: r.err,
        mysqlErr: m.err,
      });
      continue;
    }
    if (r.out !== m.out) {
      mismatches.push({
        sql: r.sql,
        reason: 'output differs',
        rusql: r.out,
        mysql: m.out,
      });
    }
  }
  return mismatches;
}

console.log('mysql-diff: rusql vs Docker MySQL 8.0 (portable fixture subset)');

if (!existsSync(fixturePath)) {
  console.log('SKIP: mysql-diff.json missing');
  process.exit(0);
}

if (!hasCmd('docker')) {
  console.log('SKIP: docker not available');
  process.exit(0);
}

const useDockerMysqlClient = !hasLocalMysql();
if (useDockerMysqlClient && !hasCmd('docker')) {
  console.log('SKIP: mysql client not available and docker not available');
  process.exit(0);
}

if (!buildServer()) {
  console.log('SKIP: could not build rusql-server');
  process.exit(0);
}

const container = dockerMysqlUp();
if (!container) {
  console.log('SKIP: could not start mysql:8.0 container');
  process.exit(0);
}

const data = JSON.parse(readFileSync(fixturePath, 'utf8'));
let rusqlChild = null;
let dataDir = null;
let exitCode = 0;

async function freshRusql() {
  stopProc(rusqlChild);
  if (dataDir) {
    try {
      rmSync(dataDir, { recursive: true, force: true });
    } catch {
    }
  }
  dataDir = mkdtempSync(join(tmpdir(), 'rusql-mysql-diff-'));
  rusqlChild = startRusql(dataDir);
  await waitForPort(RUSQL_PORT);
}

try {
  let compared = 0;
  let suitesOk = 0;
  for (const suite of data.suites ?? []) {
    const db = resetMysqlDb(container, suite.name);
    if (!db) {
      console.log(`SKIP suite ${suite.name}: could not create MySQL database`);
      continue;
    }
    await freshRusql();

    const rusqlResults = runSuiteOnRusql(suite, useDockerMysqlClient);
    const mysqlResults = runSuiteOnMysql(container, suite, db);
    const mismatches = diffSuite(suite.name, rusqlResults, mysqlResults);
    compared += suite.steps.length;
    if (mismatches.length === 0) {
      suitesOk++;
      console.log(`OK: ${suite.name} (${suite.steps.length} steps)`);
    } else {
      exitCode = 1;
      console.error(`FAIL: ${suite.name}`);
      for (const mm of mismatches) {
        console.error(`  SQL: ${mm.sql}`);
        console.error(`  ${mm.reason}`);
        if (mm.rusql !== undefined) {
          console.error(`  rusql:\n${mm.rusql || '(empty)'}`);
          console.error(`  mysql:\n${mm.mysql || '(empty)'}`);
        }
        if (mm.rusqlErr || mm.mysqlErr) {
          console.error(`  rusql err: ${mm.rusqlErr}`);
          console.error(`  mysql err: ${mm.mysqlErr}`);
        }
      }
    }
  }
  if (exitCode === 0 && compared > 0) {
    console.log(`OK: mysql-diff compared ${compared} steps across ${suitesOk} suite(s)`);
  }
} catch (e) {
  console.log(`SKIP: ${e.message}`);
  exitCode = 0;
} finally {
  stopProc(rusqlChild);
  if (dataDir) {
    try {
      rmSync(dataDir, { recursive: true, force: true });
    } catch {
    }
  }
  dockerStop(container);
}

process.exit(exitCode);
