/**
 * AI agent with sandboxed tools.
 *
 * Each tool call runs inside its own zerobox sandbox. The agent process
 * itself runs normally. Only the dangerous operations (file I/O, network)
 * are sandboxed with specific permissions.
 *
 * The fetchUrl tool demonstrates secrets: the API key is never visible
 * inside the sandbox. The proxy injects the real value only for requests
 * to the approved host.
 *
 * Usage:
 *   OPENAI_API_KEY=sk-... ZEROBOX_BIN=../../target/release/zerobox npx tsx agent.ts
 */

import { generateText, tool } from "ai";
import { openai } from "@ai-sdk/openai";
import { writeFileSync } from "node:fs";
import { Sandbox } from "zerobox";
import { z } from "zod";

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

// Each tool gets its own sandbox with the minimum permissions it needs.
const reader = Sandbox.create();
const writer = Sandbox.create({ allowWrite: ["/tmp"] });
const fetcher = Sandbox.create({
  secrets: {
    API_TOKEN: {
      value: "demo-token-for-httpbin",
      hosts: ["httpbin.org"],
    },
  },
});

// Setup: create input file for the agent to read.
writeFileSync("/tmp/zerobox-demo-input.txt", "Zerobox is a cross-platform process sandbox.");

const result = await generateText({
  model: openai("gpt-4o-mini"),
  tools: {
    readFile: tool({
      description: "Read a file from disk",
      parameters: z.object({ path: z.string() }),
      execute: async ({ path }) => {
        const r = await reader.js`
          try {
            const data = require("fs").readFileSync("${path}", "utf8");
            console.log(JSON.stringify({ success: true, content: data }));
          } catch (e) {
            console.log(JSON.stringify({ success: false, error: e.message }));
          }
        `.json<{ success: boolean; content?: string; error?: string }>();
        assert(`readFile(${path})`, r.success, r.error);
        return r;
      },
    }),

    writeFile: tool({
      description: "Write content to a file on disk",
      parameters: z.object({ path: z.string(), content: z.string() }),
      execute: async ({ path, content }) => {
        const safe = JSON.stringify(content);
        const r = await writer.js`
          try {
            require("fs").writeFileSync("${path}", ${safe});
            console.log(JSON.stringify({ success: true }));
          } catch (e) {
            console.log(JSON.stringify({ success: false, error: e.message }));
          }
        `.json<{ success: boolean; error?: string }>();
        assert(`writeFile(${path})`, r.success, r.error);
        return r;
      },
    }),

    fetchUrl: tool({
      description: "Fetch a URL and return the HTTP status code",
      parameters: z.object({ url: z.string() }),
      execute: async ({ url }) => {
        const result = await fetcher
          .exec("curl", [
            "-s",
            "--max-time",
            "5",
            "-H",
            "Authorization: Bearer $API_TOKEN",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            url,
          ])
          .output();
        const code = result.stdout.trim();
        const success = result.code === 0 && code === "200";
        // httpbin.org should succeed (secret host), anything else should fail.
        const isHttpbin = url.includes("httpbin.org");
        if (isHttpbin) {
          assert(`fetchUrl(${url})`, success, `HTTP ${code}`);
        } else {
          assert(`fetchUrl(${url}) blocked`, !success, `expected blocked, got HTTP ${code}`);
        }
        return success ? { success: true, status: 200 } : { success: false, error: "fetch failed" };
      },
    }),
  },
  maxSteps: 10,
  prompt: `You have access to file and network tools. Do the following in order:
1. Read the file at /tmp/zerobox-demo-input.txt
2. Write a one-line summary to /tmp/zerobox-demo-output.txt
3. Fetch https://httpbin.org/get and report the status code
4. Try to fetch https://example.com and report what happens (it should fail — the sandbox only allows httpbin.org)

After each step, report whether it succeeded or failed and why.`,
});

console.log("\n=== Agent Response ===");
console.log(result.text);

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
