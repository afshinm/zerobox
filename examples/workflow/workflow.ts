/**
 * Durable data pipeline with sandboxed steps.
 *
 * Each step runs its I/O inside a zerobox sandbox with the minimum
 * permissions it needs. The Workflow runtime makes each step durable
 * and retryable — if the process crashes, it resumes from the last
 * completed step.
 *
 * The fetchPage step demonstrates secrets: the API token is never
 * visible inside the sandbox. The proxy injects it for httpbin.org only.
 *
 * Usage:
 *   ZEROBOX_BIN=../../target/release/zerobox pnpm start
 */

import { writeFileSync } from "node:fs";
import { Sandbox } from "zerobox";

let passed = 0;
let failed = 0;

function assert(name: string, condition: boolean, detail?: string) {
  if (condition) {
    console.log(`  \x1b[32m✓\x1b[0m ${name}`);
    passed++;
  } else {
    console.log(`  \x1b[31m✗\x1b[0m ${name}${detail ? ` (${detail})` : ""}`);
    failed++;
  }
}

// ── Sandboxes (one per permission profile) ──

const readOnly = Sandbox.create();
const writable = Sandbox.create({ allowWrite: ["/tmp"] });
const network = Sandbox.create({
  secrets: {
    API_TOKEN: {
      value: "demo-secret-token",
      hosts: ["httpbin.org"],
    },
  },
});
const blocked = Sandbox.create(); // no network, no writes

// ── Steps ──

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
    .exec("curl", [
      "-s",
      "-H",
      "Authorization: Bearer $API_TOKEN",
      "-o",
      "/dev/null",
      "-w",
      "%{http_code}",
      url,
    ])
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

export async function pipeline(inputPath: string, outputPath: string) {
  "use workflow";

  const { content } = await readInput(inputPath);
  assert("read input", content.length > 0, `got ${content.length} chars`);

  const { status } = await fetchPage("https://httpbin.org/get");
  assert("fetch httpbin.org with secret", status === 200, `HTTP ${status}`);

  const { blocked: isBlocked } = await blockedFetch("https://httpbin.org/get");
  assert("fetch without permission is blocked", isBlocked);

  const { summary } = await transform(content, status);
  assert("transform produces summary", summary.length > 0);

  const result = await writeOutput(outputPath, summary);
  assert("write output", result.written);

  return { summary };
}

// ── Run ──

const INPUT = "/tmp/zerobox-wf-input.txt";
const OUTPUT = "/tmp/zerobox-wf-output.txt";

writeFileSync(INPUT, "Workflow makes async functions durable. Zerobox makes each step safe.");

console.log("Running sandboxed workflow pipeline...\n");
const result = await pipeline(INPUT, OUTPUT);

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
