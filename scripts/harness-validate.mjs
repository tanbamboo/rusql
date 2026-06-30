#!/usr/bin/env node
/**
 * Harness structure validator — checks anr.yaml required files/dirs and profile configs.
 */
import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs';
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

function parseSimpleYaml(content) {
  const result = { required_files: [], required_directories: [], profiles: [], active_profile: '' };
  let section = null;
  for (const line of content.split('\n')) {
    const t = line.trim();
    if (!t || t.startsWith('#')) continue;
    if (t.startsWith('required_files:')) {
      section = 'required_files';
      continue;
    }
    if (t.startsWith('required_directories:')) {
      section = 'required_directories';
      continue;
    }
    if (t.startsWith('profiles:')) {
      section = 'profiles';
      continue;
    }
    if (t.startsWith('packages:') || t.startsWith('version:') || t.startsWith('name:') || t.startsWith('description:')) {
      section = null;
      continue;
    }
    if (t.startsWith('active_profile:')) {
      result.active_profile = t.split(':').slice(1).join(':').trim();
      section = null;
      continue;
    }
    if (t.startsWith('- ') && section) {
      result[section].push(t.slice(2).trim());
      continue;
    }
    if (!t.startsWith('-') && t.includes(':') && !t.startsWith('path:') && !t.startsWith('profile:')) {
      section = null;
    }
  }
  return result;
}

const anrPath = join(root, 'anr.yaml');
if (!existsSync(anrPath)) {
  fail('anr.yaml missing');
} else {
  const manifest = parseSimpleYaml(readFileSync(anrPath, 'utf8'));
  ok('anr.yaml found');

  for (const f of manifest.required_files) {
    if (!existsSync(join(root, f))) fail(`missing required file: ${f}`);
    else ok(`required file: ${f}`);
  }

  for (const d of manifest.required_directories) {
    if (!existsSync(join(root, d))) fail(`missing required directory: ${d}`);
    else ok(`required directory: ${d}`);
  }

  const profile = manifest.active_profile;
  if (!profile) fail('active_profile not set in anr.yaml');
  else {
    const profileDir = join(root, 'profiles', profile);
    if (!existsSync(profileDir)) fail(`active profile directory missing: profiles/${profile}`);
    else ok(`active profile: ${profile}`);

    for (const file of ['profile.yaml', 'guides.md', 'sensors.yaml']) {
      const p = join(profileDir, file);
      if (!existsSync(p)) fail(`profile missing ${file}: profiles/${profile}/${file}`);
      else ok(`profile file: profiles/${profile}/${file}`);
    }
  }

  for (const p of manifest.profiles) {
    const sensors = join(root, 'profiles', p, 'sensors.yaml');
    if (!existsSync(sensors)) fail(`profile sensors missing: profiles/${p}/sensors.yaml`);
  }
}

// Cursor harness files
const cursorChecks = [
  '.cursor/rules/core.mdc',
  '.cursor/hooks/hooks.json',
  '.cursor/agents/implementer.md',
];
for (const f of cursorChecks) {
  if (!existsSync(join(root, f))) fail(`cursor harness missing: ${f}`);
  else ok(`cursor: ${f}`);
}

// Skills canonical + cursor wrapper
const skillsDir = join(root, '.agents', 'skills');
if (existsSync(skillsDir)) {
  for (const name of readdirSync(skillsDir)) {
    const skillMd = join(skillsDir, name, 'SKILL.md');
    if (statSync(join(skillsDir, name)).isDirectory() && !existsSync(skillMd)) {
      fail(`skill missing SKILL.md: .agents/skills/${name}`);
    }
  }
  ok('skills directory scanned');
}

if (errors > 0) {
  console.error(`\nHarness validation failed with ${errors} error(s).`);
  process.exit(1);
}

console.log('\nHarness validation passed.');
process.exit(0);
