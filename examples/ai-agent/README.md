# AI Agent with Sandboxed Tools

An AI agent using Vercel AI SDK where each tool call runs inside its own zerobox sandbox.

The agent process runs normally. Only the dangerous operations (file I/O, network) are sandboxed with specific permissions. The `fetchUrl` tool demonstrates **secret management**: the API token is never visible inside the sandbox — the proxy injects the real value only for approved hosts.

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
| `fetchUrl` | `Sandbox.create({ secrets: { API_TOKEN: { value: "...", hosts: ["httpbin.org"] } } })` | Network to httpbin.org only via secret. Token injected by proxy. |

The `fetchUrl` tool uses secrets:

```ts
const fetcher = Sandbox.create({
  secrets: {
    API_TOKEN: {
      value: "demo-token-for-httpbin",
      hosts: ["httpbin.org"],
    },
  },
});
```

Inside the sandbox, `$API_TOKEN` contains a random placeholder. The proxy substitutes the real value only in HTTP headers sent to `httpbin.org`.

## Expected output

```
  ✓ readFile(/tmp/zerobox-demo-input.txt)
  ✓ writeFile(/tmp/zerobox-demo-output.txt)
  ✓ fetchUrl(https://httpbin.org/get)
  ✓ fetchUrl(https://example.com) blocked

=== Agent Response ===
...

4 passed, 0 failed
```
