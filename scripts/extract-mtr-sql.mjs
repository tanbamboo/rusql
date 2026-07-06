#!/usr/bin/env node
/**
 * Extract portable SQL steps from Oracle mysql-test .test files for M30 manifest growth.
 *
 * Strips MTR runner commands (onlyif, skipif, connect, echo, loops, etc.) and emits
 * JSON suites compatible with tests/mysql-test/manifest.json.
 *
 * Usage:
 *   node scripts/extract-mtr-sql.mjs path/to/foo.test
 *   node scripts/extract-mtr-sql.mjs --stdin < foo.test
 *   node scripts/extract-mtr-sql.mjs --name my_case t/select.test
 *
 * SQuaLity-aligned skip rules (see tests/mysql-test/SKIPS.md):
 *   - Runner commands (112 non-SQL verbs) are dropped, not ported
 *   - onlyif/skipif blocks are omitted entirely
 *   - Multi-connection / replication / charset cases should be filtered manually
 */
import { readFileSync } from 'node:fs';
import { basename } from 'node:path';

const RUNNER_PREFIX = /^(onlyif|skipif|let|inc|dec|while|end|if|else|error|warning|sleep|connect|connection|disconnect|reconnect|ping|send|wait_for|source|replace_result|replace_column|eval_result|query_vertical|horizontal_query|vertical_results|sorted_result|disable_query_log|enable_query_log|enable_reconnect|disable_reconnect|exec|system|chmod|write_file|remove_file|append_file|diff_files|cat_file|copy_files|move_file|file_exist|file_not_exist|mkdir|rmdir|list_files|send_eval|send_quit|die|exit)\b/i;

const SQL_START = /^(select|insert|update|delete|create|drop|alter|show|describe|desc|use|begin|commit|rollback|set|truncate|rename|replace|call|prepare|execute|deallocate)\b/i;

function parseArgs(argv) {
  const opts = { name: null, stdin: false, files: [] };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--stdin') opts.stdin = true;
    else if (a === '--name') opts.name = argv[++i];
    else opts.files.push(a);
  }
  return opts;
}

function originTag(path) {
  const base = basename(path);
  return `mysql-test/t/${base}`;
}

function extractSql(text) {
  const steps = [];

  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    if (line.startsWith('--')) {
      const cmd = line.slice(2).trim();
      if (/^(onlyif|skipif)\b/i.test(cmd)) continue;
      if (RUNNER_PREFIX.test(cmd)) continue;
      continue;
    }
    if (RUNNER_PREFIX.test(line)) continue;
    if (SQL_START.test(line)) {
      const sql = line.endsWith(';') ? line.slice(0, -1) : line;
      steps.push({ sql, expect: { type: 'ok' } });
    }
  }
  return steps;
}

function emitSuite(name, origin, steps) {
  return {
    suites: [
      {
        name,
        origin,
        steps,
      },
    ],
  };
}

const opts = parseArgs(process.argv);
const inputs = [];

if (opts.stdin) {
  inputs.push({ name: opts.name ?? 'stdin_case', origin: 'mysql-test/t/unknown.test', text: readFileSync(0, 'utf8') });
} else {
  for (const file of opts.files) {
    const text = readFileSync(file, 'utf8');
    const stem = basename(file, '.test');
    inputs.push({
      name: opts.name ?? stem.replace(/[^a-z0-9_]+/gi, '_'),
      origin: originTag(file),
      text,
    });
  }
}

if (inputs.length === 0) {
  console.error('Usage: node scripts/extract-mtr-sql.mjs [--name CASE] file.test [...]');
  console.error('       node scripts/extract-mtr-sql.mjs --stdin < file.test');
  process.exit(1);
}

const out = { suites: [] };
for (const { name, origin, text } of inputs) {
  const steps = extractSql(text);
  if (steps.length === 0) {
    console.error(`WARN: no portable SQL in ${origin}`);
    continue;
  }
  out.suites.push({ name, origin, steps });
}

console.log(JSON.stringify(out.suites.length === 1 ? emitSuite(out.suites[0].name, out.suites[0].origin, out.suites[0].steps) : out, null, 2));
