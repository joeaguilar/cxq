#!/usr/bin/env bash
set -euo pipefail

BIN=${1:-target/debug/cxq}
ROOT=$(cd "$(dirname "$0")/fixtures/home" && pwd)
PASS=0
FAIL=0

ok() {
  PASS=$((PASS + 1))
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  FAIL=$((FAIL + 1))
}

contains() {
  local label=$1 needle=$2 body=$3
  if [[ "$body" == *"$needle"* ]]; then ok; else fail "$label"; fi
}

excludes() {
  local label=$1 needle=$2 body=$3
  if [[ "$body" != *"$needle"* ]]; then ok; else fail "$label"; fi
}

OUT=$("$BIN" --root "$ROOT" sessions -p alpha -f tsv 2>/dev/null)
contains "sessions lists alpha" "aaaaaaaa-1111" "$OUT"
excludes "project filter excludes beta" "bbbbbbbb-1111" "$OUT"

OUT=$("$BIN" --root "$ROOT" prompts -f tsv 2>/dev/null)
contains "prompts includes genuine request" "Fix the login bug" "$OUT"
excludes "prompts excludes developer context" "INJECTED_SECRET" "$OUT"
excludes "prompts excludes reasoning" "PRIVATE_REASONING" "$OUT"

OUT=$("$BIN" --root "$ROOT" bash -f tsv 2>/dev/null)
contains "bash parses direct exec" "cargo test" "$OUT"
contains "bash parses orchestrated exec" "cargo fmt --check" "$OUT"
contains "bash parses ChatGPT app shell command" "cargo test --all-targets" "$OUT"

OUT=$("$BIN" --root "$ROOT" writes -f tsv 2>/dev/null)
contains "writes parses apply patch" "src/login.rs" "$OUT"
contains "writes parses ChatGPT app nested patch" "src/app.rs" "$OUT"

OUT=$("$BIN" --root "$ROOT" tools -f tsv 2>/dev/null)
contains "tools counts direct and orchestrated exec" $'exec_command\t2' "$OUT"
contains "tools counts outer app exec" $'exec\t2' "$OUT"
contains "tools counts ChatGPT app shell command" $'shell_command\t1' "$OUT"
contains "tools counts direct and nested patches" $'apply_patch\t2' "$OUT"

OUT=$("$BIN" --root "$ROOT" grep 'login' --in text -f tsv 2>/dev/null)
contains "grep returns pointer" "aaaaaaaa-111" "$OUT"

set +e
OUT=$("$BIN" --root "$ROOT" grep 'PRIVATE_REASONING|INJECTED_SECRET' -f tsv 2>/dev/null)
STATUS=$?
set -e
[[ $STATUS -eq 1 ]] && ok || fail "grep safely excludes private context"
excludes "grep output excludes private context" "PRIVATE_REASONING" "$OUT"

OUT=$("$BIN" --root "$ROOT" show aaaa -f tsv 2>/dev/null)
contains "show renders visible assistant message" "Fixed the login bug" "$OUT"
excludes "show excludes reasoning" "PRIVATE_REASONING" "$OUT"
excludes "show excludes developer context" "INJECTED_SECRET" "$OUT"

OUT=$("$BIN" --root "$ROOT" projects -f json 2>/dev/null)
[[ $(grep -c '"project"' <<<"$OUT") -eq 3 ]] && ok || fail "json output has three projects"

OUT=$("$BIN" --root "$ROOT" sessions --since 2026-07-02 -f tsv 2>/dev/null)
contains "since includes beta" "bbbbbbbb-1111" "$OUT"
excludes "since excludes alpha events" "aaaaaaaa-1111" "$OUT"

set +e
"$BIN" --root "$ROOT" sessions -s abc >/dev/null 2>&1
STATUS=$?
set -e
[[ $STATUS -eq 2 ]] && ok || fail "short session prefix exits 2"

printf 'integration: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ $FAIL -eq 0 ]]
