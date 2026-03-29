# Workflow Agent with Sandboxed Tools

A durable AI agent using [Vercel Workflow](https://useworkflow.dev/) patterns where each tool call runs inside its own zerobox sandbox.

The agent process runs normally. Only the tool executions (file I/O, network calls, API calls) are sandboxed with specific permissions per step.

## Setup

```bash
cd examples/workflow-agent
pnpm install
```

## Run

```bash
OPENAI_API_KEY=sk-... ZEROBOX_BIN=../../target/release/zerobox pnpm start
```

## How it works

The agent has four tools, each with its own sandbox:

| Tool | Sandbox | What's allowed | What's blocked |
|------|---------|----------------|----------------|
| `readFile` | `Sandbox.create()` | Read any file | Writes, network |
| `writeFile` | `Sandbox.create({ allowWrite: ["/tmp"] })` | Write to /tmp | Other paths, network |
| `fetchUrl` | `Sandbox.create({ allowNet: ["example.com"] })` | Fetch example.com | Other domains, writes |
| `callApi` | `Sandbox.create({ allowNet: ["api.openai.com"] })` | Call OpenAI API | Other domains, writes |

Each tool uses `sandbox.js` to run inline JavaScript inside the sandbox:

```ts
const webSandbox = Sandbox.create({ allowNet: ["example.com"] });

const result = await webSandbox.js`
  const res = await fetch("https://example.com");
  console.log(JSON.stringify({ status: res.status }));
`.json();
```

In a real Workflow app, each tool's `execute` function would be marked with `"use step"` for automatic retries and durability. The sandbox wrapping works the same way.

## Expected output

```
✓ readFile({"path":"/tmp/zerobox-wf-input.txt"})
✓ writeFile({"path":"/tmp/zerobox-wf-output.txt","content":"..."})
✓ fetchUrl({"url":"https://example.com"})
✗ fetchUrl({"url":"https://evil.example.net"}) → fetch failed
✓ callApi({"prompt":"What is 2+2?"})
```
