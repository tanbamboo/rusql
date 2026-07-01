#!/usr/bin/env node
/**
 * Harness metrics — reproducible project health snapshot (stdout JSON).
 */
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = process.env.RUSQL_GITHUB_REPO ?? 'tanbamboo/rusql';

function countTestsInRust() {
  let count = 0;
  const crates = join(root, 'crates');
  for (const crate of readdirSync(crates)) {
    const src = join(crates, crate, 'src');
    if (!existsSync(src)) continue;
    for (const file of walk(src)) {
      if (!file.endsWith('.rs')) continue;
      const text = readFileSync(file, 'utf8');
      count += (text.match(/#\[test\]/g) ?? []).length;
      count += (text.match(/#\[tokio::test\]/g) ?? []).length;
    }
  }
  return count;
}

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, name.name);
    if (name.isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

function compatCaseCount() {
  const path = join(root, 'crates/rusql-server/compat/basic.json');
  if (!existsSync(path)) return { suites: 0, steps: 0 };
  const data = JSON.parse(readFileSync(path, 'utf8'));
  const suites = data.suites?.length ?? 0;
  const steps = (data.suites ?? []).reduce((n, s) => n + (s.steps?.length ?? 0), 0);
  return { suites, steps };
}

function gitMetrics() {
  const commits = Number(
    execSync('git rev-list --count main', { cwd: root, encoding: 'utf8' }).trim()
  );
  return { main_commits: commits };
}

function ghMetrics() {
  try {
    const prs = JSON.parse(
      execSync(`gh pr list --repo ${REPO} --state merged --json number --limit 100`, {
        encoding: 'utf8',
      })
    );
    const issues = JSON.parse(
      execSync(`gh issue list --repo ${REPO} --state closed --json number --limit 100`, {
        encoding: 'utf8',
      })
    );
    return { merged_prs: prs.length, closed_issues: issues.length };
  } catch {
    return { merged_prs: null, closed_issues: null, gh_error: true };
  }
}

const metrics = {
  generated_at: new Date().toISOString(),
  rust_test_functions: countTestsInRust(),
  compat: compatCaseCount(),
  git: gitMetrics(),
  github: ghMetrics(),
};

console.log(JSON.stringify(metrics, null, 2));
