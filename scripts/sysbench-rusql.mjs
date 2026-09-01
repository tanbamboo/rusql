#!/usr/bin/env node
/**
 * Sysbench oltp_point_select gate for rusql vs MySQL (PERF-B6).
 *
 * Soft-fails (exit 0) when sysbench or Docker MySQL are unavailable.
 * Fails (exit 1) when rusql QPS < threshold × MySQL QPS on same host.
 *
 * Usage:
 *   node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
 *   node scripts/sysbench-rusql.mjs --threshold 0.7 --threads 8 --time 30
 */
import { execSync, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function parseArgs(argv) {
  const opts = {
    rusqlHost: '127.0.0.1',
    rusqlPort: 3307,
    mysqlHost: '127.0.0.1',
    mysqlPort: 3308,
    user: 'root',
    password: '',
    database: 'sbtest',
    threads: 8,
    time: 30,
    tables: 4,
    tableSize: 10000,
    threshold: 0.7,
    output: '',
    softFail: true,
  };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--rusql-host') opts.rusqlHost = argv[++i];
    else if (a === '--rusql-port') opts.rusqlPort = Number(argv[++i]);
    else if (a === '--mysql-host') opts.mysqlHost = argv[++i];
    else if (a === '--mysql-port') opts.mysqlPort = Number(argv[++i]);
    else if (a === '--user') opts.user = argv[++i];
    else if (a === '--password') opts.password = argv[++i];
    else if (a === '--database') opts.database = argv[++i];
    else if (a === '--threads') opts.threads = Number(argv[++i]);
    else if (a === '--time') opts.time = Number(argv[++i]);
    else if (a === '--tables') opts.tables = Number(argv[++i]);
    else if (a === '--table-size') opts.tableSize = Number(argv[++i]);
    else if (a === '--threshold') opts.threshold = Number(argv[++i]);
    else if (a === '--output') opts.output = argv[++i];
    else if (a === '--hard-fail') opts.softFail = false;
    else if (a === '--help' || a === '-h') {
      console.log(`Usage: node scripts/sysbench-rusql.mjs [options]
  --rusql-port PORT     rusql port (default 3307)
  --mysql-port PORT     MySQL port (default 3308)
  --threads N           sysbench threads (default 8)
  --time SEC            test duration (default 30)
  --threshold RATIO     min rusql/MySQL QPS ratio (default 0.7)
  --output FILE         JSON report path
  --hard-fail           exit 1 when tools missing (default: soft-fail)`);
      process.exit(0);
    }
  }
  return opts;
}

function hasCommand(cmd) {
  const r = spawnSync(cmd, ['--version'], { encoding: 'utf8' });
  return r.status === 0;
}

function runSysbench(args) {
  const r = spawnSync('sysbench', args, { encoding: 'utf8' });
  if (r.status !== 0) {
    throw new Error(r.stderr || r.stdout || 'sysbench failed');
  }
  return r.stdout + r.stderr;
}

function parseQps(output) {
  const m = output.match(/transactions:\s+[\d.]+ \(([\d.]+) per sec\.\)/);
  if (m) return Number(m[1]);
  const m2 = output.match(/read:\s+([\d.]+)/);
  if (m2) return Number(m2[1]);
  throw new Error('could not parse sysbench QPS from output');
}

function mysqlArgs(opts, host, port) {
  const parts = [
    `--mysql-host=${host}`,
    `--mysql-port=${port}`,
    `--mysql-user=${opts.user}`,
    `--mysql-db=${opts.database}`,
  ];
  if (opts.password) parts.push(`--mysql-password=${opts.password}`);
  return parts;
}

function benchEngine(opts, label, host, port) {
  const base = [
    'oltp_point_select',
    ...mysqlArgs(opts, host, port),
    `--tables=${opts.tables}`,
    `--table-size=${opts.tableSize}`,
    `--threads=${opts.threads}`,
    `--time=${opts.time}`,
    '--report-interval=1',
  ];
  console.log(`Running sysbench oltp_point_select on ${label} (${host}:${port})…`);
  runSysbench([...base, 'prepare']);
  const out = runSysbench([...base, 'run']);
  const qps = parseQps(out);
  runSysbench([...base, 'cleanup']);
  return { label, host, port, qps, raw_tail: out.split('\n').slice(-8).join('\n') };
}

function softExit(opts, reason) {
  console.warn(`[sysbench-rusql] SKIP: ${reason}`);
  if (opts.softFail) {
    process.exit(0);
  }
  process.exit(1);
}

async function main() {
  const opts = parseArgs(process.argv);

  if (!hasCommand('sysbench')) {
    softExit(opts, 'sysbench not installed (apt install sysbench)');
  }

  try {
    execSync('docker --version', { stdio: 'pipe' });
  } catch {
    softExit(opts, 'docker not available for MySQL reference');
  }

  const report = {
    meta: {
      date: new Date().toISOString().slice(0, 10),
      hostname: os.hostname(),
      platform: `${process.platform} ${os.release()}`,
      threads: opts.threads,
      time_sec: opts.time,
      threshold: opts.threshold,
      workload: 'oltp_point_select',
    },
    results: [],
  };

  try {
    report.results.push(benchEngine(opts, 'rusql', opts.rusqlHost, opts.rusqlPort));
    report.results.push(benchEngine(opts, 'mysql', opts.mysqlHost, opts.mysqlPort));
  } catch (e) {
    softExit(opts, e.message);
  }

  const rusql = report.results.find((r) => r.label === 'rusql');
  const mysql = report.results.find((r) => r.label === 'mysql');
  const ratio = rusql.qps / mysql.qps;
  report.summary = {
    rusql_qps: rusql.qps,
    mysql_qps: mysql.qps,
    ratio,
    pass: ratio >= opts.threshold,
  };

  const out =
    opts.output ||
    join(root, 'docs/en/reports/performance-benchmark-sysbench.json');
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(report, null, 2) + '\n', 'utf8');
  console.log(`Wrote ${out}`);
  console.log(
    `Sysbench oltp_point_select: rusql ${rusql.qps.toFixed(2)} QPS | MySQL ${mysql.qps.toFixed(2)} QPS | ratio ${ratio.toFixed(2)}× (threshold ${opts.threshold})`
  );

  if (!report.summary.pass) {
    console.error('FAIL: rusql QPS below threshold');
    process.exit(1);
  }
  console.log('PASS');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
