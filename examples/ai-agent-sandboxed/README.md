# Fully Sandboxed AI Agent

The entire agent process runs inside a zerobox sandbox. The OpenAI API key is passed as a **secret** — the process never sees the real key.

Compare with [`examples/ai-agent`](../ai-agent) where only individual tool calls are sandboxed but the agent process runs freely.

## How it works

```
zerobox \
  --secret OPENAI_API_KEY=sk-... \
  --secret-host OPENAI_API_KEY=api.openai.com \
  --allow-write=/tmp \
  -- node --use-env-proxy --import tsx/esm agent.ts
```

> **Note:** `--use-env-proxy` is required because Node.js `fetch` does not respect `HTTPS_PROXY` by default. This flag tells Node to route all HTTP/HTTPS through the proxy set in the environment, which is how zerobox intercepts requests for secret substitution.

Inside the sandbox:

| What | Value |
|------|-------|
| `process.env.OPENAI_API_KEY` | `ZEROBOX_SECRET_a1b2c3...` (placeholder) |
| HTTP header to `api.openai.com` | `Authorization: Bearer sk-...` (real key, injected by proxy) |
| HTTP header to any other host | `Authorization: Bearer ZEROBOX_SECRET_a1b2c3...` (useless placeholder) |
| File writes | Only `/tmp` allowed |
| Network | Only `api.openai.com` allowed |

The agent code uses the Vercel AI SDK (`generateText`) normally. The SDK reads `process.env.OPENAI_API_KEY` and sends it in the `Authorization` header. The proxy intercepts requests to `api.openai.com` and replaces the placeholder with the real key. The sandboxed process never has access to the actual API key.

## Setup

```bash
cd examples/ai-agent-sandboxed
pnpm install
```

## Run

```bash
OPENAI_API_KEY=sk-... pnpm start
```

## Expected output

```
  ✓ API key is a placeholder (not the real key)
  ✓ LLM call succeeded through proxy
  ✓ wrote output to /tmp
  ✓ writes outside /tmp are blocked

4 passed, 0 failed
```
