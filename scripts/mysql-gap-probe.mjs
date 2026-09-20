#!/usr/bin/env node
/**
 * Inventory remaining MySQL 8.0 client SQL gaps vs rusql.
 * Always exits 0 (not a CI gate). Optional Docker MySQL 8.0 status/column-name compare.
 *
 * Usage:
 *   node scripts/mysql-gap-probe.mjs
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import net from 'node:net';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const fixturePath = join(root, 'crates/rusql-server/compat/mysql-gap-probe.json');
const MYSQL_PORT = 3309;
const MYSQL_TIMEOUT_MS = 30_000;
let rusqlPort = 3307;

function portInUse(port) {
  try {
    const r = spawnSync(
      process.platform === 'win32' ? 'netstat' : 'ss',
      process.platform === 'win32'
        ? ['-ano']
        : ['-ltn', `sport = :${port}`],
      { encoding: 'utf8', shell: process.platform === 'win32' }
    );
    const out = r.stdout ?? '';
    if (process.platform === 'win32') {
      return new RegExp(`:${port}\\s+.*LISTENING`).test(out);
    }
    return out.includes(`:${port}`);
  } catch {
    return false;
  }
}

function pickFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close((err) => (err ? reject(err) : resolve(port)));
    });
    server.on('error', reject);
  });
}

function mysqlResult(r, sql, timedOut = false) {
  if (timedOut) {
    return { ok: false, out: '', err: `timed out: ${sql}` };
  }
  return {
    ok: r.status === 0,
    out: (r.stdout ?? '').replace(/\r\n/g, '\n').trimEnd(),
    err: (r.stderr ?? '').trim(),
  };
}

function hasCmd(cmd) {
  const r = spawnSync(cmd, ['--version'], { shell: true, encoding: 'utf8' });
  return r.status === 0;
}

function serverBinary() {
  const name = process.platform === 'win32' ? 'rusql-server.exe' : 'rusql-server';
  return join(root, 'target', 'release', name);
}

function buildServer() {
  const env = { ...process.env };
  if (process.platform === 'win32' && !env.CARGO_TARGET_DIR) {
    env.CARGO_TARGET_DIR = join(root, 'target');
  }
  const r = spawnSync('cargo', ['build', '--release', '-p', 'rusql-server'], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env,
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

function startRusql(dataDir, port) {
  const bin = serverBinary();
  return spawn(bin, ['--port', String(port), '--data-dir', dataDir, '--wal-sync', 'none'], {
    cwd: root,
    stdio: 'ignore',
    env: { ...process.env },
  });
}

function stopProc(child) {
  if (!child?.pid) return;
  try {
    if (process.platform === 'win32') {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } else {
      process.kill(child.pid, 'SIGTERM');
    }
  } catch {
  }
}

function waitForPortFree(port, timeoutMs = 15_000) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const attempt = () => {
      if (!portInUse(port)) {
        resolve();
        return;
      }
      if (Date.now() - start > timeoutMs) {
        reject(new Error(`port ${port} still in use after ${timeoutMs}ms`));
        return;
      }
      setTimeout(attempt, 100);
    };
    attempt();
  });
}

function hostForDockerClient() {
  return process.platform === 'win32' || process.platform === 'darwin'
    ? 'host.docker.internal'
    : '127.0.0.1';
}

function hasLocalMysql() {
  return spawnSync('mysql', ['--version'], { shell: true, encoding: 'utf8' }).status === 0;
}

function mysqlLocalOnDb(port, database, sql) {
  const args = [
    '-h',
    '127.0.0.1',
    '-P',
    String(port),
    '-u',
    'root',
    '--ssl-mode=DISABLED',
    '-B',
    '-e',
    sql,
  ];
  if (database) {
    args.splice(8, 0, '-D', database);
  }
  const r = spawnSync('mysql', args, { encoding: 'utf8', timeout: MYSQL_TIMEOUT_MS });
  return mysqlResult(r, sql, r.error?.code === 'ETIMEDOUT');
}

function mysqlRusqlDockerOnDb(database, sql) {
  const args = [
    'run',
    '--rm',
    'mysql:8.0',
    'mysql',
    '-h',
    hostForDockerClient(),
    '-P',
    String(rusqlPort),
    '-u',
    'root',
    '--protocol=TCP',
    '--ssl-mode=DISABLED',
    '--connect-timeout=10',
    '-B',
    '-e',
    sql,
  ];
  if (database) {
    args.splice(12, 0, '-D', database);
  }
  const r = spawnSync('docker', args, { encoding: 'utf8', timeout: MYSQL_TIMEOUT_MS });
  return mysqlResult(r, sql, r.error?.code === 'ETIMEDOUT');
}

function mysqlExec(containerId, sql, db) {
  const full = db ? `USE \`${db}\`; ${sql}` : sql;
  const r = spawnSync(
    'docker',
    ['exec', containerId, 'mysql', '-u', 'root', '--protocol=TCP', '-h', '127.0.0.1', '-B', '-e', full],
    { encoding: 'utf8', timeout: MYSQL_TIMEOUT_MS }
  );
  return mysqlResult(r, sql, r.error?.code === 'ETIMEDOUT');
}

function dockerMysqlUp() {
  if (portInUse(MYSQL_PORT)) {
    console.log(`SKIP oracle: port ${MYSQL_PORT} in use`);
    return null;
  }
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

function headerLine(out) {
  if (!out) return '';
  return out.split('\n')[0] ?? '';
}

function classify(rusql, mysql) {
  const unsupported = /unsupported/i.test(rusql.err);
  const parseErr = /1064|SQL syntax/i.test(rusql.err);
  if (!mysql) {
    if (rusql.ok) return 'ok';
    if (unsupported) return 'unsupported';
    if (parseErr) return 'parse_error';
    return 'error';
  }
  if (rusql.ok && mysql.ok) {
    const rh = headerLine(rusql.out);
    const mh = headerLine(mysql.out);
    if (rh && mh && rh !== mh) return 'shape_mismatch';
    return 'ok';
  }
  if (!rusql.ok && !mysql.ok) return 'both_fail';
  if (rusql.ok && !mysql.ok) return 'mysql_rejected';
  if (unsupported) return 'unsupported';
  if (parseErr) return 'parse_error';
  return 'wrong_errno';
}

function runOnRusql(sql, useDockerClient) {
  return useDockerClient ? mysqlRusqlDockerOnDb(null, sql) : mysqlLocalOnDb(rusqlPort, null, sql);
}

console.log('mysql-gap-probe: rusql inventory (not a CI gate)');

if (!existsSync(fixturePath)) {
  console.log('SKIP: mysql-gap-probe.json missing');
  process.exit(0);
}

if (!hasCmd('docker') && !hasLocalMysql()) {
  console.log('SKIP: docker and mysql client unavailable');
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

const data = JSON.parse(readFileSync(fixturePath, 'utf8'));
let rusqlChild = null;
let dataDir = null;
let container = null;

try {
  dataDir = mkdtempSync(join(tmpdir(), 'rusql-gap-probe-'));
  rusqlPort = await pickFreePort();
  rusqlChild = startRusql(dataDir, rusqlPort);
  await waitForPort(rusqlPort);

  container = dockerMysqlUp();
  let mysqlDb = null;
  if (container) {
    mysqlExec(container, 'DROP DATABASE IF EXISTS gap_probe');
    const created = mysqlExec(container, 'CREATE DATABASE gap_probe');
    mysqlDb = created.ok ? 'gap_probe' : null;
  }

  const rows = [];
  for (const probe of data.probes ?? []) {
    let setupOk = true;
    for (const sql of probe.setup ?? []) {
      const r = runOnRusql(sql, useDockerMysqlClient);
      if (container && mysqlDb) mysqlExec(container, sql, mysqlDb);
      if (!r.ok) {
        rows.push({
          id: probe.id,
          sql: probe.sql,
          class: 'setup_failed',
          rusql_ok: false,
          mysql_ok: null,
          err: r.err,
        });
        setupOk = false;
        break;
      }
    }
    if (!setupOk) continue;

    const rusql = runOnRusql(probe.sql, useDockerMysqlClient);
    const mysql = container && mysqlDb ? mysqlExec(container, probe.sql, mysqlDb) : null;
    const klass = classify(rusql, mysql);
    rows.push({
      id: probe.id,
      sql: probe.sql,
      class: klass,
      rusql_ok: rusql.ok,
      mysql_ok: mysql ? mysql.ok : null,
      err: rusql.ok ? '' : rusql.err.slice(0, 240),
      mysql_err: mysql && !mysql.ok ? mysql.err.slice(0, 160) : '',
      rusql_header: headerLine(rusql.out),
      mysql_header: mysql ? headerLine(mysql.out) : '',
    });
    const mark = klass === 'ok' || klass === 'both_fail' ? 'OK' : 'GAP';
    console.log(`${mark}: ${probe.id} [${klass}]${rusql.ok ? '' : ` ${rusql.err.split('\n')[0]}`}`);
  }

  const gaps = rows.filter((r) => !['ok', 'both_fail', 'mysql_rejected', 'setup_failed'].includes(r.class));
  console.log(`\nprobed ${rows.length}; gaps ${gaps.length}`);
  if (gaps.length > 0) {
    console.log(JSON.stringify(gaps.map((g) => ({ id: g.id, class: g.class, err: g.err })), null, 2));
  }
} catch (e) {
  console.log(`SKIP: ${e.message}`);
} finally {
  stopProc(rusqlChild);
  rusqlChild = null;
  try {
    if (rusqlPort) await waitForPortFree(rusqlPort);
  } catch {
  }
  if (dataDir) {
    try {
      rmSync(dataDir, { recursive: true, force: true });
    } catch {
    }
  }
  dockerStop(container);
}

process.exit(0);
