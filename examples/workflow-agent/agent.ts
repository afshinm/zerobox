/**
 * Durable AI agent with sandboxed tools for Vercel Workflow.
 *
 * Each tool's execute function runs inside its own zerobox sandbox.
 * The workflow itself runs normally. Only the dangerous I/O operations
 * are sandboxed with specific permissions per step.
 *
 * In a real Workflow app, these tools would be marked with "use step"
 * for automatic retries and durability. This example shows the sandboxing
 * pattern that works with any Workflow step.
 *
 * Usage:
 *   OPENAI_API_KEY=sk-... ZEROBOX_BIN=../../target/release/zerobox npx tsx agent.ts
 */

import { generateText, tool } from "ai";
import { openai } from "@ai-sdk/openai";
import { writeFileSync } from "node:fs";
import { Sandbox } from "zerobox";
import { z } from "zod";

// ── Sandboxed tool implementations ──
//
// Each tool gets the minimum permissions it needs.
// In a Workflow app, wrap these in "use step" for durability.

const fileSandbox = Sandbox.create({ allowWrite: ["/tmp"] });
const apiSandbox = Sandbox.create({ allowNet: ["api.openai.com"] });
const webSandbox = Sandbox.create({ allowNet: ["example.com"] });

// Setup: create input file.
writeFileSync(
  "/tmp/zerobox-wf-input.txt",
  "Workflow makes async functions durable. Zerobox makes tool calls safe.",
);

// ── Agent definition ──

const result = await generateText({
  model: openai("gpt-4o-mini"),
  tools: {
    // Step 1: Read a file (read-only sandbox, default permissions)
    readFile: tool({
      description: "Read a file and return its contents",
      parameters: z.object({
        path: z.string().describe("Absolute path to the file"),
      }),
      execute: async ({ path }) => {
        const reader = Sandbox.create();
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

    // Step 2: Write a file (sandbox allows /tmp only)
    writeFile: tool({
      description: "Write content to a file. Only /tmp is writable.",
      parameters: z.object({
        path: z.string().describe("Absolute path (must be under /tmp)"),
        content: z.string().describe("Content to write"),
      }),
      execute: async ({ path, content }) => {
        const safe = JSON.stringify(content);
        const r = await fileSandbox.js`
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

    // Step 3: Fetch a URL (sandbox allows specific domain only)
    fetchUrl: tool({
      description: "Fetch a URL. Only example.com is reachable.",
      parameters: z.object({
        url: z.string().describe("URL to fetch"),
      }),
      execute: async ({ url }) => {
        const r = await webSandbox.js`
          fetch("${url}")
            .then(async (res) => {
              const body = await res.text();
              console.log(JSON.stringify({
                success: true,
                status: res.status,
                body: body.slice(0, 200),
              }));
            })
            .catch((e) => {
              console.log(JSON.stringify({ success: false, error: e.message }));
            });
        `.json<{ success: boolean; status?: number; body?: string; error?: string }>();
        return r;
      },
    }),

    // Step 4: Call OpenAI API (sandbox allows only api.openai.com)
    callApi: tool({
      description:
        "Make an API call to OpenAI. Only api.openai.com is reachable.",
      parameters: z.object({
        prompt: z.string().describe("Prompt to send to the API"),
      }),
      execute: async ({ prompt }) => {
        const key = process.env.OPENAI_API_KEY ?? "";
        const r = await apiSandbox.js`
          fetch("https://api.openai.com/v1/chat/completions", {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Authorization": "Bearer ${key}",
            },
            body: JSON.stringify({
              model: "gpt-4o-mini",
              messages: [{ role: "user", content: "${prompt}" }],
              max_tokens: 50,
            }),
          })
            .then(async (res) => {
              const data = await res.json();
              const text = data.choices?.[0]?.message?.content ?? "";
              console.log(JSON.stringify({ success: true, response: text }));
            })
            .catch((e) => {
              console.log(JSON.stringify({ success: false, error: e.message }));
            });
        `.json<{ success: boolean; response?: string; error?: string }>();
        return r;
      },
    }),
  },
  maxSteps: 15,
  prompt: `You are a workflow agent with sandboxed tools. Each tool runs in its own sandbox with limited permissions. Do the following steps in order:

1. Read the file at /tmp/zerobox-wf-input.txt
2. Write a one-sentence summary to /tmp/zerobox-wf-output.txt
3. Fetch https://example.com and report the HTTP status
4. Try to fetch https://evil.example.net (it should be blocked)
5. Use callApi to ask "What is 2+2?" (only api.openai.com is allowed)

Report the result of each step, noting which succeeded and which were blocked by the sandbox.`,
});

// ── Output ──

console.log("\n=== Agent Response ===");
console.log(result.text);

console.log("\n=== Tool Calls ===");
for (const step of result.steps) {
  for (const call of step.toolCalls) {
    const toolResult = step.toolResults.find(
      (r) => r.toolCallId === call.toolCallId,
    );
    const outcome = toolResult?.result as
      | { success: boolean; error?: string }
      | undefined;
    const icon = outcome?.success ? "✓" : "✗";
    const detail = outcome?.error ? ` → ${outcome.error}` : "";
    console.log(
      `  ${icon} ${call.toolName}(${JSON.stringify(call.args)})${detail}`,
    );
  }
}
