#!/usr/bin/env node
/**
 * Persistent-connection benchmark: rusql vs MySQL 8.0 (PERF-B1/B4/B5).
 *
 * Supports single-thread iteration mode and multi-client concurrency
 * (--threads, --duration, --thread-matrix) with read/write mix summaries.
 *
 * Usage:
 *   node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql
 *   node scripts/bench-rusql-vs-mysql.mjs --compare --rusql-port 3307 --mysql-port 3308
 *   node scripts/bench-rusql-vs-mysql.mjs --threads 8 --duration 30 --workloads read-heavy
 *   node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { WireBenchClient } from './wire-bench-client.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

const WARMUP = 20;
const THREAD_MATRIX = [1, 4, 8, 16];

const WORKLOAD_DEFS = {
  select1: {
    name: 'select1',
    mix: 'read',
    sql: () => 'SELECT 1',
  },
  point_select_pk: {
    name: 'point_select_pk',
    mix: 'read',
    sql: () => 'SELECT name FROM bench_t WHERE id = 5000',
  },
  index_lookup: {
    name: 'index_lookup',
    mix: 'read',
    sql: () => 'SELECT id FROM bench_t WHERE k = 42',
  },
  scan_order_limit: {
    name: 'scan_order_limit',
    mix: 'read',
    sql: () => 'SELECT id FROM bench_t ORDER BY k LIMIT 100',
  },
  insert_single: {
    name: 'insert_single',
    mix: 'write',
    setup: async (_conn, state) => {
      state.insertId = 20_000;
    },
    sql: (state) => {
      const id = state.insertId++;
      return `INSERT INTO bench_t (id, k, name) VALUES (${id}, ${id % 100}, 'ins${id}')`;
    },
  },
  update_pk: {
    name: 'update_pk',
    mix: 'write',
    sql: () => "UPDATE bench_t SET name = 'u' WHERE id = 5000",
  },
  begin_commit: {
    name: 'begin_commit',
    mix: 'write',
    setup: async (_conn, state) => {
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
};

const MIX_WORKLOADS = {
  all: Object.keys(WORKLOAD_DEFS),
  'read-heavy': ['select1', 'point_select_pk', 'index_lookup', 'scan_order_limit'],
  'write-heavy': ['insert_single', 'update_pk', 'begin_commit'],
};

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
    threads: 1,
    duration: 0,
    threadMatrix: false,
    workloads: 'all',
    iterations: 0,
  };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--compare') opts.compare = true;
    else if (a === '--setup-only') opts.setupOnly = true;
    else if (a === '--thread-matrix') opts.threadMatrix = true;
    else if (a === '--host') opts.host = argv[++i];
    else if (a === '--port') opts.port = Number(argv[++i]);
    else if (a === '--rusql-port') opts.rusqlPort = Number(argv[++i]);
    else if (a === '--mysql-port') opts.mysqlPort = Number(argv[++i]);
    else if (a === '--user') opts.user = argv[++i];
    else if (a === '--password') opts.password = argv[++i];
    else if (a === '--database') opts.database = argv[++i];
    else if (a === '--label') opts.label = argv[++i];
    else if (a === '--output') opts.output = argv[++i];
    else if (a === '--threads') opts.threads = Number(argv[++i]);
    else if (a === '--duration') opts.duration = Number(argv[++i]);
    else if (a === '--workloads') opts.workloads = argv[++i];
    else if (a === '--iterations') opts.iterations = Number(argv[++i]);
    else if (a === '--help' || a === '-h') {
      console.log(`Usage: node scripts/bench-rusql-vs-mysql.mjs [options]
  --host HOST           default 127.0.0.1
  --port PORT           default 3307
  --label NAME          engine label in JSON
  --output FILE         write JSON report
  --compare             run rusql + mysql and print ratio table
  --rusql-port PORT     with --compare (default 3307)
  --mysql-port PORT     with --compare (default 3308)
  --threads N           concurrent clients (default 1)
  --duration SEC        time-based run per workload (overrides iterations)
  --thread-matrix       run at 1/4/8/16 threads and report scaling
  --workloads MIX       all | read-heavy | write-heavy | comma-separated names
  --iterations N        per-workload iteration count (default varies by workload)
  --setup-only          create schema + seed only`);
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

function resolveWorkloadNames(spec) {
  if (MIX_WORKLOADS[spec]) return MIX_WORKLOADS[spec];
  return spec.split(',').map((s) => s.trim()).filter(Boolean);
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

async function runClientLoop(conn, workload, durationSec, iterations) {
  const state = {};
  const latencies = [];
  let ops = 0;

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

  const deadline = durationSec > 0 ? performance.now() + durationSec * 1000 : 0;
  const start = performance.now();

  while (true) {
    if (durationSec > 0) {
      if (performance.now() >= deadline) break;
    } else if (ops >= iterations) {
      break;
    }

    const t0 = performance.now();
    if (workload.txn) {
      await workload.txn(conn, state);
    } else {
      await conn.query(workload.sql(state));
    }
    latencies.push(performance.now() - t0);
    ops++;
  }

  const elapsedSec = (performance.now() - start) / 1000;
  return { ops, elapsedSec, latencies };
}

function summarizeThreadResults(threadResults, workloadName, threads) {
  const totalOps = threadResults.reduce((s, r) => s + r.ops, 0);
  const wallSec = Math.max(...threadResults.map((r) => r.elapsedSec));
  const aggregateQps = totalOps / wallSec;
  const allLatencies = threadResults.flatMap((r) => r.latencies).sort((a, b) => a - b);
  const perThread = threadResults.map((r, i) => ({
    thread: i,
    ops: r.ops,
    qps: Number((r.ops / r.elapsedSec).toFixed(2)),
    avg_ms: r.latencies.length
      ? Number((r.latencies.reduce((s, v) => s + v, 0) / r.latencies.length).toFixed(2))
      : 0,
  }));

  return {
    name: workloadName,
    threads,
    total_ops: totalOps,
    wall_sec: Number(wallSec.toFixed(3)),
    aggregate_qps: Number(aggregateQps.toFixed(2)),
    avg_ms: allLatencies.length
      ? Number((allLatencies.reduce((s, v) => s + v, 0) / allLatencies.length).toFixed(2))
      : 0,
    p50_ms: Number(percentile(allLatencies, 50).toFixed(2)),
    p95_ms: Number(percentile(allLatencies, 95).toFixed(2)),
    per_thread: perThread,
  };
}

async function runWorkloadConcurrent(opts, workloadDef, threads) {
  const durationSec = opts.duration;
  const iterations =
    opts.iterations ||
    (durationSec > 0 ? 0 : workloadDef.name === 'begin_commit' ? 200 : workloadDef.mix === 'write' ? 500 : 1000);

  const clients = await Promise.all(
    Array.from({ length: threads }, () => connect(opts))
  );

  try {
    const threadResults = await Promise.all(
      clients.map((conn) => runClientLoop(conn, workloadDef, durationSec, iterations))
    );
    return summarizeThreadResults(threadResults, workloadDef.name, threads);
  } finally {
    await Promise.all(clients.map((c) => c.end()));
  }
}

function mixSummary(workloads) {
  const read = workloads.filter((w) => WORKLOAD_DEFS[w.name]?.mix === 'read');
  const write = workloads.filter((w) => WORKLOAD_DEFS[w.name]?.mix === 'write');
  const sumQps = (list) => list.reduce((s, w) => s + w.aggregate_qps, 0);
  return {
    read_heavy_qps: Number(sumQps(read).toFixed(2)),
    write_heavy_qps: Number(sumQps(write).toFixed(2)),
    read_workloads: read.length,
    write_workloads: write.length,
  };
}

async function benchOne(opts) {
  const setupConn = await connect(opts);
  try {
    await ensureSchema(setupConn, opts.database);
    if (opts.setupOnly) {
      console.log(`Schema ready on ${opts.host}:${opts.port}/${opts.database}`);
      return null;
    }
  } finally {
    await setupConn.end();
  }

  const workloadNames = resolveWorkloadNames(opts.workloads);
  const threads = opts.threads;
  const results = [];

  for (const name of workloadNames) {
    const def = WORKLOAD_DEFS[name];
    if (!def) {
      console.error(`Unknown workload: ${name}`);
      continue;
    }
    process.stderr.write(`  ${opts.label}: ${name} (${threads} threads)…\n`);
    results.push(await runWorkloadConcurrent(opts, def, threads));
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
      threads,
      duration_sec: opts.duration || null,
      workloads: opts.workloads,
      row_count: 10_000,
    },
    mix_summary: mixSummary(results),
    workloads: results,
  };
}

async function benchThreadMatrix(opts) {
  const matrix = [];
  for (const threads of THREAD_MATRIX) {
    process.stderr.write(`\n=== ${opts.label}: ${threads} threads ===\n`);
    const report = await benchOne({ ...opts, threads });
    matrix.push({ threads, ...report });
  }
  return {
    meta: {
      ...matrix[0].meta,
      thread_matrix: THREAD_MATRIX,
    },
    scaling: matrix.map((m) => ({
      threads: m.threads,
      aggregate_qps: m.workloads.reduce((s, w) => s + w.aggregate_qps, 0),
      mix_summary: m.mix_summary,
      workloads: m.workloads,
    })),
  };
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
  console.log('\n| Workload | threads | rusql QPS | MySQL QPS | rusql/MySQL |');
  console.log('|----------|---------|-----------|-----------|-------------|');
  for (const rw of rusql.workloads) {
    const mw = mysql.workloads.find((w) => w.name === rw.name);
    if (!mw) continue;
    const ratio = (rw.aggregate_qps / mw.aggregate_qps).toFixed(2);
    console.log(
      `| ${rw.name} | ${rw.threads} | ${rw.aggregate_qps} | ${mw.aggregate_qps} | ${ratio}× |`
    );
  }
  if (rusql.mix_summary && mysql.mix_summary) {
    console.log('\nMix summary:');
    console.log(
      `  read-heavy: rusql ${rusql.mix_summary.read_heavy_qps} vs mysql ${mysql.mix_summary.read_heavy_qps}`
    );
    console.log(
      `  write-heavy: rusql ${rusql.mix_summary.write_heavy_qps} vs mysql ${mysql.mix_summary.write_heavy_qps}`
    );
  }
}

function scalingTable(rusqlScaling, mysqlScaling) {
  console.log('\n| Threads | rusql total QPS | MySQL total QPS | rusql/MySQL |');
  console.log('|---------|-----------------|-----------------|-------------|');
  for (const rs of rusqlScaling.scaling) {
    const ms = mysqlScaling.scaling.find((m) => m.threads === rs.threads);
    if (!ms) continue;
    const ratio = (rs.aggregate_qps / ms.aggregate_qps).toFixed(2);
    console.log(`| ${rs.threads} | ${rs.aggregate_qps.toFixed(0)} | ${ms.aggregate_qps.toFixed(0)} | ${ratio}× |`);
  }
}

async function main() {
  const opts = parseArgs(process.argv);

  if (opts.compare) {
    if (opts.threadMatrix) {
      console.log('Running thread-matrix benchmark (compare mode)…');
      const rusql = await benchThreadMatrix({
        ...opts,
        port: opts.rusqlPort,
        label: 'rusql',
      });
      const mysql = await benchThreadMatrix({
        ...opts,
        port: opts.mysqlPort,
        label: 'mysql',
      });
      const report = { rusql, mysql };
      const out =
        opts.output || join(root, 'docs/en/reports/performance-benchmark-thread-matrix.json');
      writeReport(report, out);
      scalingTable(rusql, mysql);
      return;
    }

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

  if (opts.threadMatrix) {
    console.log(`Thread-matrix benchmark ${opts.label} @ ${opts.host}:${opts.port}…`);
    const report = await benchThreadMatrix(opts);
    writeReport(report, opts.output);
    return;
  }

  console.log(`Benchmark ${opts.label} @ ${opts.host}:${opts.port} (${opts.threads} threads)…`);
  const report = await benchOne(opts);
  if (report) {
    writeReport(report, opts.output);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
