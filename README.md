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

Cross-platform process sandboxing powered by [OpenAI Codex](https://github.com/openai/codex)'s production sandbox runtime — seatbelt on macOS, bubblewrap + seccomp on Linux.

- 🔒 **Deny by default** — writes and network blocked unless you allow them
- 📁 **File access control** — allow/deny reads and writes to specific paths
- 🌐 **Network filtering** — allow/deny by domain, powered by a real HTTP/SOCKS proxy
- 🧩 **TypeScript SDK** — `import { Sandbox } from "zerobox"` with Deno-style API
- 🖥️ **Cross-platform** — macOS, Linux, and Windows
- 📦 **Single binary** — no runtime dependencies, no Docker, no VMs

## Install

```bash
# Cargo
cargo install zerobox

# npm
npm install -g zerobox

# From source
git clone https://github.com/afshinm/zerobox && cd zerobox
./sync.sh && cargo build --release -p zerobox
```

## Usage

```bash
# Run a command — writes and network blocked by default
zerobox -- node -e "console.log('hello')"

# Allow writes to specific paths
zerobox --allow-write=. -- node script.js

# Protect .git from writes
zerobox --allow-write=. --deny-write=./.git -- npm install

# Allow network to specific domains
zerobox --allow-net=example.com,api.example.com -- node fetch.js

# Allow all network, block specific domains
zerobox --allow-net --deny-net=evil.com -- node server.js

# Restrict reads (system libraries still accessible)
zerobox --allow-read=/tmp --allow-write=/tmp -- python3 script.py

# No sandbox (escape hatch)
zerobox --allow-all -- bash -c "anything goes"
```

## Sandboxing a browser agent

Use [LightPanda](https://lightpanda.io) (headless browser) for fully sandboxed web browsing:

```bash
# Fetch a page — only example.com is reachable
zerobox --allow-net=example.com -- lightpanda fetch --dump markdown https://example.com

# With write access for downloads
zerobox --allow-net=example.com --allow-write=/tmp -- lightpanda fetch --dump html https://example.com
```

> **Note:** GUI browsers (Chrome, Firefox) cannot run inside the sandbox — they need macOS WindowServer access and Unix socket IPC that the sandbox blocks by design. Use a headless engine like LightPanda, or run the browser outside the sandbox and connect via CDP.

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

## CLI reference

| Flag | Example | Effect |
|------|---------|--------|
| `--allow-read=<paths>` | `--allow-read=/tmp,/data` | Restrict reads to listed paths (default: all reads allowed) |
| `--deny-read=<paths>` | `--deny-read=/secret` | Block reads (takes precedence over allow) |
| `--allow-write[=<paths>]` | `--allow-write=.` | Allow writes (default: no writes) |
| `--deny-write=<paths>` | `--deny-write=./.git` | Block writes (takes precedence over allow) |
| `--allow-net[=<domains>]` | `--allow-net=example.com` | Allow network (default: no network) |
| `--deny-net=<domains>` | `--deny-net=evil.com` | Block domains (takes precedence over allow) |
| `--allow-all` / `-A` | `-A` | Disable sandbox entirely |
| `--no-sandbox` | `--no-sandbox` | Same as --allow-all |
| `-C <dir>` | `-C /workspace` | Set working directory |

## Platform support

| Platform | Backend | Status |
|----------|---------|--------|
| macOS | Seatbelt (`sandbox-exec`) | Fully supported |
| Linux | Bubblewrap + Seccomp + Namespaces | Fully supported |
| Windows | Restricted Tokens + ACLs + Firewall | Supported (not yet tested in CI) |

## License

Apache-2.0
