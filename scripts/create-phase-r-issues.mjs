#!/usr/bin/env node
/**
 * Create Phase R (M114–M132) GitHub issues from templates.
 * Idempotent: skips if any issue title (open or closed) already contains [M114] etc.
 */
import { execSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = process.env.RUSQL_GITHUB_REPO ?? 'tanbamboo/rusql';
const MILESTONE = 'Phase R — Post-Q client SQL (M114–M132)';

const ISSUES = [
  { id: 'M114', title: 'CREATE DATABASE CHARACTER SET / COLLATE', priority: 'P0', labels: 'enhancement,area:sql', ready: true, bodyFile: 'issue-m114-create-database-charset.md' },
  { id: 'M115', title: 'JSON_EXTRACT ($.key)', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m115-json-extract.md' },
  { id: 'M116', title: 'UUID()', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m116-uuid.md' },
  { id: 'M117', title: 'LAST_INSERT_ID(expr) setter', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m117-last-insert-id-expr.md' },
  { id: 'M118', title: 'GET_LOCK / RELEASE_LOCK', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m118-get-lock.md' },
  { id: 'M119', title: 'information_schema.TABLE_CONSTRAINTS', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m119-table-constraints.md' },
  { id: 'M120', title: 'information_schema.PROCESSLIST', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m120-information-schema-processlist.md' },
  { id: 'M121', title: 'information_schema.PARAMETERS', priority: 'P2', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m121-information-schema-parameters.md' },
  { id: 'M122', title: 'SHOW BINARY LOGS', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m122-show-binary-logs.md' },
  { id: 'M123', title: 'SHOW BINLOG EVENTS', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m123-show-binlog-events.md' },
  { id: 'M124', title: 'CREATE OR REPLACE VIEW', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m124-create-or-replace-view.md' },
  { id: 'M125', title: 'PREPARE / EXECUTE / DEALLOCATE PREPARE (text)', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m125-prepare-execute-text.md' },
  { id: 'M126', title: 'SAVEPOINT / ROLLBACK TO / RELEASE', priority: 'P1', labels: 'enhancement,area:storage', ready: false, bodyFile: 'issue-m126-savepoint.md' },
  { id: 'M127', title: 'WITH RECURSIVE', priority: 'P1', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m127-with-recursive.md' },
  { id: 'M128', title: 'INTERSECT', priority: 'P2', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m128-intersect.md' },
  { id: 'M129', title: 'Window ROWS BETWEEN frames', priority: 'P2', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m129-window-frame-rows.md' },
  { id: 'M130', title: 'CREATE EVENT DISABLE ON SLAVE', priority: 'P2', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m130-event-disable-on-slave.md' },
  { id: 'M131', title: 'SHOW ENGINE INNODB STATUS stub', priority: 'P2', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m131-show-engine-innodb-status.md' },
  { id: 'M132', title: 'Procedure IN parameters', priority: 'P2', labels: 'enhancement,area:sql', ready: false, bodyFile: 'issue-m132-procedure-in-param.md' },
];

function gh(args) {
  return execSync(`gh ${args} --repo ${REPO}`, { encoding: 'utf8' }).trim();
}

function ghApi(args) {
  return execSync(`gh api ${args}`, { encoding: 'utf8' }).trim();
}

function allTitles() {
  const open = JSON.parse(gh('issue list --state open --limit 200 --json title'));
  const closed = JSON.parse(gh('issue list --state closed --limit 200 --json title'));
  return [...open, ...closed].map((i) => i.title);
}

function ensureMilestone() {
  const list = JSON.parse(ghApi(`repos/${REPO}/milestones?state=open`));
  const found = list.find((m) => m.title === MILESTONE);
  if (found) {
    console.log(`OK: milestone exists #${found.number}`);
    return found.title;
  }
  const created = JSON.parse(
    ghApi(
      `repos/${REPO}/milestones -f title="${MILESTONE}" -f description="High-ROI gap-probe slices after Phase Q. Canonical plan: docs/en/specs/mysql-full-parity-roadmap.md"`
    )
  );
  console.log(`OK: created milestone #${created.number}`);
  return created.title;
}

function bodyPath(issue) {
  const p = join(root, '.github', 'issue-bodies', issue.bodyFile);
  return existsSync(p) ? p : null;
}

ensureMilestone();
const existing = allTitles();
const created = [];

for (const m of ISSUES) {
  const marker = `[${m.id}]`;
  if (existing.some((t) => t.includes(marker))) {
    console.log(`SKIP: ${marker} already exists`);
    continue;
  }
  const bodyFile = bodyPath(m);
  if (!bodyFile) {
    console.error(`FAIL: missing body for ${m.id}`);
    process.exit(1);
  }
  const labelList = [m.labels, `priority:${m.priority}`, ...(m.ready ? ['agent-ready'] : [])].join(',');
  const pathArg = bodyFile.replace(/\\/g, '/');
  const url = gh(
    `issue create --title "[${m.priority}] ${m.id}: ${m.title}" --label "${labelList}" --milestone "${MILESTONE}" --body-file "${pathArg}"`
  );
  created.push({ id: m.id, url });
  console.log(`OK: ${url}`);
}

console.log(`\nCreated ${created.length} issue(s).`);
if (created.length) {
  console.log('Update docs/en/specs/mysql-full-parity-roadmap.md issue numbers.');
}
