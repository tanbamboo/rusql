#!/usr/bin/env node
/**
 * Build en + zh-CN mdBooks when mdbook is on PATH; otherwise print SKIP.
 */
import { execSync, spawnSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function hasMdbook() {
  try {
    execSync('mdbook --version', { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

if (!hasMdbook()) {
  console.log('SKIP: mdbook not installed (cargo install mdbook)');
  process.exit(0);
}

const editions = ['en', 'zh-CN'];
for (const edition of editions) {
  const dir = join(root, 'docs/book', edition);
  console.log(`Building ${edition}...`);
  const r = spawnSync('mdbook', ['build'], { cwd: dir, stdio: 'inherit' });
  if (r.status !== 0) {
    process.exit(r.status ?? 1);
  }
}

console.log('Book build complete → book-output/');
