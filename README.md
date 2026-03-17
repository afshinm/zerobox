# zerobox

Firecracker microVM sandbox supervisor for AI agents. Single Rust binary, REST API, TypeScript SDK.

## Quick Start

```bash
# Start the daemon (requires Linux with /dev/kvm)
sudo zerobox serve --config config.example.yaml

# Create a sandbox
zerobox start --image base
# sbx_a5024a87bf0a (status: running)

# Run commands inside it
zerobox exec sbx_a5024a87bf0a -- uname -a
# Linux localhost 6.1.166 #3 SMP aarch64 GNU/Linux

zerobox exec sbx_a5024a87bf0a -- echo hello from firecracker
# hello from firecracker

# Interactive shell
zerobox connect sbx_a5024a87bf0a

# Snapshot, stop, destroy
zerobox snapshot create sbx_a5024a87bf0a
zerobox stop sbx_a5024a87bf0a
zerobox destroy sbx_a5024a87bf0a

# List everything
zerobox list
zerobox snapshot list
```

## REST API

```bash
curl -X POST http://localhost:7000/v1/sandboxes \
  -H 'Content-Type: application/json' \
  -d '{"source":{"type":"image","image":"base"},"timeout":300000}'

curl http://localhost:7000/v1/sandboxes

curl -X POST http://localhost:7000/v1/sandboxes/sbx_abc123/commands \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"ls","args":["/"]}'
```

## Build

```bash
cargo build --release          # daemon + CLI
cargo test                     # unit tests
./test.sh                      # API + guest agent tests (macOS ok)
./test-e2e.sh                  # full VM lifecycle (needs Lima)
```

## macOS Development

Firecracker needs Linux/KVM. On Apple Silicon, use Lima:

```bash
brew install lima
./test-e2e.sh   # auto-provisions a Lima VM with nested virt
```

## License

Apache-2.0
