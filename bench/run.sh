#!/usr/bin/env bash
#
# Benchmark zerobox sandbox overhead.
#
# Usage:
#   ./bench/run.sh                         # uses ./target/release/zerobox
#   ZEROBOX_BIN=/path/to/zerobox ./bench/run.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."
ZEROBOX="${ZEROBOX_BIN:-$ROOT/target/release/zerobox}"
RUNS="${BENCH_RUNS:-10}"
RESULTS_FILE="$SCRIPT_DIR/results.md"
TMP_TIME=$(mktemp)
BENCH_FILE=$(mktemp)
IS_DARWIN=$([[ "$(uname)" == "Darwin" ]] && echo 1 || echo 0)

trap 'rm -f "$TMP_TIME" "$BENCH_FILE"' EXIT

if [ ! -x "$ZEROBOX" ]; then
  echo "error: zerobox binary not found at $ZEROBOX"
  echo "Build it first: cargo build --release -p zerobox"
  exit 1
fi

# Create a 10MB test file for I/O benchmark.
dd if=/dev/zero bs=1024 count=10240 of="$BENCH_FILE" 2>/dev/null

# ── Measurement ──
# Returns: wall_time_ms peak_memory_kb
# Takes best (minimum) time and peak (maximum) memory across $RUNS runs.
measure() {
  local cmd="$1"
  local best_time=999999
  local peak_mem=0

  for _ in $(seq 1 "$RUNS"); do
    local time_ms=0 mem_kb=0

    if [ "$IS_DARWIN" = "1" ]; then
      /usr/bin/time -l bash -c "$cmd" > /dev/null 2> "$TMP_TIME" || true
      local real_sec
      real_sec=$(awk '/real/{print $1}' "$TMP_TIME")
      time_ms=$(awk "BEGIN{printf \"%d\", ${real_sec:-0} * 1000}")
      local mem_bytes
      mem_bytes=$(awk '/maximum resident/{print $1}' "$TMP_TIME")
      mem_kb=$(( ${mem_bytes:-0} / 1024 ))
    else
      /usr/bin/time -v bash -c "$cmd" > /dev/null 2> "$TMP_TIME" || true
      local elapsed
      elapsed=$(grep "Elapsed" "$TMP_TIME" | awk '{print $NF}')
      time_ms=$(echo "$elapsed" | awk -F'[:.]' '{
        if (NF==3) printf "%d", ($1*60 + $2)*1000 + $3*10;
        else if (NF==4) printf "%d", ($1*3600 + $2*60 + $3)*1000;
      }')
      mem_kb=$(grep "Maximum resident" "$TMP_TIME" | awk '{print $NF}')
    fi

    time_ms=${time_ms:-0}
    mem_kb=${mem_kb:-0}

    [ "$time_ms" -lt "$best_time" ] && best_time=$time_ms
    [ "$mem_kb" -gt "$peak_mem" ] && peak_mem=$mem_kb
  done

  echo "$best_time $peak_mem"
}

# ── Run benchmarks ──

declare -a NAMES=()
declare -a BARE_RESULTS=()
declare -a SANDBOX_RESULTS=()

bench() {
  local name="$1" bare="$2" sandboxed="$3"
  printf "  %-30s" "$name"
  # Warmup both commands so neither benefits from cold caches.
  bash -c "$bare" > /dev/null 2>&1 || true
  bash -c "$sandboxed" > /dev/null 2>&1 || true
  local b s
  b=$(measure "$bare")
  s=$(measure "$sandboxed")
  echo "done"
  NAMES+=("$name")
  BARE_RESULTS+=("$b")
  SANDBOX_RESULTS+=("$s")
}

echo "Benchmarking zerobox overhead ($RUNS runs each, best time, peak memory)..."
echo ""

# Use /bin/echo (not shell builtin) for fair comparison.
bench "echo hello" \
  "/bin/echo hello" \
  "$ZEROBOX -- /bin/echo hello"

bench "node -e '...'" \
  "node -e 'console.log(1)'" \
  "$ZEROBOX -- node -e 'console.log(1)'"

bench "python3 -c '...'" \
  "python3 -c 'print(1)'" \
  "$ZEROBOX -- python3 -c 'print(1)'"

bench "cat 10MB file" \
  "cat $BENCH_FILE > /dev/null" \
  "$ZEROBOX -- sh -c 'cat $BENCH_FILE > /dev/null'"

bench "curl https://example.com" \
  "curl -s -o /dev/null https://example.com" \
  "$ZEROBOX --allow-net -- curl -s -o /dev/null https://example.com"

# ── Output ──

echo ""

{
  echo "## Benchmark: sandbox overhead"
  echo ""
  echo "Best of $RUNS runs (with warmup). $(uname -s) $(uname -m), $(date -u +%Y-%m-%d)."
  echo ""
  echo "| Command | Bare (ms) | Sandboxed (ms) | Overhead | Bare Mem (KB) | Sandbox Mem (KB) |"
  echo "|---------|-----------|----------------|----------|---------------|-----------------|"

  for i in "${!NAMES[@]}"; do
    bt=$(echo "${BARE_RESULTS[$i]}" | awk '{print $1}')
    bm=$(echo "${BARE_RESULTS[$i]}" | awk '{print $2}')
    st=$(echo "${SANDBOX_RESULTS[$i]}" | awk '{print $1}')
    sm=$(echo "${SANDBOX_RESULTS[$i]}" | awk '{print $2}')

    if [ "$bt" -gt 0 ]; then
      diff=$((st - bt))
      pct=$(( diff * 100 / bt ))
      overhead="+${diff}ms (+${pct}%)"
    else
      diff=$((st - bt))
      overhead="+${diff}ms"
    fi

    printf "| %-27s | %9s | %14s | %15s | %13s | %15s |\n" \
      "${NAMES[$i]}" "$bt" "$st" "$overhead" "$bm" "$sm"
  done
} | tee "$RESULTS_FILE"

echo ""
echo "Saved to $RESULTS_FILE"
