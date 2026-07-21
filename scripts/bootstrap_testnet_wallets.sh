#!/usr/bin/env bash
# Import reusable Liquid Testnet fixture wallets (alice/bob/carol/maker).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${RGBMVP_BIN:-./target/debug/rgbmvp}"
FIXTURE="${1:-fixtures/testnet_wallets.json}"
FORCE="${FORCE:-}"

if [[ ! -x "$BIN" ]]; then
  echo "building lab-cli…"
  cargo build -p lab-cli
  BIN=./target/debug/rgbmvp
fi

ARGS=(wallet bootstrap-testnet --fixture "$FIXTURE")
if [[ "${FORCE}" == "1" || "${2:-}" == "--force" ]]; then
  ARGS+=(--force)
fi

echo "== bootstrap fixtures =="
"$BIN" "${ARGS[@]}"
echo
echo "== registry =="
"$BIN" wallet registry
echo
echo "== list =="
"$BIN" wallet list
echo
echo "Next: fund alice, then rebalance:"
echo "  $BIN wallet address --name alice"
echo "  # https://liquidtestnet.com/faucet"
echo "  $BIN wallet send --from alice --to bob --amount-sats 20000"
echo "  $BIN wallet list --sync"
