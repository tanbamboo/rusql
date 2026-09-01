#!/usr/bin/env node
/**
 * Create M36+ full-parity and PERF-B* GitHub issues.
 * Idempotent: skips if an open issue title already contains the milestone id.
 */
import { execSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = process.env.RUSQL_GITHUB_REPO ?? 'tanbamboo/rusql';

const ISSUES = [
  { id: 'M36', title: 'CREATE/DROP DATABASE + multi-schema catalog', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M37', title: 'AUTO_INCREMENT columns', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M38', title: 'ALTER TABLE extended (DROP/MODIFY/RENAME)', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M39', title: 'FOREIGN KEY constraints', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M40', title: 'Extended data types (DECIMAL, DATETIME, TEXT/BLOB, JSON)', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M41', title: 'LEFT/RIGHT OUTER JOIN', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M42', title: 'Subqueries (IN, EXISTS, derived tables)', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M43', title: 'GROUP BY, HAVING, aggregate functions', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M44', title: 'UNION / UNION ALL', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M45', title: 'Extended WHERE (OR, NOT, LIKE, BETWEEN, IN lists)', priority: 'P0', labels: 'enhancement,area:sql', ready: true },
  { id: 'M46', title: 'SQL expressions and built-in functions', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M47', title: 'Stored procedures and functions', priority: 'P3', labels: 'enhancement,area:sql', ready: false },
  { id: 'M48', title: 'Triggers (BEFORE/AFTER DML)', priority: 'P3', labels: 'enhancement,area:sql', ready: false },
  { id: 'M49', title: 'Cost-based planner and index selection', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M50', title: 'Composite and covering indexes', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M51', title: 'COM_CHANGE_USER + COM_RESET_CONNECTION', priority: 'P2', labels: 'enhancement,area:protocol', ready: false },
  { id: 'M52', title: 'COM_FIELD_LIST + COM_STMT_RESET / long data', priority: 'P2', labels: 'enhancement,area:protocol', ready: false },
  { id: 'M53', title: 'COM_PROCESS_INFO + SHOW PROCESSLIST', priority: 'P2', labels: 'enhancement,area:protocol', ready: false },
  { id: 'M54', title: 'GRANT/REVOKE privilege model', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M55-auth', title: 'Multi-user accounts + mysql_native_password', priority: 'P2', labels: 'enhancement,area:protocol', ready: false, bodyFile: 'issue-m55-auth.md' },
  { id: 'M56', title: 'Production binlog event stream', priority: 'P3', labels: 'enhancement,area:storage', ready: false },
  { id: 'M57', title: 'Replica applier + COM_BINLOG_DUMP', priority: 'P3', labels: 'enhancement,area:storage', ready: false },
  { id: 'M58', title: 'GTID sets and failover semantics', priority: 'P3', labels: 'enhancement,area:storage', ready: false },
  { id: 'M59', title: 'Full utf8mb4 collation (compare/sort)', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M60', title: 'mysql-test subset expansion (100+ portable cases)', priority: 'P1', labels: 'enhancement,area:harness', ready: false },
  { id: 'M61', title: 'Sysbench-compatible OLTP schema', priority: 'P2', labels: 'enhancement,area:harness', ready: false },
  { id: 'PERF-B1', title: 'Persistent-connection benchmark harness', priority: 'P1', labels: 'enhancement,area:harness', ready: false, bodyFile: 'issue-perf-b1.md' },
  { id: 'PERF-B2', title: 'Scan + ORDER BY + LIMIT optimization', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-perf-b2.md' },
  { id: 'PERF-B3', title: 'Primary-key UPDATE path optimization', priority: 'P1', labels: 'enhancement,area:storage', ready: false, bodyFile: 'issue-perf-b3.md' },
  { id: 'PERF-B4', title: 'Multi-threaded benchmark (1/4/8/16 clients)', priority: 'P2', labels: 'enhancement,area:harness', ready: false, bodyFile: 'issue-perf-b4.md' },
  { id: 'PERF-B5', title: 'WAL fsync policy vs throughput tuning', priority: 'P2', labels: 'enhancement,area:storage', ready: false, bodyFile: 'issue-perf-b5.md' },
  { id: 'PERF-B6', title: 'Sysbench oltp_point_select CI gate', priority: 'P2', labels: 'enhancement,area:harness', ready: false, bodyFile: 'issue-perf-b6.md' },
];

function gh(args) {
  return execSync(`gh ${args} --repo ${REPO}`, { encoding: 'utf8' }).trim();
}

function openTitles() {
  return JSON.parse(gh('issue list --state open --limit 200 --json title')).map((i) => i.title);
}

function bodyPath(issue) {
  const name = issue.bodyFile ?? `issue-${issue.id.toLowerCase()}.md`;
  const p = join(root, '.github', 'issue-bodies', name);
  return existsSync(p) ? p : null;
}

const existing = openTitles();
const created = [];

for (const m of ISSUES) {
  const marker = `[${m.id}]`;
  if (existing.some((t) => t.includes(marker))) {
    console.log(`SKIP: ${marker} already open`);
    continue;
  }
  const bodyFile = bodyPath(m);
  if (!bodyFile) {
    console.error(`FAIL: missing body for ${m.id}`);
    process.exit(1);
  }
  const labelList = [m.labels, `priority:${m.priority}`, ...(m.ready ? ['agent-ready'] : [])].join(',');
  const url = gh(
    `issue create --title "[${m.priority}] ${m.id}: ${m.title}" --label "${labelList}" --body-file "${bodyFile.replace(/\\/g, '/')}"`
  );
  created.push({ id: m.id, url });
  console.log(`OK: ${url}`);
}

console.log(`\nCreated ${created.length} issue(s).`);
if (created.length) {
  console.log('Update docs/en/specs/mysql-full-parity-roadmap.md issue numbers.');
}
