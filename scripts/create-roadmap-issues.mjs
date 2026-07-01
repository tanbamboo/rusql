#!/usr/bin/env node
/**
 * Create MySQL compat roadmap GitHub issues from templates.
 * Idempotent: skips if an open issue title already contains the milestone id.
 */
import { execSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = process.env.RUSQL_GITHUB_REPO ?? 'tanbamboo/rusql';

const MILESTONES = [
  { id: 'M17', title: 'ORDER BY', priority: 'P0', labels: 'enhancement,area:sql', ready: true },
  { id: 'M18', title: 'SELECT column aliases', priority: 'P0', labels: 'enhancement,area:sql', ready: false },
  { id: 'M19', title: 'LIMIT OFFSET', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M20', title: 'WHERE comparisons and AND', priority: 'P0', labels: 'enhancement,area:sql', ready: false },
  { id: 'M21', title: 'IS NULL predicates', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M22', title: 'INNER JOIN', priority: 'P0', labels: 'enhancement,area:sql', ready: false },
  { id: 'M23', title: 'PRIMARY KEY metadata', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M24', title: 'ALTER TABLE ADD COLUMN', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M25', title: 'Binary resultset', priority: 'P1', labels: 'enhancement,area:protocol', ready: false },
  { id: 'M26', title: 'caching_sha2 RSA auth', priority: 'P2', labels: 'enhancement,area:protocol', ready: false },
  { id: 'M27', title: 'information_schema expansion', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M28', title: 'SHOW INDEX', priority: 'P1', labels: 'enhancement,area:sql', ready: false },
  { id: 'M29', title: 'mysql-diff runner', priority: 'P0', labels: 'enhancement,area:harness', ready: false },
  { id: 'M30', title: 'mysql-test subset', priority: 'P2', labels: 'enhancement,area:harness', ready: false },
  { id: 'M31', title: 'Durable COMMIT WAL', priority: 'P0', labels: 'enhancement,area:storage', ready: false },
  { id: 'M32', title: 'MVCC snapshot isolation', priority: 'P2', labels: 'enhancement,area:storage', ready: false },
  { id: 'M33', title: 'SQL views', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
  { id: 'M34', title: 'Binlog replication', priority: 'P3', labels: 'enhancement,area:storage', ready: false },
  { id: 'M35', title: 'utf8mb4 charset metadata', priority: 'P2', labels: 'enhancement,area:sql', ready: false },
];

function gh(args) {
  return execSync(`gh ${args} --repo ${REPO}`, { encoding: 'utf8' }).trim();
}

function openTitles() {
  return JSON.parse(
    gh('issue list --state open --limit 100 --json title')
  ).map((i) => i.title);
}

function bodyPath(id) {
  const slug = id.toLowerCase();
  const p = join(root, '.github', 'issue-bodies', `issue-${slug}.md`);
  return existsSync(p) ? p : null;
}

const existing = openTitles();
const created = [];

for (const m of MILESTONES) {
  const marker = `[${m.id}]`;
  if (existing.some((t) => t.includes(marker))) {
    console.log(`SKIP: ${marker} already open`);
    continue;
  }
  const bodyFile = bodyPath(m.id);
  if (!bodyFile) {
    console.error(`FAIL: missing ${m.id} body template`);
    process.exit(1);
  }
  const labelList = [
    m.labels,
    `priority:${m.priority}`,
    ...(m.ready ? ['agent-ready'] : []),
  ].join(',');
  const url = gh(
    `issue create --title "[${m.priority}] ${m.id}: ${m.title}" --label "${labelList}" --body-file "${bodyFile.replace(/\\/g, '/')}"`
  );
  created.push(url);
  console.log(`OK: ${url}`);
}

console.log(`\nCreated ${created.length} issue(s). Roadmap: docs/en/specs/mysql-compat-roadmap.md`);
