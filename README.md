# zerobox

Firecracker microVM sandbox supervisor for AI agents. Single Rust binary, REST API, TypeScript SDK.

Each sandbox is a lightweight VM with its own kernel — hardware-level isolation, sub-second boot, snapshot/restore.

## Install

```bash
# Linux (Ubuntu/Debian)
sudo ./setup.sh

# macOS (auto-provisions a Lima VM)
./setup.sh
```

Or with the generated installer (after a release):

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/afshinm/zerobox/releases/latest/download/zerobox-daemon-installer.sh | sh
```

Or from `.deb` package:

```bash
sudo dpkg -i zerobox-daemon_0.1.0_amd64.deb
```

## Quick Start

```bash
# Start the daemon (setup.sh does this automatically via systemd)
sudo systemctl start zerobox

# Create a sandbox
zerobox start --image base
# sbx_a5024a87bf0a (status: running)

# Run commands inside it
zerobox exec sbx_a5024a87bf0a -- uname -a
# Linux localhost 6.1.166 #3 SMP aarch64 GNU/Linux

zerobox exec sbx_a5024a87bf0a -- ls /
# bin boot dev etc home lib ...

# Interactive shell
zerobox connect sbx_a5024a87bf0a
# zerobox:a5024a87$ whoami
# root
# zerobox:a5024a87$ ^D

# Snapshot and restore
zerobox snapshot create sbx_a5024a87bf0a
zerobox snapshot list

# Clean up
zerobox stop sbx_a5024a87bf0a
zerobox destroy sbx_a5024a87bf0a
```

## REST API

All CLI commands map to the REST API at `http://localhost:7000/v1`:

```bash
# Create
curl -X POST http://localhost:7000/v1/sandboxes \
  -H 'Content-Type: application/json' \
  -d '{"source":{"type":"image","image":"base"},"timeout":300000}'

# List
curl http://localhost:7000/v1/sandboxes

# Execute
curl -X POST http://localhost:7000/v1/sandboxes/sbx_abc/commands \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"echo","args":["hello"]}'

# Destroy
curl -X DELETE http://localhost:7000/v1/sandboxes/sbx_abc
```

## Build from Source

```bash
cargo build --release      # daemon + CLI + guest agent
cargo test                 # unit tests (12 tests)
cargo clippy -- -D warnings
```

## Test

```bash
./test.sh       # API + guest agent tests (runs on macOS, no VM needed)
./test-e2e.sh   # full Firecracker VM lifecycle (provisions Lima VM)
```

## Architecture

```
CLI / TypeScript SDK
        |  HTTP
   zerobox daemon (Rust, axum)
        |
   Firecracker microVMs
        |  vsock
   guest agent (Rust, inside each VM)
```

- **zerobox-daemon** — REST API server + CLI. Manages VM lifecycle, networking, snapshots.
- **zerobox-guest-agent** — Runs inside each VM. Executes commands, handles file I/O over vsock.
- **zerobox-common** — Shared types and JSON-RPC protocol definitions.

## Configuration

Default config at `/etc/zerobox/config.yaml`:

```yaml
listen: "0.0.0.0:7000"
data_dir: "/var/lib/zerobox"

firecracker:
  binary: "/usr/local/bin/firecracker"
  default_vcpus: 2
  default_memory_mib: 512

networking:
  bridge: "zerobox-br0"
  subnet: "10.20.0.0/16"
```

## Packaging

```bash
# Generate .deb with systemd service
cargo deb -p zerobox-daemon

# Release binaries + install script (via cargo-dist)
git tag v0.1.0 && git push --tags
```

## License

Apache-2.0
