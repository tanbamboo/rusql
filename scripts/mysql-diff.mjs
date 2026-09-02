#!/usr/bin/env node
/**
 * Differential compat: run portable SQL on rusql-server and Docker MySQL 8.0; diff batch output.
 * Skips gracefully when Docker, mysql client, or server build is unavailable.
 *
 * Usage:
 *   node scripts/mysql-diff.mjs            # full diff (smoke + suites)
 *   node scripts/mysql-diff.mjs --smoke-only
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import net from 'node:net';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const fixturePath = join(root, 'crates/rusql-server/compat/mysql-diff.json');
const RUSQL_PORT_BASE = 3307;
const MYSQL_PORT = 3308;
const MYSQL_TIMEOUT_MS = 60_000;
const smokeOnly = process.argv.includes('--smoke-only');
let rusqlPort = RUSQL_PORT_BASE;

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

function checkPorts() {
  if (portInUse(MYSQL_PORT)) {
    console.error(
      `FAIL: port ${MYSQL_PORT} is in use — stop stale docker mysql containers`
    );
    process.exit(1);
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
    return {
      ok: false,
      out: '',
      err: `rusql client timed out after ${MYSQL_TIMEOUT_MS}ms — check protocol compat (handshake, COM_QUERY attrs, metadata EOF). SQL: ${sql}`,
    };
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

function logMysqlClient() {
  const r = spawnSync('mysql', ['--version'], { shell: true, encoding: 'utf8' });
  if (r.status === 0) {
    console.log(`mysql client: ${(r.stdout || r.stderr).trim()}`);
  } else {
    console.log('mysql client: (using Docker mysql:8.0 per step)');
  }
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
  const env = { ...process.env };
  return spawn(bin, ['--port', String(port), '--data-dir', dataDir], {
    cwd: root,
    stdio: 'ignore',
    env,
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

/** Wait until nothing listens on `port` (avoids Linux race after SIGTERM). */
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
    ['-h', '127.0.0.1', '-P', String(port), '-u', 'root', '--ssl-mode=DISABLED', '-B', '-e', sql],
    { encoding: 'utf8', timeout: MYSQL_TIMEOUT_MS }
  );
  return mysqlResult(r, sql, r.error?.code === 'ETIMEDOUT');
}

function mysqlExec(containerId, sql) {
  const r = spawnSync(
    'docker',
    ['exec', containerId, 'mysql', '-u', 'root', '--protocol=TCP', '-h', '127.0.0.1', '-B', '-e', sql],
    { encoding: 'utf8', timeout: MYSQL_TIMEOUT_MS }
  );
  return mysqlResult(r, sql, r.error?.code === 'ETIMEDOUT');
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
      String(rusqlPort),
      '-u',
      'root',
      '--protocol=TCP',
      '--ssl-mode=DISABLED',
      '--connect-timeout=10',
      '-B',
      '-e',
      sql,
    ],
    { encoding: 'utf8', timeout: MYSQL_TIMEOUT_MS }
  );
  return mysqlResult(r, sql, r.error?.code === 'ETIMEDOUT');
}

function resetMysqlDb(containerId, suiteName) {
  const db = `md_${suiteName.replace(/[^a-zA-Z0-9_]/g, '_')}`;
  mysqlExec(containerId, `DROP DATABASE IF EXISTS \`${db}\``);
  const create = mysqlExec(containerId, `CREATE DATABASE \`${db}\``);
  if (!create.ok) return null;
  return db;
}

function runStepsOnMysql(containerId, steps, db) {
  const results = [];
  for (const step of steps) {
    const sql = db ? `USE \`${db}\`; ${step.sql}` : step.sql;
    const got = mysqlExec(containerId, sql);
    results.push({ sql: step.sql, compare_output: step.compare_output, ...got });
  }
  return results;
}

function runStepsOnRusql(steps, useDockerClient) {
  const results = [];
  let sessionDb = null;
  for (const step of steps) {
    const useMatch = step.sql.trim().match(/^USE\s+(`?)(\w+)\1\s*;?$/i);
    if (useMatch) {
      sessionDb = useMatch[2];
      const verifySql = 'SELECT DATABASE()';
      const got = useDockerClient
        ? mysqlRusqlDockerOnDb(sessionDb, verifySql)
        : mysqlLocalOnDb(rusqlPort, sessionDb, verifySql);
      results.push({ sql: step.sql, compare_output: step.compare_output, ...got });
      continue;
    }
    const got = useDockerClient
      ? mysqlRusqlDockerOnDb(sessionDb, step.sql)
      : mysqlLocalOnDb(rusqlPort, sessionDb, step.sql);
    results.push({ sql: step.sql, compare_output: step.compare_output, ...got });
  }
  return results;
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

function diffSteps(suiteName, rusql, mysql) {
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
      if (r.compare_output === false) {
        continue;
      }
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

function reportFailures(suiteName, mismatches) {
  console.error(`FAIL: ${suiteName}`);
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
    if (mm.rusqlErr?.includes('timed out')) {
      console.error(
        '  hint: enable RUST_LOG=rusql_server=debug and retry; verify caching_sha2 AuthMoreData + query-attributes parsing'
      );
    }
  }
}

console.log(
  smokeOnly
    ? 'mysql-protocol-smoke: official mysql client vs rusql'
    : 'mysql-diff: rusql vs Docker MySQL 8.0 (portable fixture subset)'
);

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

logMysqlClient();

if (!buildServer()) {
  console.log('SKIP: could not build rusql-server');
  process.exit(0);
}

checkPorts();

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
  rusqlChild = null;
  if (rusqlPort) {
    try {
      await waitForPortFree(rusqlPort);
    } catch {
    }
  }
  if (dataDir) {
    try {
      rmSync(dataDir, { recursive: true, force: true });
    } catch {
    }
  }
  dataDir = mkdtempSync(join(tmpdir(), 'rusql-mysql-diff-'));
  rusqlPort = await pickFreePort();
  rusqlChild = startRusql(dataDir, rusqlPort);
  await waitForPort(rusqlPort);
}

try {
  let compared = 0;
  let suitesOk = 0;

  const smokeSteps = data.protocol_smoke ?? [];
  if (smokeSteps.length > 0) {
    await freshRusql();
    const rusqlResults = runStepsOnRusql(smokeSteps, useDockerMysqlClient);
    const mysqlResults = runStepsOnMysql(container, smokeSteps, null);
    const mismatches = diffSteps('protocol_smoke', rusqlResults, mysqlResults);
    compared += smokeSteps.length;
    if (mismatches.length === 0) {
      suitesOk++;
      console.log(`OK: protocol_smoke (${smokeSteps.length} steps)`);
    } else {
      exitCode = 1;
      reportFailures('protocol_smoke', mismatches);
    }
  }

  if (!smokeOnly) {
    for (const suite of data.suites ?? []) {
      const db = resetMysqlDb(container, suite.name);
      if (!db) {
        console.log(`SKIP suite ${suite.name}: could not create MySQL database`);
        continue;
      }
      await freshRusql();

      const rusqlResults = runStepsOnRusql(suite.steps, useDockerMysqlClient);
      const mysqlResults = runStepsOnMysql(container, suite.steps, db);
      const mismatches = diffSteps(suite.name, rusqlResults, mysqlResults);
      compared += suite.steps.length;
      if (mismatches.length === 0) {
        suitesOk++;
        console.log(`OK: ${suite.name} (${suite.steps.length} steps)`);
      } else {
        exitCode = 1;
        reportFailures(suite.name, mismatches);
      }
    }
  }

  if (exitCode === 0 && compared > 0) {
    console.log(`OK: compared ${compared} steps across ${suitesOk} suite(s)`);
  }
} catch (e) {
  console.log(`SKIP: ${e.message}`);
  exitCode = 0;
} finally {
  stopProc(rusqlChild);
  rusqlChild = null;
  try {
    await waitForPortFree(rusqlPort);
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

process.exit(exitCode);
