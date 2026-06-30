#!/usr/bin/env node
/**
 * Sync canonical .agents/skills → .cursor/skills thin wrappers.
 */
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const srcDir = join(root, '.agents', 'skills');
const destDir = join(root, '.cursor', 'skills');

if (!existsSync(srcDir)) {
  console.error('No .agents/skills directory found.');
  process.exit(1);
}

mkdirSync(destDir, { recursive: true });

for (const name of readdirSync(srcDir)) {
  const skillPath = join(srcDir, name, 'SKILL.md');
  if (!statSync(join(srcDir, name)).isDirectory() || !existsSync(skillPath)) continue;

  const content = readFileSync(skillPath, 'utf8');
  const frontmatter = content.match(/^---\n([\s\S]*?)\n---/);
  const nameMatch = frontmatter?.[1]?.match(/^name:\s*(.+)$/m);
  const descMatch = frontmatter?.[1]?.match(/^description:\s*(.+)$/m);
  const skillName = nameMatch?.[1]?.trim() ?? name;
  const description = descMatch?.[1]?.trim() ?? `Synced from .agents/skills/${name}`;

  const wrapper = `---
name: ${skillName}
description: ${description} (synced from .agents/skills/${name})
---

# ${skillName}

> Canonical source: [.agents/skills/${name}/SKILL.md](../../.agents/skills/${name}/SKILL.md)

Read and follow the canonical skill file above.
`;

  const outPath = join(destDir, name, 'SKILL.md');
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, wrapper, 'utf8');
  console.log(`Synced: .cursor/skills/${name}/SKILL.md`);
}

console.log('Sync complete.');
