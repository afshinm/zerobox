<div align="center">
  <h1>🫙 zerobox</h1>
  <p><strong>Run any command in a sandbox. Control what it can read, write, and connect to.</strong></p>
  <p>
    <a href="https://crates.io/crates/zerobox" target="_blank">
      <img src="https://img.shields.io/crates/v/zerobox?style=for-the-badge&labelColor=000000" alt="crates.io version" />
    </a>
    <a href="https://www.npmjs.com/package/zerobox" target="_blank">
      <img src="https://img.shields.io/npm/v/zerobox?style=for-the-badge&labelColor=000000" alt="npm version" />
    </a>
    <a href="https://github.com/afshinm/zerobox/blob/main/LICENSE" target="_blank">
      <img src="https://img.shields.io/github/license/afshinm/zerobox?style=for-the-badge&labelColor=000000" alt="license" />
    </a>
    <a href="https://github.com/afshinm/zerobox/actions/workflows/ci.yml" target="_blank">
      <img src="https://img.shields.io/github/actions/workflow/status/afshinm/zerobox/ci.yml?style=for-the-badge&labelColor=000000&label=CI" alt="CI status" />
    </a>
  </p>
</div>

## Overview

Cross-platform process sandboxing powered by [OpenAI Codex](https://github.com/openai/codex)'s production sandbox runtime. Uses seatbelt on macOS and bubblewrap + seccomp on Linux.

- 🔒 **Deny by default.** Writes and network are blocked unless you allow them.
- 📁 **File access control.** Allow or deny reads and writes to specific paths.
- 🌐 **Network filtering.** Allow or deny by domain, powered by a real HTTP/SOCKS proxy.
- 🧩 **TypeScript SDK.** `import { Sandbox } from "zerobox"` with a Deno-style API.
- 🖥️ **Cross-platform.** macOS, Linux, and Windows.
- 📦 **Single binary.** No runtime dependencies, no Docker, no VMs.

## Install

```bash
# Cargo
cargo install zerobox

# npm
npm install -g zerobox

# From source
git clone https://github.com/afshinm/zerobox && cd zerobox
./scripts/sync.sh && cargo build --release -p zerobox
```

## Quick start

```bash
# Writes and network are blocked by default
zerobox -- node -e "console.log('hello')"

# Allow writes to a directory
zerobox --allow-write=. -- node script.js

# Allow network to specific domains
zerobox --allow-net=api.openai.com -- node agent.js
```

## Examples

### Run AI-generated code safely

An LLM generates code. You need to execute it without risking file corruption, data exfiltration, or network abuse.

```bash
# LLM writes code to /tmp/task.py. Run it with no writes, no network.
zerobox -- python3 /tmp/task.py

# Allow writes only to an output directory
zerobox --allow-write=/tmp/output -- python3 /tmp/task.py

# Allow the script to call a specific API
zerobox --allow-write=/tmp/output --allow-net=api.openai.com -- python3 /tmp/task.py
```

Or via the TypeScript SDK:

```ts
import { Sandbox } from "zerobox";

const sandbox = Sandbox.create({
  allowWrite: ["/tmp/output"],
  allowNet: ["api.openai.com"],
});

const result = await sandbox.sh`python3 /tmp/task.py`.output();
console.log(result.code, result.stdout);
```

### Sandbox a browser agent

Use [LightPanda](https://lightpanda.io), a headless browser, for fully sandboxed web browsing. The agent can only reach the domains you allow.

```bash
# Fetch a page as markdown (only example.com is reachable)
zerobox --allow-net=example.com -- lightpanda fetch --dump markdown https://example.com

# Allow write access for saving results
zerobox --allow-net=example.com --allow-write=/tmp -- lightpanda fetch --dump html https://example.com
```

> **Note:** GUI browsers like Chrome and Firefox cannot run inside the sandbox. They require macOS WindowServer access and Unix socket IPC that the sandbox blocks by design. Use a headless engine like LightPanda, or run the browser outside the sandbox and connect via CDP.

### Restrict LLM tool calls

Each tool call can be sandboxed individually. The agent runs normally. Only the dangerous operations are sandboxed.

```ts
import { Sandbox } from "zerobox";

// Each tool gets its own sandbox with minimum permissions.
const reader = Sandbox.create();                               // read-only
const writer = Sandbox.create({ allowWrite: ["/tmp"] });       // writes to /tmp
const fetcher = Sandbox.create({ allowNet: ["example.com"] }); // one domain

// Read a file inside the sandbox
const data = await reader.js`
  const content = require("fs").readFileSync("/tmp/input.txt", "utf8");
  console.log(JSON.stringify({ content }));
`.json();

// Write a file (only /tmp is writable)
await writer.js`
  require("fs").writeFileSync("/tmp/output.txt", "result");
  console.log("ok");
`.text();

// Fetch a URL (only example.com is reachable)
const result = await fetcher.js`
  const res = await fetch("https://example.com");
  console.log(JSON.stringify({ status: res.status }));
`.json();
```

Full working examples:
- [`examples/ai-agent`](examples/ai-agent) -- Vercel AI SDK with sandboxed tools
- [`examples/workflow`](examples/workflow) -- [Vercel Workflow](https://useworkflow.dev/) with sandboxed durable steps

### Protect your repo during builds

Run package installs and build scripts without risking your `.git` history or config files.

```bash
# npm install can write to node_modules but not .git or .env
zerobox --allow-write=./node_modules,./package-lock.json --deny-write=./.git,./.env -- npm install

# Run a build script with network access for downloading deps
zerobox --allow-write=./dist --allow-net -- npm run build

# Run tests with no network (catch accidental external calls)
zerobox --allow-write=/tmp -- npm test
```

## SDK (TypeScript)

```bash
npm install zerobox
```

```ts
import { Sandbox } from "zerobox";

const sandbox = Sandbox.create({
  allowWrite: ["/tmp"],
  allowNet: ["example.com"],
});

// Shell commands via tagged template
const output = await sandbox.sh`echo hello`.text();

// Parse JSON output
const data = await sandbox.sh`cat data.json`.json();

// Raw output (doesn't throw on non-zero exit)
const result = await sandbox.sh`exit 42`.output();
// { code: 42, stdout: "", stderr: "" }

// Explicit command + args
await sandbox.exec("node", ["-e", "console.log('hi')"]).text();

// Cancellation
const controller = new AbortController();
await sandbox.sh`sleep 60`.text({ signal: controller.signal });
```

Non-zero exit codes throw `SandboxCommandError`:

```ts
import { Sandbox, SandboxCommandError } from "zerobox";

const sandbox = Sandbox.create();
try {
  await sandbox.sh`exit 1`.text();
} catch (e) {
  if (e instanceof SandboxCommandError) {
    console.log(e.code);   // 1
    console.log(e.stderr);  // error output
  }
}
```

## Performance

Sandbox overhead is minimal, typically ~10ms and ~7MB:

| Command | Bare | Sandboxed | Overhead | Bare Mem | Sandbox Mem |
|---------|------|-----------|----------|----------|-------------|
| `echo hello` | <1ms | 10ms | +10ms | 1.2 MB | 8.4 MB |
| `node -e '...'` | 10ms | 20ms | +10ms | 39.3 MB | 39.1 MB |
| `python3 -c '...'` | 10ms | 20ms | +10ms | 12.9 MB | 13.0 MB |
| `cat 10MB file` | <1ms | 10ms | +10ms | 1.9 MB | 8.4 MB |
| `curl https://...` | 50ms | 60ms | +10ms | 7.2 MB | 8.4 MB |

<sub>Best of 10 runs with warmup, Apple M5 Pro. The ~7MB memory overhead is the sandbox-exec process. For runtimes like Node/Python, the runtime itself dominates memory. Run `./bench/run.sh` to reproduce.</sub>

## Platform support

| Platform | Backend | Status |
|----------|---------|--------|
| macOS | Seatbelt (`sandbox-exec`) | Fully supported |
| Linux | Bubblewrap + Seccomp + Namespaces | Fully supported |
| Windows | Restricted Tokens + ACLs + Firewall | Supported (not yet tested in CI) |

## CLI reference

| Flag | Example | Description |
|------|---------|-------------|
| `--allow-read <paths>` | `--allow-read=/tmp,/data` | Restrict readable user data to listed paths. System libraries remain accessible. Default: all reads allowed. |
| `--deny-read <paths>` | `--deny-read=/secret` | Block reading from these paths. Takes precedence over `--allow-read`. |
| `--allow-write [paths]` | `--allow-write=.` | Allow writing to these paths. Without a value, allows writing everywhere. Default: no writes. |
| `--deny-write <paths>` | `--deny-write=./.git` | Block writing to these paths. Takes precedence over `--allow-write`. |
| `--allow-net [domains]` | `--allow-net=example.com` | Allow outbound network. Without a value, allows all domains. Default: no network. |
| `--deny-net <domains>` | `--deny-net=evil.com` | Block network to these domains. Takes precedence over `--allow-net`. |
| `-A`, `--allow-all` | `-A` | Grant all permissions. No sandbox enforcement. |
| `--no-sandbox` | `--no-sandbox` | Disable the sandbox entirely. |
| `-C <dir>` | `-C /workspace` | Set working directory for the sandboxed command. |
| `-V`, `--version` | `--version` | Print version. |
| `-h`, `--help` | `--help` | Print help. |

## License

Apache-2.0
