/**
 * AI agent with sandboxed tools.
 *
 * Each tool call runs inside its own zerobox sandbox. The agent process
 * itself runs normally. Only the dangerous operations (file I/O, network)
 * are sandboxed with specific permissions.
 *
 * Usage:
 *   OPENAI_API_KEY=sk-... ZEROBOX_BIN=../../target/release/zerobox npx tsx agent.ts
 */

import { generateText, tool } from "ai";
import { openai } from "@ai-sdk/openai";
import { writeFileSync } from "node:fs";
import { Sandbox } from "zerobox";
import { z } from "zod";

// Each tool gets its own sandbox with the minimum permissions it needs.
const reader = Sandbox.create(); // read-only (default)
const writer = Sandbox.create({ allowWrite: ["/tmp"] }); // writes to /tmp only
const fetcher = Sandbox.create({ allowNet: ["example.com"] }); // one domain only

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
        return r;
      },
    }),

    fetchUrl: tool({
      description: "Fetch a URL and return the status and body preview",
      parameters: z.object({ url: z.string() }),
      execute: async ({ url }) => {
        const r = await fetcher.js`
          fetch("${url}")
            .then(async (res) => {
              const body = await res.text();
              console.log(JSON.stringify({
                success: true,
                status: res.status,
                body: body.slice(0, 300),
              }));
            })
            .catch((e) => {
              console.log(JSON.stringify({ success: false, error: e.message }));
            });
        `.json<{ success: boolean; status?: number; body?: string; error?: string }>();
        return r;
      },
    }),
  },
  maxSteps: 10,
  prompt: `You have access to file and network tools. Do the following in order:
1. Read the file at /tmp/zerobox-demo-input.txt
2. Write a one-line summary to /tmp/zerobox-demo-output.txt
3. Fetch https://example.com and report the status code
4. Try to fetch https://evil.example.net and report what happens

After each step, report whether it succeeded or failed and why.`,
});

console.log("\n=== Agent Response ===");
console.log(result.text);

console.log("\n=== Tool Calls ===");
for (const step of result.steps) {
  for (const call of step.toolCalls) {
    const toolResult = step.toolResults.find((r) => r.toolCallId === call.toolCallId);
    const outcome = toolResult?.result as
      | { success: boolean; error?: string }
      | undefined;
    const status = outcome?.success ? "✓" : "✗";
    const detail = outcome?.error ? ` → ${outcome.error}` : "";
    console.log(`  ${status} ${call.toolName}(${JSON.stringify(call.args)})${detail}`);
  }
}
