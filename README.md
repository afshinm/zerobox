# sandbox

Cross-platform process sandboxing, built on [OpenAI Codex](https://github.com/openai/codex)'s
production sandboxing crates.

## Structure

```
sandbox/
  upstream/       Codex crates, copied verbatim by sync.sh (zero source modifications)
  shims/          Thin API-compatible replacements for heavy Codex deps (~250 LOC)
  cli/            zerobox-exec binary -- Deno-style sandbox CLI
  sync.sh         Pulls upstream crates from a Codex release
  UPSTREAM_VERSION  Pinned ref + commit SHA
```

## Usage

```bash
# Default: full read, no write, no network
zerobox-exec -- node -e "console.log('hello')"

# Allow writes to specific paths
zerobox-exec --allow-write=. -- node -e "require('fs').writeFileSync('out.txt','hi')"

# Allow network
zerobox-exec --allow-net -- curl https://example.com

# Restrict reads to specific paths
zerobox-exec --allow-read=/tmp,/data --allow-write=. -- python3 script.py

# No sandbox
zerobox-exec --allow-all -- bash -c "anything goes"
```

## Platform support

| Platform | Backend | Build | Test | Runtime |
|----------|---------|-------|------|---------|
| macOS | Seatbelt (`/usr/bin/sandbox-exec`) | yes | yes (43 tests) | yes |
| Linux | Bubblewrap + Seccomp + Namespaces | yes | unit tests only (see below) | yes |
| Windows | Restricted Tokens + ACLs + Firewall | yes | yes | untested |

## Shims

The Codex sandbox crates depend on `codex-core` (30+ transitive crates) and
`codex-network-proxy` (HTTP/SOCKS proxy runtime). We replace these with thin
shims that expose only the types the sandbox crates actually import:

| Shim | Replaces | Provides | Why |
|------|----------|----------|-----|
| `shims/core/` | `codex-core` | `error::{Result, CodexErr, SandboxErr}` | linux-sandbox uses 3 error types |
| `shims/network-proxy/` | `codex-network-proxy` | `NetworkProxy` struct + proxy env helpers | sandboxing uses struct + 5 functions |
| `shims/git-utils/` | `codex-git-utils` | `GitSha`, `GhostCommit` | protocol uses 2 types |
| `shims/rustls-provider/` | `codex-utils-rustls-provider` | empty | nothing references it |

## Known limitation: linux-sandbox integration tests

The upstream `linux-sandbox/tests/` integration tests import
`codex_core::exec::process_exec_tool_call` and related types from the full
Codex execution engine. Our `codex-core` shim intentionally does not provide
these (doing so would require pulling in 30+ crates).

**Impact**: `cargo test -p codex-linux-sandbox` will fail to compile on Linux.

**What works on Linux**:
- `cargo check` / `cargo build` -- the library and binary compile fine
- `cargo test --lib -p codex-linux-sandbox` -- inline unit tests pass
- Runtime sandboxing via `zerobox-exec` -- works

The integration tests exercise "run a command through Codex's exec pipeline and
verify the sandbox restricted it." That test path belongs to Codex's CI. Our
CLI (`zerobox-exec`) covers the same runtime behavior.

## Upgrading upstream

```bash
./sync.sh main            # or a specific tag/branch
cargo check               # compiles? done.
                          # fails? update shims/ to match API changes.
```

The script clones the specified ref, copies the 15 crates into `upstream/`,
applies one mechanical patch (`windows-sandbox-rs` Cargo.toml path dep →
workspace), and records the commit SHA in `UPSTREAM_VERSION`.
