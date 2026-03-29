/**
 * Durable data pipeline with sandboxed steps.
 *
 * Each step runs its I/O inside a zerobox sandbox with the minimum
 * permissions it needs. The Workflow runtime makes each step durable
 * and retryable — if the process crashes, it resumes from the last
 * completed step.
 *
 * Usage:
 *   ZEROBOX_BIN=../../target/release/zerobox pnpm start
 */

import { writeFileSync } from "node:fs";
import { Sandbox } from "zerobox";

// ── Sandboxes (one per permission profile) ──

const readOnly = Sandbox.create();
const writable = Sandbox.create({ allowWrite: ["/tmp"] });
const network = Sandbox.create({ allowNet: ["example.com"] });
const blocked = Sandbox.create(); // no network, no writes

// ── Steps ──
// Each step is an isolated, retryable unit of work.
// The "use step" directive makes the Workflow runtime record its
// inputs and outputs so replays skip already-completed steps.

async function readInput(path: string) {
  "use step";

  return await readOnly.js`
    const data = require("fs").readFileSync("${path}", "utf8");
    console.log(JSON.stringify({ content: data }));
  `.json<{ content: string }>();
}

async function fetchPage(url: string) {
  "use step";

  const output = await network
    .exec("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", url])
    .text();
  return { status: parseInt(output.trim(), 10) };
}

async function blockedFetch(url: string) {
  "use step";

  const result = await blocked
    .exec("curl", ["-s", "--max-time", "3", "-o", "/dev/null", "-w", "%{http_code}", url])
    .output();
  return { blocked: result.code !== 0 || result.stdout.trim() !== "200" };
}

async function transform(input: string, status: number) {
  "use step";

  const data = JSON.stringify({ input, status });
  return await readOnly.js`
    const d = JSON.parse('${data}');
    const summary = d.input.trim() + " (verified: HTTP " + d.status + ")";
    console.log(JSON.stringify({ summary }));
  `.json<{ summary: string }>();
}

async function writeOutput(path: string, content: string) {
  "use step";

  const safe = JSON.stringify(content);
  return await writable.js`
    require("fs").writeFileSync("${path}", ${safe});
    console.log(JSON.stringify({ written: true, path: "${path}" }));
  `.json<{ written: boolean; path: string }>();
}

// ── Workflow ──
// The "use workflow" directive makes this function durable.
// If the process restarts, execution resumes after the last
// completed step instead of re-running everything.

export async function pipeline(inputPath: string, outputPath: string) {
  "use workflow";

  const { content } = await readInput(inputPath);
  console.log("  step 1/5: read input (%d chars)", content.length);

  const { status } = await fetchPage("https://example.com");
  console.log("  step 2/5: fetched example.com (HTTP %d)", status);

  const { blocked } = await blockedFetch("https://example.com");
  console.log("  step 3/5: fetch without network permission: %s", blocked ? "blocked" : "allowed");

  const { summary } = await transform(content, status);
  console.log("  step 4/5: transformed");

  await writeOutput(outputPath, summary);
  console.log("  step 5/5: wrote output to %s", outputPath);

  return { summary };
}

// ── Run ──

const INPUT = "/tmp/zerobox-wf-input.txt";
const OUTPUT = "/tmp/zerobox-wf-output.txt";

writeFileSync(INPUT, "Workflow makes async functions durable. Zerobox makes each step safe.");

console.log("Running sandboxed workflow pipeline...\n");
const result = await pipeline(INPUT, OUTPUT);
console.log("\nDone:", result.summary);
