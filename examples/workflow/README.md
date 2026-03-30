# Sandboxed Workflow Steps

A durable data pipeline using [Vercel Workflow](https://useworkflow.dev/) where each step runs its I/O inside a zerobox sandbox.

The workflow orchestrates five steps. Each step gets only the permissions it needs — reads, writes, or network — enforced at the OS level by zerobox. The `fetchPage` step demonstrates **secret management**: the API token is never visible inside the sandbox.

## Setup

```bash
cd examples/workflow
pnpm install
```

## Run

```bash
ZEROBOX_BIN=../../target/release/zerobox pnpm start
```

## How it works

Each step is a `"use step"` function. The Workflow runtime makes them durable and retryable. Zerobox makes them sandboxed:

| Step | Sandbox | What's allowed | What's blocked |
|------|---------|----------------|----------------|
| `readInput` | `Sandbox.create()` | Read any file | Writes, network |
| `fetchPage` | `Sandbox.create({ secrets: { API_TOKEN: { ... } } })` | httpbin.org with secret | Other domains, writes |
| `blockedFetch` | `Sandbox.create()` | Nothing | Writes, network |
| `transform` | `Sandbox.create()` | Read-only computation | Writes, network |
| `writeOutput` | `Sandbox.create({ allowWrite: ["/tmp"] })` | Write to /tmp | Other paths, network |

The `fetchPage` step uses secrets:

```ts
const network = Sandbox.create({
  secrets: {
    API_TOKEN: {
      value: "demo-secret-token",
      hosts: ["httpbin.org"],
    },
  },
});

async function fetchPage(url: string) {
  "use step";

  const output = await network
    .exec("curl", ["-s", "-H", "Authorization: Bearer $API_TOKEN", "-o", "/dev/null", "-w", "%{http_code}", url])
    .text();
  return { status: parseInt(output.trim(), 10) };
}
```

Inside the sandbox, `$API_TOKEN` contains a random placeholder. The proxy substitutes the real value only in HTTP headers sent to `httpbin.org`. The `"use step"` directive gives you durability. The `Sandbox` with secrets gives you isolation.

## Expected output

```
Running sandboxed workflow pipeline...

  step 1/5: read input (69 chars)
  step 2/5: fetched httpbin.org (HTTP 200)
  step 3/5: fetch without network permission: blocked
  step 4/5: transformed
  step 5/5: wrote output to /tmp/zerobox-wf-output.txt

Done: Workflow makes async functions durable. Zerobox makes each step safe. (verified: HTTP 200)
```

## Pipeline

```
readInput → fetchPage → blockedFetch → transform → writeOutput
  (read)    (secret+net)  (blocked)      (read)      (write)
```

If the process crashes mid-pipeline, the Workflow runtime replays from the last completed step. The sandbox ensures no step can exceed its permissions, even on retry.
