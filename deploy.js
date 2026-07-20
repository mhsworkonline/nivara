#!/usr/bin/env node
/*
 * deploy.js — stage, commit, and push the current changes to GitHub.
 *
 * Usage:
 *   node deploy.js                 # commit with an auto-generated message, then push
 *   node deploy.js "your message"  # commit with a custom message, then push
 *   node deploy.js -m "message"    # same, explicit flag
 *
 * Behaviour:
 *   - Runs from the repo root (this file's directory), regardless of cwd.
 *   - Stages everything (git add -A). .gitignore already excludes node_modules,
 *     target/, dist/, installers, and local settings.
 *   - If there is nothing to commit, it will still push any commits that are
 *     ahead of the remote.
 *   - Pushes the current branch to origin, setting upstream on first push.
 */

import { execFileSync } from "child_process";
import { fileURLToPath } from "url";
import { dirname } from "path";

const ROOT = dirname(fileURLToPath(import.meta.url));

// --- helpers ---------------------------------------------------------------
function git(args, { capture = true } = {}) {
  // execFileSync avoids shell-quoting issues (important for commit messages).
  return execFileSync("git", args, {
    cwd: ROOT,
    encoding: "utf8",
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
}

function tryGit(args) {
  try {
    return git(args).trim();
  } catch {
    return "";
  }
}

function fail(msg) {
  console.error(`\n✖ ${msg}`);
  process.exit(1);
}

// --- parse args ------------------------------------------------------------
function parseMessage(argv) {
  const args = argv.slice(2);
  const i = args.indexOf("-m");
  if (i !== -1 && args[i + 1]) return args[i + 1];
  const positional = args.filter((a) => a !== "-m");
  if (positional.length) return positional.join(" ");
  const stamp = new Date().toISOString().replace("T", " ").slice(0, 19);
  return `Deploy: update ${stamp}`;
}

// --- preflight -------------------------------------------------------------
if (!tryGit(["rev-parse", "--is-inside-work-tree"])) {
  fail("Not a git repository. Run this from inside the Nivara repo.");
}

const branch = tryGit(["rev-parse", "--abbrev-ref", "HEAD"]) || "HEAD";
const remoteUrl = tryGit(["remote", "get-url", "origin"]);
if (!remoteUrl) {
  fail('No "origin" remote configured. Add one with: git remote add origin <url>');
}

console.log(`Repo:   ${ROOT}`);
console.log(`Branch: ${branch}`);
console.log(`Origin: ${remoteUrl}`);

// --- stage -----------------------------------------------------------------
git(["add", "-A"]);

const staged = tryGit(["diff", "--cached", "--name-only"]);
if (staged) {
  console.log("\nStaged changes:");
  staged.split("\n").forEach((f) => console.log(`  ${f}`));

  const message = parseMessage(process.argv);
  console.log(`\nCommitting: "${message}"`);
  try {
    git(["commit", "-m", message], { capture: false });
  } catch {
    fail("git commit failed.");
  }
} else {
  console.log("\nNo file changes to commit.");
}

// --- push ------------------------------------------------------------------
// Is the local branch ahead of / not yet on the remote?
const hasUpstream = !!tryGit(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
const ahead = hasUpstream
  ? tryGit(["rev-list", "--count", "@{u}..HEAD"])
  : "unknown";

if (hasUpstream && ahead === "0") {
  console.log("\nNothing to push — already up to date with the remote.");
  process.exit(0);
}

console.log("\nPushing...");
try {
  if (hasUpstream) {
    git(["push", "origin", "HEAD"], { capture: false });
  } else {
    git(["push", "-u", "origin", branch], { capture: false });
  }
} catch {
  fail(
    "git push failed. If this is an authentication issue, ensure Git Credential " +
      "Manager or a valid token is set up for GitHub."
  );
}

const head = tryGit(["rev-parse", "HEAD"]);
const webUrl = remoteUrl.replace(/\.git$/, "");
console.log("\n✔ Deployed.");
console.log(`  Branch: ${branch}`);
console.log(`  Commit: ${head}`);
console.log(`  Repo:   ${webUrl}`);
