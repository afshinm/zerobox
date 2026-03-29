## Benchmark: sandbox overhead

Best of 10 runs (with warmup). Darwin arm64, 2026-03-29.

| Command | Bare (ms) | Sandboxed (ms) | Overhead | Bare Mem (KB) | Sandbox Mem (KB) |
|---------|-----------|----------------|----------|---------------|-----------------|
| echo hello                  |         0 |             10 |           +10ms |          1248 |            8576 |
| node -e '...'               |        10 |             20 |   +10ms (+100%) |         40144 |           40144 |
| python3 -c '...'            |        10 |             20 |   +10ms (+100%) |         13296 |           13312 |
| cat 10MB file               |         0 |             10 |           +10ms |          1984 |            8608 |
| curl https://example.com    |        50 |             60 |    +10ms (+20%) |          7360 |            8592 |
