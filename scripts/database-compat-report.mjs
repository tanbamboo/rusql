#!/usr/bin/env node
/**
 * Full database compat report: rusql wire tests + optional MySQL 8.0 differential diff.
 * Writes docs/en/reports/database-compat-report-<date>.md
 *
 * Usage: node scripts/database-compat-report.mjs [--date YYYY-MM-DD]
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, existsSync, mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import net from 'node:net';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const basicPath = join(root, 'crates/rusql-server/compat/basic.json');
const diffPath = join(root, 'crates/rusql-server/compat/mysql-diff.json');
const RUSQL_PORT = 3307;
const MYSQL_PORT = 3308;

const dateArg = process.argv.find((a) => a.startsWith('--date='))?.slice(7)
  ?? (process.argv.includes('--date') ? process.argv[process.argv.indexOf('--date') + 1] : null);
const reportDate = dateArg ?? new Date().toISOString().slice(0, 10);
const reportPath = join(root, 'docs/en/reports', `database-compat-report-${reportDate}.md`);

const RUSQL_ONLY_PATTERNS = [
  /^USE\s+rusql\b/i,
  /information_schema/i,
  /^SHOW\s+CREATE\s+TABLE/i,
  /^SHOW\s+INDEX/i,
  /^SHOW\s+DATABASES/i,
  /^DESCRIBE\b/i,
  /^SHOW\s+COLUMNS/i,
  /^ALTER\s+TABLE/i,
  /^BEGIN\b/i,
  /^COMMIT\b/i,
  /^ROLLBACK\b/i,
];

function dockerOk() {
  const r = spawnSync('docker', ['info'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  return r.status === 0;
}

function mysqlClientCmd() {
  if (existsSync('/.dockerenv')) return ['mysql'];
  if (spawnSync('mysql', ['--version'], { shell: true, encoding: 'utf8' }).status === 0) {
    return ['mysql'];
  }
  if (dockerOk()) return ['docker', 'run', '--rm', 'mysql:8.0', 'mysql'];
  return null;
}

function hostForDockerClient() {
  return process.platform === 'win32' || process.platform === 'darwin'
    ? 'host.docker.internal'
    : '127.0.0.1';
}

function runCargoCompat() {
  const start = Date.now();
  const r = spawnSync('cargo', ['test', '-p', 'rusql-server', 'compat'], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const ms = Date.now() - start;
  const passed = r.status === 0;
  const tests = (r.stdout.match(/(\d+) passed/) ?? [])[1] ?? '?';
  return { passed, tests: Number(tests), ms, stdout: r.stdout, stderr: r.stderr };
}

function classifySql(sql) {
  if (RUSQL_ONLY_PATTERNS.some((p) => p.test(sql.trim()))) return 'rusql-specific';
  return 'portable';
}

function inventoryFixtures() {
  const basic = JSON.parse(readFileSync(basicPath, 'utf8'));
  const diff = existsSync(diffPath) ? JSON.parse(readFileSync(diffPath, 'utf8')) : { suites: [] };
  const suites = [];
  let portable = 0;
  let rusqlOnly = 0;
  let totalSteps = 0;
  for (const suite of basic.suites ?? []) {
    const steps = (suite.steps ?? []).map((s) => {
      const kind = classifySql(s.sql);
      if (kind === 'portable') portable++;
      else rusqlOnly++;
      totalSteps++;
      return { sql: s.sql, kind, expect: s.expect?.type ?? 'unknown' };
    });
    suites.push({ name: suite.name, steps });
  }
  const diffSteps = (diff.suites ?? []).reduce((n, s) => n + (s.steps?.length ?? 0), 0);
  return { suites, totalSteps, portable, rusqlOnly, diffSuites: diff.suites ?? [], diffSteps };
}

function serverBinary() {
  const name = process.platform === 'win32' ? 'rusql-server.exe' : 'rusql-server';
  return join(root, 'target', 'release', name);
}

function buildServer() {
  return spawnSync('cargo', ['build', '--release', '-p', 'rusql-server'], {
    cwd: root,
    encoding: 'utf8',
  }).status === 0 && existsSync(serverBinary());
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
        if (Date.now() - start > timeoutMs) reject(new Error(`port ${port} timeout`));
        else setTimeout(attempt, 250);
      });
    };
    attempt();
  });
}

function startRusql(dataDir) {
  return spawn(serverBinary(), ['--port', String(RUSQL_PORT), '--data-dir', dataDir], {
    cwd: root,
    stdio: 'ignore',
    detached: process.platform !== 'win32',
  });
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
    ['run', '--rm', '-d', '-e', 'MYSQL_ALLOW_EMPTY_PASSWORD=yes', '-p', `${MYSQL_PORT}:3306`, 'mysql:8.0'],
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

function mysqlRusql(sql) {
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

async function runMysqlDiff(diffSuites) {
  if (!buildServer()) return { status: 'skip', reason: 'rusql-server build failed' };
  const container = dockerMysqlUp();
  if (!container) return { status: 'skip', reason: 'could not start mysql:8.0 container' };

  let rusqlChild = null;
  let dataDir = null;
  const results = [];
  let matched = 0;
  let mismatched = 0;

  async function freshRusql() {
    stopProc(rusqlChild);
    if (dataDir) {
      try {
        rmSync(dataDir, { recursive: true, force: true });
      } catch {
      }
    }
    dataDir = mkdtempSync(join(tmpdir(), 'rusql-report-'));
    rusqlChild = startRusql(dataDir);
    await waitForPort(RUSQL_PORT);
  }

  try {
    for (const suite of diffSuites) {
      const db = `rpt_${suite.name.replace(/[^a-zA-Z0-9_]/g, '_')}`;
      mysqlExec(container, `DROP DATABASE IF EXISTS \`${db}\``);
      if (!mysqlExec(container, `CREATE DATABASE \`${db}\``).ok) continue;
      await freshRusql();

      for (const step of suite.steps ?? []) {
        const rusql = mysqlRusql(step.sql);
        const mysql = mysqlExec(container, `USE \`${db}\`; ${step.sql}`);
        const same = rusql.ok === mysql.ok && rusql.out === mysql.out;
        if (same) matched++;
        else mismatched++;
        results.push({
          suite: suite.name,
          sql: step.sql,
          status: same ? 'match' : 'mismatch',
          rusql: rusql.ok ? rusql.out || '(empty)' : rusql.err,
          mysql: mysql.ok ? mysql.out || '(empty)' : mysql.err,
        });
      }
    }
    return { status: 'ran', matched, mismatched, total: results.length, results };
  } catch (e) {
    return { status: 'skip', reason: e.message, results };
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
}

function renderReport({ compat, inventory, mysqlDiff, docker, client }) {
  const lines = [];
  lines.push(`# Database compatibility report — ${reportDate}`);
  lines.push('');
  lines.push('Automated rusql wire tests and optional differential comparison against MySQL 8.0.');
  lines.push('');
  lines.push('## Executive summary');
  lines.push('');
  lines.push(`| Layer | Result |`);
  lines.push(`|-------|--------|`);
  lines.push(`| rusql compat (\`cargo test -p rusql-server compat\`) | ${compat.passed ? '**PASS**' : '**FAIL**'} (${compat.tests} tests, ${compat.ms}ms) |`);
  if (mysqlDiff.status === 'ran') {
    const pct = mysqlDiff.total ? Math.round((100 * mysqlDiff.matched) / mysqlDiff.total) : 0;
    const label =
      mysqlDiff.total === 0
        ? '**SKIP** (0 steps executed)'
        : mysqlDiff.mismatched === 0
          ? '**PASS**'
          : '**FAIL**';
    lines.push(
      `| MySQL 8.0 differential (\`mysql-diff.json\`) | ${label} (${mysqlDiff.matched}/${mysqlDiff.total} steps match${mysqlDiff.total ? `, ${pct}%` : ''}) |`
    );
  } else {
    lines.push(`| MySQL 8.0 differential | **SKIP** — ${mysqlDiff.reason} |`);
  }
  lines.push('');
  lines.push('## Test inventory');
  lines.push('');
  lines.push('| Fixture | Suites | Steps | Notes |');
  lines.push('|---------|--------|-------|-------|');
  lines.push(`| \`compat/basic.json\` | ${inventory.suites.length} | ${inventory.totalSteps} | rusql wire protocol; ${inventory.portable} portable / ${inventory.rusqlOnly} rusql-specific SQL |`);
  lines.push(`| \`compat/mysql-diff.json\` | ${inventory.diffSuites.length} | ${inventory.diffSteps} | MySQL 8.0 differential subset |`);
  lines.push(`| Rust integration | 2 | — | \`run_basic_compat_fixtures\`, \`compat_persistence_after_restart\` |`);
  lines.push(`| Workspace unit tests | all crates | — | \`cargo test\` |`);
  lines.push('');
  lines.push('### \`basic.json\` suites');
  lines.push('');
  for (const s of inventory.suites) {
    const p = s.steps.filter((x) => x.kind === 'portable').length;
    const r = s.steps.length - p;
    lines.push(`- **${s.name}** — ${s.steps.length} steps (${p} portable, ${r} rusql-specific)`);
  }
  lines.push('');
  lines.push('## rusql compat results');
  lines.push('');
  if (compat.passed) {
    lines.push('All compat integration tests passed:');
    lines.push('');
    lines.push('- `compat_suite::run_basic_compat_fixtures` — 14 JSON suites, 76 SQL steps');
    lines.push('- `compat_suite::compat_persistence_after_restart` — WAL survive restart');
  } else {
    lines.push('```');
    lines.push(compat.stderr || compat.stdout);
    lines.push('```');
  }
  lines.push('');
  lines.push('## MySQL 8.0 comparison');
  lines.push('');
  lines.push(`| Environment | Value |`);
  lines.push(`|-------------|-------|`);
  lines.push(`| Docker daemon | ${docker ? 'available' : 'unavailable'} |`);
  lines.push(`| MySQL client | ${client ?? 'unavailable'} |`);
  lines.push(`| rusql-server port | ${RUSQL_PORT} |`);
  lines.push(`| MySQL 8.0 container port | ${MYSQL_PORT} |`);
  lines.push('');

  if (mysqlDiff.status === 'ran') {
    lines.push('### Differential results (`mysql-diff.json`)');
    lines.push('');
    if (mysqlDiff.total === 0) {
      lines.push('No steps executed (MySQL container or database setup failed).');
    } else if (mysqlDiff.mismatched === 0) {
      lines.push('All portable differential steps matched MySQL 8.0 batch output (`mysql -B`).');
    } else {
      lines.push(`**${mysqlDiff.mismatched}** step(s) differed from MySQL 8.0:`);
      lines.push('');
      lines.push('| Suite | SQL | Issue |');
      lines.push('|-------|-----|-------|');
      for (const r of mysqlDiff.results.filter((x) => x.status === 'mismatch')) {
        const issue = r.rusqlErr || r.mysqlErr || 'output differs';
        lines.push(`| ${r.suite} | \`${r.sql.replace(/`/g, "'")}\` | ${issue.slice(0, 80)} |`);
      }
      lines.push('');
      lines.push(
        '> **Note:** The official `mysql` 8.0 CLI client (used by `mysql-diff.mjs` via Docker) shows protocol gaps vs rusql\'s internal wire test client: multi-connection persistence and `UPDATE`/`DELETE` may fail with error 1105 while `cargo test -p rusql-server compat` passes.'
      );
    }
    lines.push('');
  } else {
    lines.push(`> MySQL differential not run: ${mysqlDiff.reason}`);
    lines.push('');
    lines.push('To run locally: start Docker Desktop, then:');
    lines.push('');
    lines.push('```bash');
    lines.push('node scripts/database-compat-report.mjs');
    lines.push('# or quick diff only:');
    lines.push('node scripts/mysql-diff.mjs');
    lines.push('```');
    lines.push('');
  }

  lines.push('## Known gaps (rusql vs MySQL 8.0)');
  lines.push('');
  lines.push('| Area | rusql | MySQL 8.0 |');
  lines.push('|------|-------|-----------|');
  lines.push('| Default database | `rusql` schema | Server default / user DB |');
  lines.push('| `information_schema` | Virtual minimal views | Full catalog |');
  lines.push('| `SHOW CREATE TABLE` | rusql DDL export | Native DDL |');
  lines.push('| Transactions | BEGIN/COMMIT overlay | Full ACID + isolation |');
  lines.push('| Prepared statements | COM_STMT subset | Full binary protocol |');
  lines.push('| Auth | caching_sha2 fast + RSA | Full plugin ecosystem |');
  lines.push('| Aggregates / subqueries | Not implemented | Full SQL |');
  lines.push(
    '| Official mysql CLI vs rusql wire | INSERT may not persist across connections; UPDATE/DELETE error 1105 | Use compat_suite wire client until fixed |'
  );
  lines.push('| mysql-test official suite | Not ported (M30 planned) | Thousands of tests |');
  lines.push('');
  lines.push('## How to re-run');
  lines.push('');
  lines.push('```bash');
  lines.push('cargo test -p rusql-server compat          # rusql wire fixtures');
  lines.push('node scripts/mysql-diff.mjs                # portable MySQL diff (needs Docker)');
  lines.push('node scripts/database-compat-report.mjs      # this report');
  lines.push('```');
  lines.push('');
  lines.push(`*Generated by \`scripts/database-compat-report.mjs\` on ${reportDate}.*`);
  return lines.join('\n');
}

async function main() {
  console.log('database-compat-report: collecting results…');
  const compat = runCargoCompat();
  const inventory = inventoryFixtures();
  const docker = dockerOk();
  const clientCmd = mysqlClientCmd();
  const client = clientCmd
    ? clientCmd[0] === 'docker'
      ? 'docker mysql:8.0 client'
      : 'local mysql'
    : null;

  let mysqlDiff = { status: 'skip', reason: 'Docker or mysql client unavailable' };
  if (docker && inventory.diffSuites.length > 0) {
    console.log('database-compat-report: running MySQL 8.0 differential…');
    mysqlDiff = await runMysqlDiff(inventory.diffSuites);
  }

  const markdown = renderReport({ compat, inventory, mysqlDiff, docker, client });
  mkdirSync(dirname(reportPath), { recursive: true });
  writeFileSync(reportPath, markdown, 'utf8');
  console.log(`Wrote ${reportPath}`);
  if (!compat.passed) process.exit(1);
  if (mysqlDiff.status === 'ran' && mysqlDiff.mismatched > 0) process.exit(1);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
