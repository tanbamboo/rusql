#!/usr/bin/env node
/**
 * Poll GitHub issues labeled needs-human for owner/maintainer replies.
 * Exit 0 always; prints JSON summary for agent consumption.
 */
import { execSync } from 'node:child_process';

const REPO = process.env.RUSQL_GITHUB_REPO ?? 'tanbamboo/rusql';

function gh(args) {
  return execSync(`gh ${args} --repo ${REPO}`, { encoding: 'utf8' }).trim();
}

let issues;
try {
  issues = JSON.parse(
    gh(
      'issue list --label needs-human --state open --json number,title,comments,updatedAt'
    )
  );
} catch (e) {
  console.error('FAIL: gh issue list failed — is gh authenticated?');
  process.exit(1);
}

const withReplies = [];
const awaiting = [];

for (const issue of issues) {
  const commentCount = issue.comments?.length ?? 0;
  if (commentCount > 0) {
    const last = issue.comments[commentCount - 1];
    withReplies.push({
      number: issue.number,
      title: issue.title,
      lastAuthor: last.author?.login,
      lastBody: last.body?.slice(0, 200),
      url: `https://github.com/${REPO}/issues/${issue.number}`,
    });
  } else {
    awaiting.push({ number: issue.number, title: issue.title });
  }
}

const summary = {
  repo: REPO,
  needsHumanOpen: issues.length,
  withReplies,
  awaitingReply: awaiting,
};

console.log(JSON.stringify(summary, null, 2));

if (withReplies.length > 0) {
  console.error(
    `\nACTION: ${withReplies.length} issue(s) have replies — record ADR, close or update issue, remove needs-human.`
  );
}
