#!/usr/bin/env node
/**
 * Validate CHANGELOG and release-notes are present and structured.
 */
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let errors = 0;

function fail(msg) {
  console.error(`FAIL: ${msg}`);
  errors++;
}

function ok(msg) {
  console.log(`OK: ${msg}`);
}

const changelogPath = join(root, 'CHANGELOG.md');
const releaseEn = join(root, 'docs/en/release-notes.md');
const releaseZh = join(root, 'docs/zh-CN/release-notes.md');

for (const [p, label] of [
  [changelogPath, 'CHANGELOG.md'],
  [releaseEn, 'docs/en/release-notes.md'],
  [releaseZh, 'docs/zh-CN/release-notes.md'],
]) {
  if (!existsSync(p)) fail(`missing ${label}`);
  else ok(`found ${label}`);
}

if (errors === 0) {
  const changelog = readFileSync(changelogPath, 'utf8');
  if (!/^#\s+Changelog/m.test(changelog)) fail('CHANGELOG.md missing # Changelog heading');
  else ok('CHANGELOG heading');
  if (!/##\s+\[Unreleased\]/m.test(changelog)) fail('CHANGELOG.md missing [Unreleased] section');
  else ok('CHANGELOG [Unreleased] section');
  if (!/keepachangelog\.com/i.test(changelog)) fail('CHANGELOG.md should link Keep a Changelog');
  else ok('CHANGELOG format reference');

  const en = readFileSync(releaseEn, 'utf8');
  const zh = readFileSync(releaseZh, 'utf8');
  if (!/##\s+Latest:/i.test(en)) fail('en release-notes missing Latest section');
  else ok('en release-notes Latest section');
  if (!/最新/.test(zh)) fail('zh release-notes missing 最新 section');
  else ok('zh release-notes 最新 section');
  for (const marker of ['user-guide', 'check-changelog']) {
    if (!en.includes(marker)) fail(`en release-notes missing marker: ${marker}`);
    if (!zh.includes(marker)) fail(`zh release-notes missing marker: ${marker}`);
  }
  ok('release-notes cross-links present');
}

if (errors > 0) {
  console.error(`\nChangelog check failed with ${errors} error(s).`);
  process.exit(1);
}

console.log('\nChangelog check passed.');
process.exit(0);
