#!/usr/bin/env bash
#
# P2 C4 — Simplicity time-locked staking (keyless after maturity).
#
# Stake UTXO under stake_covenant.simf:
#   - absolute height MATURE_HEIGHT via nLockTime + check_lock_height
#   - unstake forces full principal to STAKER_SPK (no key)
#   - fee from a separate P2WPKH input
#
# Proves: early unstake rejected; mature unstake OK; wrong-dest / wrong-amount reject.
# See docs/C4_CLOSED.md · ADR-C4.
#
# Requires: ./scripts/regtest_simplicity.sh up
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SIMP="${SIMP:-./target/debug/lab-simp}"
echo "== building lab-simp =="
cargo build -q -p lab-simplicity

STAKE_PROG="${STAKE_PROG:-crates/lab-simplicity/programs/stake_covenant.simf}"
STAKE_SAT=50000
FEE_IN_SAT=20000
FEE_SAT=2000
DELAY_BLOCKS=3
OUT_DIR="${OUT_DIR:-.rgbmvp/tmp/c4}"
mkdir -p "$OUT_DIR"

scli() { ./scripts/regtest_simplicity.sh cli "$@"; }
scli_w() {
  local w="$1"; shift
  docker exec rgbmvp-elementsd-simplicity \
    elements-cli -chain=elementsregtest -rpcuser=user -rpcpassword=pass -rpcport=7042 \
    -rpcwallet="$w" "$@"
}
btc() { python3 -c "from decimal import Decimal as D; print((D('$1')/D('100000000')).quantize(D('0.00000001')))"; }

echo "════════════════════════════════════════════════════════════"
echo "  C4: time-locked stake (absolute height + principal home)"
echo "════════════════════════════════════════════════════════════"

ACTIVE=$(scli getdeploymentinfo | jq -r '.deployments.simplicity.active')
[ "$ACTIVE" = "true" ] || { echo "✗ simplicity not active"; exit 1; }
GENESIS=$(scli getblockhash 0)
LBTC=$(scli dumpassetlabels | jq -r '.bitcoin')
HEIGHT=$(scli getblockcount)
MATURE=$((HEIGHT + DELAY_BLOCKS + 1))
echo "  simplicity : ACTIVE"
echo "  height     : $HEIGHT → mature $MATURE (+$DELAY_BLOCKS)"
echo "  L-BTC      : ${LBTC:0:20}…"

# ── wallet ────────────────────────────────────────────────────────
scli createwallet w_c4 >/dev/null 2>&1 || scli loadwallet w_c4 >/dev/null 2>&1 || true
W=$(scli_w w_c4 getaddressinfo "$(scli_w w_c4 getnewaddress)" | jq -r '.unconfidential')
seed_from_peer() {
  local dest="$1" peer
  for peer in w_c0 w_c1 w_c2 w_s; do
    scli loadwallet "$peer" >/dev/null 2>&1 || true
    local bal
    bal=$(scli_w "$peer" getbalance 2>/dev/null | jq -r 'if type=="object" then (.bitcoin // 0) else . end' 2>/dev/null || echo 0)
    if python3 -c "from decimal import Decimal as D; raise SystemExit(0 if D('${bal:-0}')>=D('2') else 1)" 2>/dev/null; then
      echo "== Seed w_c4 from $peer ($bal L-BTC) =="
      if scli_w "$peer" sendtoaddress "$dest" 2 >/dev/null 2>&1; then
        scli generatetoaddress 1 "$dest" >/dev/null
        return 0
      fi
    fi
  done
  return 1
}
if [ "$(scli_w w_c4 listunspent 0 | jq 'length')" -eq 0 ]; then
  seed_from_peer "$W" || { echo "✗ no funds to seed w_c4"; exit 1; }
fi
echo "  w_c4 balance: $(scli_w w_c4 getbalance | jq -r '.bitcoin // .')"

# ── staker + fee key ──────────────────────────────────────────────
STAKER=$("$SIMP" demo-address --label staker-c4)
STAKER_ADDR=$(echo "$STAKER" | jq -r '.address')
STAKER_SPK=$(echo "$STAKER" | jq -r '.spk_hex')
FEEKEY=$("$SIMP" demo-address --label unstaker)
FEE_ADDR=$(echo "$FEEKEY" | jq -r '.address')
FEE_SPK=$(echo "$FEEKEY" | jq -r '.spk_hex')

STAKE=$("$SIMP" stake-address --program "$STAKE_PROG" \
  --mature-height "$MATURE" --staker-spk "$STAKER_SPK" --principal-asset "$LBTC")
STAKE_ADDR=$(echo "$STAKE" | jq -r '.address')
STAKE_SPK=$(echo "$STAKE" | jq -r '.spk_hex')
CMR=$(echo "$STAKE" | jq -r '.cmr')
echo "  staker    : $STAKER_ADDR"
echo "  fee key   : $FEE_ADDR"
echo "  stake cvd : $STAKE_ADDR"
echo "  CMR       : $CMR"
echo "$STAKE" > "$OUT_DIR/stake.json"

fund_to() {
  local dest_addr="$1" dest_spk="$2" sat="$3" txid vout
  txid=$(scli_w w_c4 sendtoaddress "$dest_addr" "$(btc $sat)")
  scli generatetoaddress 1 "$W" >/dev/null
  vout=$(scli getrawtransaction "$txid" 1 | jq --arg s "$dest_spk" '.vout[] | select(.scriptPubKey.hex==$s) | .n')
  echo "$txid:$vout"
}

build_raw() {
  local tamper="$1" stake_u fee_u
  stake_u="$2"
  fee_u="$3"
  "$SIMP" stake-spend --program "$STAKE_PROG" \
    --mature-height "$MATURE" --staker-spk "$STAKER_SPK" --principal-asset "$LBTC" \
    --stake-txid "${stake_u%%:*}" --stake-vout "${stake_u##*:}" --stake-value-sat $STAKE_SAT \
    --fee-txid "${fee_u%%:*}" --fee-vout "${fee_u##*:}" --fee-input-sat $FEE_IN_SAT --fee-sat $FEE_SAT \
    --key-label unstaker --lbtc-asset "$LBTC" --genesis-hash "$GENESIS" \
    --lock-height "$MATURE" --tamper "$tamper"
}

echo
echo "════════ FUND stake + fee UTXOs ════════"
STAKE_U=$(fund_to "$STAKE_ADDR" "$STAKE_SPK" $STAKE_SAT)
FEE_U=$(fund_to "$FEE_ADDR" "$FEE_SPK" $FEE_IN_SAT)
echo "  stake UTXO: $STAKE_U"
echo "  fee UTXO  : $FEE_U"
echo "$STAKE_U" > "$OUT_DIR/stake_utxo.txt"

echo
echo "════════ NEGATIVE — early unstake (height < mature) ════════"
EARLY=$(build_raw none "$STAKE_U" "$FEE_U")
if scli sendrawtransaction "$EARLY" >/dev/null 2>&1; then
  echo "  ✗ FAIL: early unstake accepted before maturity"; exit 1
else
  echo "  ✓ early unstake rejected (non-final / locktime)"
fi

echo
echo "════════ NEGATIVE — wrong-dest / wrong-amount (after mature for mempool) ════════"
# Generate to maturity first so only covenant rules reject (not locktime alone)
scli generatetoaddress $((DELAY_BLOCKS + 2)) "$W" >/dev/null
NOW=$(scli getblockcount)
echo "  height now: $NOW (mature was $MATURE)"
[ "$NOW" -ge "$MATURE" ] || { echo "✗ height still below mature"; exit 1; }

for MODE in wrong-dest wrong-amount early-lock; do
  RAW=$(build_raw "$MODE" "$STAKE_U" "$FEE_U")
  if scli sendrawtransaction "$RAW" >/dev/null 2>&1; then
    echo "  ✗ FAIL: $MODE accepted"; exit 1
  else
    echo "  ✓ $MODE → rejected by consensus"
  fi
done

echo
echo "════════ POSITIVE — mature unstake ════════"
RAW=$(build_raw none "$STAKE_U" "$FEE_U")
TXID=$(scli sendrawtransaction "$RAW")
scli generatetoaddress 1 "$W" >/dev/null
echo "  ✓ unstake tx: $TXID"
echo "$TXID" > "$OUT_DIR/unstake_txid.txt"

# Principal on staker SPK
DEC=$(scli getrawtransaction "$TXID" 1)
GOT=$(echo "$DEC" | jq --arg s "$STAKER_SPK" --arg a "$LBTC" \
  '[.vout[] | select(.scriptPubKey.hex==$s and .asset==$a) | .value] | add // 0')
WANT=$(btc $STAKE_SAT)
if [ "$(python3 -c "from decimal import Decimal as D; print(D('$GOT')==D('$WANT'))")" = "True" ]; then
  echo "  ✓ principal $GOT returned to staker SPK"
else
  echo "  ✗ FAIL principal $GOT expected $WANT"; exit 1
fi

echo
echo "✅ C4 DONE — stake maturity + principal-home enforced on-chain"
echo "   artifacts under $OUT_DIR"
