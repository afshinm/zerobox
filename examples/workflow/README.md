# Sandboxed Workflow Steps

A durable data pipeline using [Vercel Workflow](https://useworkflow.dev/) where each step runs its I/O inside a zerobox sandbox.

The workflow orchestrates five steps. Each step gets only the permissions it needs — reads, writes, or network — enforced at the OS level by zerobox.

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
| `fetchPage` | `Sandbox.create({ allowNet: ["example.com"] })` | Fetch example.com | Other domains, writes |
| `blockedFetch` | `Sandbox.create()` | Nothing | Writes, network |
| `transform` | `Sandbox.create()` | Read-only computation | Writes, network |
| `writeOutput` | `Sandbox.create({ allowWrite: ["/tmp"] })` | Write to /tmp | Other paths, network |

```ts
async function readInput(path: string) {
  "use step";

  return await readOnly.js`
    const data = require("fs").readFileSync("${path}", "utf8");
    console.log(JSON.stringify({ content: data }));
  `.json();
}
```

The `"use step"` directive gives you durability (automatic retries, crash recovery). The `Sandbox` gives you isolation (least-privilege I/O). They compose naturally — the sandbox runs inside the step.

## Expected output

```
Running sandboxed workflow pipeline...

  step 1/5: read input (69 chars)
  step 2/5: fetched example.com (HTTP 200)
  step 3/5: fetch without network permission: blocked
  step 4/5: transformed
  step 5/5: wrote output to /tmp/zerobox-wf-output.txt

Done: Workflow makes async functions durable. Zerobox makes each step safe. (verified: HTTP 200)
```

## Pipeline

```
readInput → fetchPage → blockedFetch → transform → writeOutput
  (read)     (network)    (blocked)      (read)      (write)
```

If the process crashes mid-pipeline, the Workflow runtime replays from the last completed step. The sandbox ensures no step can exceed its permissions, even on retry.
