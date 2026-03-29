# AI Agent with Sandboxed Tools

An AI agent using Vercel AI SDK where each tool call runs inside its own zerobox sandbox.

The agent process runs normally. Only the dangerous operations (file I/O, network) are sandboxed with specific permissions per tool.

## Setup

```bash
cd examples/ai-agent
pnpm install
```

## Run

```bash
OPENAI_API_KEY=sk-... ZEROBOX_BIN=../../target/release/zerobox pnpm start
```

## How it works

The agent has three tools, each with its own sandbox:

| Tool | Sandbox | Permissions |
|------|---------|-------------|
| `readFile` | `Sandbox.create()` | Read-only (default). No writes, no network. |
| `writeFile` | `Sandbox.create({ allowWrite: ["/tmp"] })` | Writes to `/tmp` only. No network. |
| `fetchUrl` | `Sandbox.create({ allowNet: ["example.com"] })` | Network to `example.com` only. No writes. |

Each tool uses `sandbox.js` to run inline JavaScript inside the sandbox:

```ts
const reader = Sandbox.create();
const data = await reader.js`
  const content = require("fs").readFileSync("/tmp/input.txt", "utf8");
  console.log(JSON.stringify({ content }));
`.json();
```

The agent asks the LLM to read a file, write a summary, fetch a URL, and try to fetch a blocked domain. The sandbox enforces the permissions at the OS level.

## Expected output

```
✓ readFile({"path":"/tmp/zerobox-demo-input.txt"})
✓ writeFile({"path":"/tmp/zerobox-demo-output.txt","content":"..."})
✓ fetchUrl({"url":"https://example.com"})
✗ fetchUrl({"url":"https://evil.example.net"}) → fetch failed
```
