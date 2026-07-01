#!/usr/bin/env node
/**
 * Validate HANDOFF.md is consistent with main branch state.
 */
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const handoffPath = join(root, 'HANDOFF.md');
let errors = 0;

function fail(msg) {
  console.error(`FAIL: ${msg}`);
  errors++;
}

function ok(msg) {
  console.log(`OK: ${msg}`);
}

if (!existsSync(handoffPath)) {
  fail('HANDOFF.md missing');
  process.exit(1);
}

const handoff = readFileSync(handoffPath, 'utf8');
let currentBranch = 'main';
try {
  currentBranch = execSync('git rev-parse --abbrev-ref HEAD', {
    cwd: root,
    encoding: 'utf8',
  }).trim();
} catch {
  /* ignore */
}
const dateMatch = handoff.match(/Last updated\s*\|\s*([^\|]+)/);
if (!dateMatch) {
  fail('HANDOFF missing Last updated field');
} else {
  const updated = dateMatch[1].trim();
  const ageDays = (Date.now() - Date.parse(updated)) / 86_400_000;
  if (Number.isNaN(ageDays)) {
    fail(`HANDOFF Last updated not parseable: ${updated}`);
  } else if (ageDays > 7) {
    fail(`HANDOFF stale (${Math.floor(ageDays)} days old): update after merge`);
  } else {
    ok(`HANDOFF updated ${updated}`);
  }
}

if (/PR pending/i.test(handoff) && currentBranch === 'main') {
  fail('HANDOFF still says PR pending — update Current issue / branch after merge');
}

const branchMatch = handoff.match(/Branch\s*\|\s*`?([^`\|\n]+)`?/);
if (branchMatch) {
  const branch = branchMatch[1].trim();
  if (branch === 'main') {
    ok('HANDOFF branch is main');
  } else if (currentBranch === 'main') {
    fail(`HANDOFF branch '${branch}' still set — should be main after merge`);
  } else {
    ok(`HANDOFF branch '${branch}' (feature work in progress)`);
  }
}

if (errors > 0) {
  console.error(`\nHANDOFF check failed with ${errors} error(s).`);
  process.exit(1);
}

console.log('\nHANDOFF check passed.');
process.exit(0);
