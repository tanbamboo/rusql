#!/usr/bin/env node
/**
 * Persistent-connection micro-benchmark: rusql vs MySQL 8.0 (PERF-B1).
 *
 * Same 7 workloads as docs/en/reports/performance-benchmark-2026-08-11.md
 * without per-query CLI spawn overhead.
 *
 * Usage:
 *   node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql
 *   node scripts/bench-rusql-vs-mysql.mjs --compare --rusql-port 3307 --mysql-port 3308
 *   node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --output target/bench-rusql.json
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { WireBenchClient } from './wire-bench-client.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

const WARMUP = 20;
const WORKLOADS = [
  { name: 'select1', iterations: 1000, sql: () => 'SELECT 1' },
  {
    name: 'point_select_pk',
    iterations: 1000,
    sql: () => 'SELECT name FROM bench_t WHERE id = 5000',
  },
  {
    name: 'index_lookup',
    iterations: 1000,
    sql: () => 'SELECT id FROM bench_t WHERE k = 42',
  },
  {
    name: 'scan_order_limit',
    iterations: 1000,
    sql: () => 'SELECT id FROM bench_t ORDER BY k LIMIT 100',
  },
  {
    name: 'insert_single',
    iterations: 500,
    setup: async (conn, state) => {
      state.insertId = 20_000;
    },
    sql: (state) => {
      const id = state.insertId++;
      return `INSERT INTO bench_t (id, k, name) VALUES (${id}, ${id % 100}, 'ins${id}')`;
    },
  },
  {
    name: 'update_pk',
    iterations: 500,
    sql: () => "UPDATE bench_t SET name = 'u' WHERE id = 5000",
  },
  {
    name: 'begin_commit',
    iterations: 200,
    setup: async (conn, state) => {
      state.txnId = 30_000;
    },
    txn: async (conn, state) => {
      const id = state.txnId++;
      await conn.query('BEGIN');
      await conn.query(
        `INSERT INTO bench_t (id, k, name) VALUES (${id}, ${id % 100}, 'tx${id}')`
      );
      await conn.query('COMMIT');
    },
  },
];

function parseArgs(argv) {
  const opts = {
    host: '127.0.0.1',
    port: 3307,
    user: 'root',
    password: '',
    database: 'bench',
    label: 'server',
    output: '',
    compare: false,
    rusqlPort: 3307,
    mysqlPort: 3308,
    setupOnly: false,
  };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--compare') opts.compare = true;
    else if (a === '--setup-only') opts.setupOnly = true;
    else if (a === '--host') opts.host = argv[++i];
    else if (a === '--port') opts.port = Number(argv[++i]);
    else if (a === '--rusql-port') opts.rusqlPort = Number(argv[++i]);
    else if (a === '--mysql-port') opts.mysqlPort = Number(argv[++i]);
    else if (a === '--user') opts.user = argv[++i];
    else if (a === '--password') opts.password = argv[++i];
    else if (a === '--database') opts.database = argv[++i];
    else if (a === '--label') opts.label = argv[++i];
    else if (a === '--output') opts.output = argv[++i];
    else if (a === '--help' || a === '-h') {
      console.log(`Usage: node scripts/bench-rusql-vs-mysql.mjs [options]
  --host HOST          default 127.0.0.1
  --port PORT          default 3307
  --label NAME         engine label in JSON (default server)
  --output FILE        write JSON report (default stdout)
  --compare            run rusql + mysql and print ratio table
  --rusql-port PORT    with --compare (default 3307)
  --mysql-port PORT    with --compare (default 3308)
  --setup-only         create schema + seed only`);
      process.exit(0);
    }
  }
  return opts;
}

function percentile(sorted, p) {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
  return sorted[idx];
}

async function connect(opts) {
  return WireBenchClient.connect({
    host: opts.host,
    port: opts.port,
    user: opts.user,
    password: opts.password,
  });
}

async function ensureSchema(conn, database) {
  await conn.query(`CREATE DATABASE IF NOT EXISTS \`${database}\``);
  await conn.query(`USE \`${database}\``);
  await conn.query('DROP TABLE IF EXISTS bench_t');
  await conn.query(`
    CREATE TABLE bench_t (
      id INT NOT NULL PRIMARY KEY,
      k INT NOT NULL,
      name VARCHAR(32) NOT NULL
    )
  `);
  await conn.query('CREATE INDEX idx_bench_k ON bench_t (k)');
  const batch = [];
  for (let id = 1; id <= 10_000; id++) {
    batch.push(`(${id}, ${id % 100}, 'n${id}')`);
    if (batch.length >= 500) {
      await conn.query(`INSERT INTO bench_t (id, k, name) VALUES ${batch.join(',')}`);
      batch.length = 0;
    }
  }
  if (batch.length) {
    await conn.query(`INSERT INTO bench_t (id, k, name) VALUES ${batch.join(',')}`);
  }
}

async function runWorkload(conn, workload) {
  const state = {};
  if (workload.setup) {
    await workload.setup(conn, state);
  }
  for (let i = 0; i < WARMUP; i++) {
    if (workload.txn) {
      await workload.txn(conn, state);
    } else {
      await conn.query(workload.sql(state));
    }
  }
  const latencies = [];
  const start = performance.now();
  for (let i = 0; i < workload.iterations; i++) {
    const t0 = performance.now();
    if (workload.txn) {
      await workload.txn(conn, state);
    } else {
      await conn.query(workload.sql(state));
    }
    latencies.push(performance.now() - t0);
  }
  const elapsedSec = (performance.now() - start) / 1000;
  latencies.sort((a, b) => a - b);
  const qps = workload.iterations / elapsedSec;
  const avgMs = latencies.reduce((s, v) => s + v, 0) / latencies.length;
  return {
    name: workload.name,
    iterations: workload.iterations,
    qps: Number(qps.toFixed(2)),
    avg_ms: Number(avgMs.toFixed(2)),
    p50_ms: Number(percentile(latencies, 50).toFixed(2)),
    p95_ms: Number(percentile(latencies, 95).toFixed(2)),
  };
}

async function benchOne(opts) {
  const conn = await connect(opts);
  try {
    await ensureSchema(conn, opts.database);
    if (opts.setupOnly) {
      console.log(`Schema ready on ${opts.host}:${opts.port}/${opts.database}`);
      return null;
    }
    const results = [];
    for (const w of WORKLOADS) {
      process.stderr.write(`  ${opts.label}: ${w.name}…\n`);
      results.push(await runWorkload(conn, w));
    }
    return {
      meta: {
        date: new Date().toISOString().slice(0, 10),
        engine: opts.label,
        host: opts.host,
        port: opts.port,
        database: opts.database,
        hostname: os.hostname(),
        platform: `${process.platform} ${os.release()}`,
        node: process.version,
        connection: 'persistent (wire client)',
        warmup: WARMUP,
        row_count: 10_000,
      },
      workloads: results,
    };
  } finally {
    await conn.end();
  }
}

function writeReport(report, outputPath) {
  const json = JSON.stringify(report, null, 2);
  if (outputPath) {
    mkdirSync(dirname(outputPath), { recursive: true });
    writeFileSync(outputPath, json + '\n', 'utf8');
    console.log(`Wrote ${outputPath}`);
  } else {
    console.log(json);
  }
}

function ratioTable(rusql, mysql) {
  console.log('\n| Workload | rusql QPS | MySQL QPS | rusql/MySQL |');
  console.log('|----------|-----------|-----------|-------------|');
  for (const rw of rusql.workloads) {
    const mw = mysql.workloads.find((w) => w.name === rw.name);
    if (!mw) continue;
    const ratio = (rw.qps / mw.qps).toFixed(2);
    console.log(`| ${rw.name} | ${rw.qps} | ${mw.qps} | ${ratio}× |`);
  }
}

async function main() {
  const opts = parseArgs(process.argv);
  if (opts.compare) {
    console.log('Running persistent-connection benchmark (compare mode)…');
    const rusql = await benchOne({
      ...opts,
      port: opts.rusqlPort,
      label: 'rusql',
    });
    const mysql = await benchOne({
      ...opts,
      port: opts.mysqlPort,
      label: 'mysql',
    });
    const report = { rusql, mysql };
    const out = opts.output || join(root, 'docs/en/reports/performance-benchmark-persistent.json');
    writeReport(report, out);
    ratioTable(rusql, mysql);
    return;
  }
  console.log(`Benchmark ${opts.label} @ ${opts.host}:${opts.port}…`);
  const report = await benchOne(opts);
  if (report) {
    writeReport(report, opts.output);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
