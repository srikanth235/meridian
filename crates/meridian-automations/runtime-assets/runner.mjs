// Symphony automation runner.
//
// Invoked by the meridian daemon to either:
//   describe <file>   → print {name, schedule} JSON for a script
//   run      <file>   → execute the script's default-exported automation
//
// Config flows in via env vars:
//   MERIDIAN_AUTOMATION_BASE   — http://127.0.0.1:<port>/api/automations/sdk
//   MERIDIAN_AUTOMATION_TOKEN  — per-run shared secret
//   MERIDIAN_AUTOMATION_RUN_ID — run row id (string)
//   MERIDIAN_AUTOMATION_DRY    — "1" if dry-run
//   MERIDIAN_LAST_RUN_AT       — ISO8601 of the previous successful run start, or empty
//
// Stdout/stderr are streamed back to the daemon and stored in the run log.

import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const [, , mode, fileArg] = process.argv;
if (!mode || !fileArg) {
  console.error("usage: runner.mjs <describe|run> <file>");
  process.exit(2);
}

let mod;
try {
  mod = await import(pathToFileURL(resolve(fileArg)).href);
} catch (err) {
  console.error(`[automation] failed to import ${fileArg}: ${err.message}`);
  process.exit(3);
}

const def = mod.default;
if (!def || typeof def !== "object") {
  console.error("[automation] script has no default export");
  process.exit(4);
}

if (mode === "describe") {
  const out = { name: def.name, schedule: def.schedule };
  process.stdout.write(JSON.stringify(out));
  process.exit(0);
}

if (mode !== "run") {
  console.error(`[automation] unknown mode: ${mode}`);
  process.exit(2);
}

if (typeof def.run !== "function") {
  console.error("[automation] default export has no run() function");
  process.exit(4);
}

const lastRunRaw = process.env.MERIDIAN_LAST_RUN_AT;
const ctx = {
  lastRunAt: lastRunRaw ? new Date(lastRunRaw) : null,
  dryRun: process.env.MERIDIAN_AUTOMATION_DRY === "1",
  log: (msg) => process.stdout.write(`[log] ${msg}\n`),
};

try {
  await def.run(ctx);
} catch (err) {
  console.error(`[automation] run() threw: ${err.stack || err.message || err}`);
  process.exit(1);
}
