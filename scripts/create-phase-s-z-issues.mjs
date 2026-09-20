#!/usr/bin/env node
/**
 * Generate issue bodies and create Phase S–Z GitHub milestones + issues.
 * Idempotent: skips if any issue title (open or closed) already contains [M133] etc.
 *
 * Usage:
 *   node scripts/create-phase-s-z-issues.mjs --bodies-only
 *   node scripts/create-phase-s-z-issues.mjs
 *
 * Never labels issues `agent-ready` (sequencing + M114 overlap).
 */
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PHASES, allIssues, renderBody, bodyFileName } from './phase-s-z-catalog.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = process.env.RUSQL_GITHUB_REPO ?? 'tanbamboo/rusql';
const bodiesOnly = process.argv.includes('--bodies-only');

function runGh(args, options = {}) {
  const r = spawnSync('gh', args, {
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: 20 * 1024 * 1024,
    ...options,
  });
  if (r.status !== 0) {
    const err = (r.stderr || r.stdout || '').trim();
    throw new Error(`gh ${args.join(' ')}\n${err}`);
  }
  return (r.stdout || '').trim();
}

function writeBodies() {
  const dir = join(root, '.github', 'issue-bodies');
  mkdirSync(dir, { recursive: true });
  let n = 0;
  for (const { phase, issue } of allIssues()) {
    const name = bodyFileName(issue);
    const path = join(dir, name);
    writeFileSync(path, renderBody(phase, issue), 'utf8');
    n++;
  }
  console.log(`OK: wrote ${n} issue bodies under .github/issue-bodies/`);
}

function existingTitles() {
  const open = JSON.parse(runGh(['issue', 'list', '--repo', REPO, '--state', 'open', '--limit', '500', '--json', 'title']));
  const closed = JSON.parse(runGh(['issue', 'list', '--repo', REPO, '--state', 'closed', '--limit', '500', '--json', 'title']));
  return [...open, ...closed].map((i) => i.title);
}

function ensureMilestone(title, description) {
  const list = JSON.parse(runGh(['api', `repos/${REPO}/milestones?state=open`]));
  const found = list.find((m) => m.title === title);
  if (found) {
    console.log(`OK: milestone exists #${found.number} ${title}`);
    return found;
  }
  const created = JSON.parse(
    runGh(['api', `repos/${REPO}/milestones`, '--input', '-'], {
      input: JSON.stringify({ title, description, state: 'open' }),
    }),
  );
  console.log(`OK: created milestone #${created.number} ${title}`);
  return created;
}

function extraLabels(phase, issue) {
  const labels = new Set(issue.extraLabels ?? []);
  for (const l of phase.defaultExtraLabels ?? []) labels.add(l);
  return [...labels];
}

writeBodies();
if (bodiesOnly) {
  process.exit(0);
}

const existing = existingTitles();
const created = [];
const skipped = [];

for (const phase of PHASES) {
  ensureMilestone(phase.milestone, phase.description);
  for (const issue of phase.issues) {
    const marker = `[${issue.id}]`;
    if (existing.some((t) => t.includes(marker))) {
      console.log(`SKIP: ${marker} already exists`);
      skipped.push(issue.id);
      continue;
    }
    const bodyPath = join(root, '.github', 'issue-bodies', bodyFileName(issue));
    if (!existsSync(bodyPath)) {
      console.error(`FAIL: missing body ${bodyPath}`);
      process.exit(1);
    }
    const extras = extraLabels(phase, issue);
    const labels = [...issue.labels.split(','), `priority:${issue.priority}`, ...extras];
    const title = `[${issue.priority}] ${issue.id}: ${issue.title}`;
    const args = [
      'issue',
      'create',
      '--repo',
      REPO,
      '--title',
      title,
      '--milestone',
      phase.milestone,
      '--body-file',
      bodyPath,
    ];
    for (const label of labels) {
      args.push('--label', label);
    }
    const url = runGh(args);
    created.push({ id: issue.id, url });
    console.log(`OK: ${url}`);
  }
}

console.log(`\nCreated ${created.length} issue(s); skipped ${skipped.length}.`);
if (created.length) {
  console.log('Update docs/en/specs/mysql-full-parity-roadmap.md issue numbers.');
  for (const c of created) {
    console.log(`${c.id} ${c.url}`);
  }
}
