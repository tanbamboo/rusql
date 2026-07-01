#!/usr/bin/env node
/**
 * Validate mdBook chapter structure for en + zh-CN without requiring mdbook binary.
 */
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const locales = [
  { id: 'en', dir: join(root, 'docs/book/en/src') },
  { id: 'zh-CN', dir: join(root, 'docs/book/zh-CN/src') },
];

let errors = 0;

function fail(msg) {
  console.error(`FAIL: ${msg}`);
  errors++;
}

function ok(msg) {
  console.log(`OK: ${msg}`);
}

function chapterPaths(summaryDir) {
  const summary = readFileSync(join(summaryDir, 'SUMMARY.md'), 'utf8');
  const paths = [];
  for (const line of summary.split('\n')) {
    const m = line.match(/\]\(\.\/([^)]+)\)/);
    if (m) paths.push(m[1]);
  }
  return paths;
}

for (const { id, dir } of locales) {
  const bookToml = join(root, `docs/book/${id}/book.toml`);
  if (!existsSync(bookToml)) {
    fail(`${id}: missing book.toml`);
    continue;
  }
  ok(`${id}: book.toml present`);

  const paths = chapterPaths(dir);
  if (paths.length < 10) {
    fail(`${id}: expected at least 10 chapters, got ${paths.length}`);
  } else {
    ok(`${id}: ${paths.length} chapters in SUMMARY`);
  }

  for (const rel of paths) {
    const full = join(dir, rel);
    if (!existsSync(full)) {
      fail(`${id}: missing chapter file ${rel}`);
    }
  }
}

const enPaths = chapterPaths(locales[0].dir);
const zhPaths = chapterPaths(locales[1].dir);
if (enPaths.length !== zhPaths.length) {
  fail(`chapter count mismatch en=${enPaths.length} zh=${zhPaths.length}`);
} else {
  ok(`en/zh chapter parity (${enPaths.length})`);
}

for (let i = 0; i < enPaths.length; i++) {
  if (enPaths[i] !== zhPaths[i]) {
    fail(`chapter path mismatch at index ${i}: en=${enPaths[i]} zh=${zhPaths[i]}`);
  }
}
if (errors === 0) {
  ok('chapter path alignment en ↔ zh');
}

if (errors > 0) {
  console.error(`\nBook check failed with ${errors} error(s).`);
  process.exit(1);
}

console.log('\nBook check passed.');
process.exit(0);
