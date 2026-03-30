/**
 * Fully sandboxed AI agent.
 *
 * The ENTIRE process runs inside a zerobox sandbox. The OpenAI API key
 * is passed as a secret — this process never sees the real key, only a
 * placeholder. The proxy injects the real key for requests to api.openai.com.
 *
 * Compare with examples/ai-agent where only individual tools are sandboxed
 * but the agent process itself runs unsandboxed.
 *
 * Usage:
 *   OPENAI_API_KEY=sk-... pnpm start
 *
 * What happens:
 *   1. pnpm start runs: zerobox --secret OPENAI_API_KEY=sk-... --secret-host OPENAI_API_KEY=api.openai.com --allow-write=/tmp -- npx tsx agent.ts
 *   2. This process sees: OPENAI_API_KEY=ZEROBOX_SECRET_a1b2c3... (placeholder)
 *   3. When the AI SDK calls api.openai.com, the proxy substitutes the real key
 *   4. The process can write to /tmp but nowhere else
 *   5. The process can only reach api.openai.com (no other network)
 */

import { generateText } from "ai";
import { openai } from "@ai-sdk/openai";
import { writeFileSync, readFileSync } from "node:fs";

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

// Verify the key is a placeholder, not the real value.
const key = process.env.OPENAI_API_KEY ?? "";
assert(
  "API key is a placeholder (not the real key)",
  key.startsWith("ZEROBOX_SECRET_"),
  key.startsWith("sk-") ? "got real key — not running inside zerobox?" : undefined,
);

// Call the LLM. The AI SDK uses process.env.OPENAI_API_KEY which contains the
// placeholder. When the SDK sends the request to api.openai.com, the proxy
// intercepts and substitutes the real key in the Authorization header.
const result = await generateText({
  model: openai("gpt-4o-mini"),
  prompt: "Reply with exactly: SANDBOX_OK",
});

assert("LLM call succeeded through proxy", result.text.includes("SANDBOX_OK"), result.text);

// Write output to /tmp (allowed).
writeFileSync("/tmp/zerobox-sandboxed-agent-output.txt", result.text);
const written = readFileSync("/tmp/zerobox-sandboxed-agent-output.txt", "utf8");
assert("wrote output to /tmp", written === result.text);

// Try to write outside /tmp (should fail).
let writeBlocked = false;
try {
  writeFileSync("/var/zerobox-sandboxed-agent-test", "test");
} catch {
  writeBlocked = true;
}
assert("writes outside /tmp are blocked", writeBlocked);

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
