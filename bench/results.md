## Benchmark: sandbox overhead

Best of 10 runs (with warmup). Darwin arm64, 2026-03-30.

| Command | Bare (ms) | Sandboxed (ms) | Overhead | Bare Mem (KB) | Sandbox Mem (KB) |
|---------|-----------|----------------|----------|---------------|-----------------|
| echo hello                  |         0 |             10 |           +10ms |          1248 |            8768 |
| node -e '...'               |        10 |             20 |   +10ms (+100%) |         40112 |           40576 |
| python3 -c '...'            |        10 |             20 |   +10ms (+100%) |         13328 |           13248 |
| cat 10MB file               |         0 |             10 |           +10ms |          1968 |            8784 |
| curl https://example.com    |        50 |             60 |    +10ms (+20%) |          7472 |            8816 |
