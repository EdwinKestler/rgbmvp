#!/usr/bin/env bash
#
# P2 C1 — Simplicity mint-gate: vault lock + gate recursion.
#
# Consensus enforces (no key on the gate):
#   vout[0]  opret RGB-anchor shape
#   vout[1]  exact TRANCHE of BACKING_ASSET → vault SPK
#   vout[3]  re-create same gate covenant (recursion)
#
# Proves: two chained mints + drop-anchor / wrong-amount / no-recreate rejects.
# RGB IFA schema is out of scope here (container only); see docs/C1_CLOSED.md.
#
# Requires: ./scripts/regtest_simplicity.sh up
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SIMP="${SIMP:-./target/debug/lab-simp}"
echo "== building lab-simp =="
cargo build -q -p lab-simplicity


GATE_PROG="${GATE_PROG:-crates/lab-simplicity/programs/mint_gate_covenant.simf}"
TRANCHE=250000
GATE_SAT=20000
RECIP_SAT=5000
FEE_SAT=2000
OUT_DIR="${OUT_DIR:-.rgbmvp/tmp/c1}"
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
echo "  C1: mint-gate (vault lock + gate recursion)"
echo "════════════════════════════════════════════════════════════"

ACTIVE=$(scli getdeploymentinfo | jq -r '.deployments.simplicity.active')
[ "$ACTIVE" = "true" ] || { echo "✗ simplicity not active"; exit 1; }
GENESIS=$(scli getblockhash 0)
LBTC=$(scli dumpassetlabels | jq -r '.bitcoin')
echo "  simplicity : ACTIVE"
echo "  genesis    : $GENESIS"
echo "  L-BTC      : ${LBTC:0:20}…"

# ── wallet + seed ─────────────────────────────────────────────────
scli createwallet w_c1 >/dev/null 2>&1 || scli loadwallet w_c1 >/dev/null 2>&1 || true
W=$(scli_w w_c1 getaddressinfo "$(scli_w w_c1 getnewaddress)" | jq -r '.unconfidential')
seed_from_op_true() {
  local dest="$1"
  local SCAN GTX GVO OUTS RAW
  SCAN=$(scli scantxoutset start '["raw(51)"]' 2>/dev/null || echo '{}')
  if ! echo "$SCAN" | jq -e '.unspents | length > 0' >/dev/null 2>&1; then
    return 1
  fi
  GTX=$(echo "$SCAN" | jq -r '.unspents[0].txid')
  GVO=$(echo "$SCAN" | jq -r '.unspents[0].vout')
  OUTS=$(jq -n --arg w "$dest" --arg a "$LBTC" '[{($w):50,"asset":$a},{"fee":20999950,"asset":$a}]')
  RAW=$(scli createrawtransaction "[{\"txid\":\"$GTX\",\"vout\":$GVO}]" "$OUTS")
  scli sendrawtransaction "$RAW" 0 >/dev/null
  scli generatetoaddress 2 "$dest" >/dev/null
  return 0
}
seed_from_peer_wallet() {
  local dest="$1" peer
  for peer in w_c0 w_s w_g lab; do
    # Already-loaded wallets return -35; that is fine.
    scli loadwallet "$peer" >/dev/null 2>&1 || true
    local bal
    bal=$(scli_w "$peer" getbalance 2>/dev/null | jq -r 'if type=="object" then (.bitcoin // 0) else . end' 2>/dev/null || echo 0)
    bal=${bal:-0}
    if python3 -c "from decimal import Decimal as D; raise SystemExit(0 if D('$bal')>=D('1') else 1)" 2>/dev/null; then
      echo "== Seed w_c1 from wallet $peer ($bal L-BTC) =="
      scli_w "$peer" sendtoaddress "$dest" 5 >/dev/null
      scli generatetoaddress 1 "$dest" >/dev/null
      return 0
    fi
  done
  return 1
}
if [ "$(scli_w w_c1 listunspent 0 | jq 'length')" -eq 0 ]; then
  if seed_from_op_true "$W"; then
    echo "== Seeded w_c1 from OP_TRUE genesis =="
  elif seed_from_peer_wallet "$W"; then
    :
  else
    echo "✗ No funds available (OP_TRUE spent and no peer wallet with L-BTC)."
    echo "  Tip: ./scripts/regtest_simplicity.sh wipe && up, then re-run demos."
    exit 1
  fi
fi
echo "  w_c1 balance: $(scli_w w_c1 getbalance | jq -r '.bitcoin // .')"

# ── backing asset ─────────────────────────────────────────────────
ISSUE=$(scli_w w_c1 issueasset 100 0)
BACKING=$(echo "$ISSUE" | jq -r '.asset')
scli generatetoaddress 1 "$W" >/dev/null
BEXP=$(scli_w w_c1 getaddressinfo "$(scli_w w_c1 getnewaddress)" | jq -r '.unconfidential')
# Explicit send of half the issued asset for spendable UTXOs
scli_w w_c1 sendtoaddress "$BEXP" 50 "" "" false false 1 "UNSET" false "$BACKING" >/dev/null
scli generatetoaddress 1 "$W" >/dev/null
echo "  backing asset: ${BACKING:0:20}…"

# ── vault + minter key ────────────────────────────────────────────
VAULT_INFO=$("$SIMP" demo-address --label vault)
VAULT_SPK=$(echo "$VAULT_INFO" | jq -r '.spk_hex')
VAULT_ADDR=$(echo "$VAULT_INFO" | jq -r '.address')
KEY=$("$SIMP" demo-address --label minter)
KEY_ADDR=$(echo "$KEY" | jq -r '.address')
KEY_SPK=$(echo "$KEY" | jq -r '.spk_hex')

GATE=$("$SIMP" address --program "$GATE_PROG" \
  --vault-spk "$VAULT_SPK" --backing-asset "$BACKING" --tranche "$TRANCHE")
GATE_ADDR=$(echo "$GATE" | jq -r '.address')
GATE_SPK=$(echo "$GATE" | jq -r '.spk_hex')
CMR=$(echo "$GATE" | jq -r '.cmr')
echo "  vault     : $VAULT_ADDR"
echo "  minter    : $KEY_ADDR"
echo "  mint-gate : $GATE_ADDR"
echo "  CMR       : $CMR"
echo "$GATE" > "$OUT_DIR/gate.json"

fund_gate() {
  local txid vout
  txid=$(scli_w w_c1 sendtoaddress "$GATE_ADDR" "$(btc $GATE_SAT)")
  scli generatetoaddress 1 "$W" >/dev/null
  vout=$(scli getrawtransaction "$txid" 1 | jq --arg s "$GATE_SPK" '.vout[] | select(.scriptPubKey.hex==$s) | .n')
  echo "$txid:$vout"
}

fund_key() {
  local asset="$1" sat="$2" txid vout
  if [ "$asset" = "$LBTC" ]; then
    txid=$(scli_w w_c1 sendtoaddress "$KEY_ADDR" "$(btc $sat)")
  else
    txid=$(scli_w w_c1 sendtoaddress "$KEY_ADDR" "$(btc $sat)" "" "" false false 1 "UNSET" false "$asset")
  fi
  scli generatetoaddress 1 "$W" >/dev/null
  vout=$(scli getrawtransaction "$txid" 1 | jq --arg s "$KEY_SPK" '.vout[] | select(.scriptPubKey.hex==$s) | .n')
  echo "$txid:$vout"
}

RECIP_SPK=$(scli_w w_c1 getaddressinfo "$(scli_w w_c1 getaddressinfo "$(scli_w w_c1 getnewaddress)" | jq -r '.unconfidential')" | jq -r '.scriptPubKey')

assert_vault() {
  local txid="$1" dec locked want
  dec=$(scli getrawtransaction "$txid" 1)
  locked=$(echo "$dec" | jq --arg s "$VAULT_SPK" --arg a "$BACKING" \
    '[.vout[] | select(.scriptPubKey.hex==$s and .asset==$a) | .value] | add // 0')
  want=$(btc $TRANCHE)
  if [ "$(python3 -c "from decimal import Decimal as D; print(D('$locked')>=D('$want'))")" = "True" ]; then
    echo "  ✓ vault holds $locked backing (>= $want)"
  else
    echo "  ✗ FAIL vault holds $locked expected >= $want"; exit 1
  fi
  # gate recreated at vout 3
  local gspk
  gspk=$(echo "$dec" | jq -r '.vout[3].scriptPubKey.hex')
  [ "$gspk" = "$GATE_SPK" ] || { echo "✗ FAIL vout3 not gate spk"; exit 1; }
  echo "  ✓ vout[3] re-creates mint-gate covenant"
}

do_mint() {
  local round="$1" gate="$2" tamper="$3"
  local gate_txid="${gate%%:*}" gate_vout="${gate##*:}"
  local root need a_utxo f_utxo raw
  root=$(printf 'c1-mint-round-%s-%s' "$round" "$RANDOM" | sha256sum | awk '{print $1}')
  need=$((RECIP_SAT + GATE_SAT + FEE_SAT + 5000))
  a_utxo=$(fund_key "$BACKING" "$TRANCHE")
  f_utxo=$(fund_key "$LBTC" "$need")

  raw=$("$SIMP" mint-spend --program "$GATE_PROG" \
    --vault-spk "$VAULT_SPK" --backing-asset "$BACKING" --tranche "$TRANCHE" \
    --anchor-payload "$root" \
    --gate-txid "$gate_txid" --gate-vout "$gate_vout" --gate-value-sat $GATE_SAT \
    --asset-txid "${a_utxo%%:*}" --asset-vout "${a_utxo##*:}" \
    --fee-txid "${f_utxo%%:*}" --fee-vout "${f_utxo##*:}" --fee-input-sat $need \
    --key-label minter --vault-spk-out "$VAULT_SPK" \
    --recipient-spk "$RECIP_SPK" --recipient-sat $RECIP_SAT --fee-sat $FEE_SAT \
    --lbtc-asset "$LBTC" --genesis-hash "$GENESIS" --tamper "$tamper")

  if [ "$tamper" != "none" ]; then
    if scli sendrawtransaction "$raw" >/dev/null 2>&1; then
      echo "TAMPER_ACCEPTED"
    else
      echo "TAMPER_REJECTED"
    fi
    return
  fi

  local txid
  txid=$(scli sendrawtransaction "$raw")
  scli generatetoaddress 1 "$W" >/dev/null
  # recreated gate is vout 3
  echo "$txid $txid:3"
}

echo
echo "════════ ROUND 1 — mint against genesis gate ════════"
GATE1=$(fund_gate)
echo "  gate UTXO: $GATE1"
read -r TX1 GATE2 < <(do_mint 1 "$GATE1" none)
echo "  ✓ mint tx: $TX1"
echo "  recreated gate: $GATE2"
assert_vault "$TX1"
echo "$TX1" > "$OUT_DIR/mint1_txid.txt"

echo
echo "════════ ROUND 2 — mint against recreated gate ════════"
echo "  spending: $GATE2"
read -r TX2 GATE3 < <(do_mint 2 "$GATE2" none)
echo "  ✓ mint tx: $TX2"
echo "  recreated gate: $GATE3"
assert_vault "$TX2"
echo "$TX2" > "$OUT_DIR/mint2_txid.txt"

echo
echo "════════ NEGATIVES — consensus rejects ════════"
for MODE in drop-anchor wrong-amount no-recreate; do
  GN=$(fund_gate)
  R=$(do_mint "neg-$MODE" "$GN" "$MODE")
  if [ "$R" = "TAMPER_REJECTED" ]; then
    echo "  ✓ $MODE → rejected by consensus"
  else
    echo "  ✗ FAIL: $MODE was accepted"; exit 1
  fi
done

echo
echo "✅ C1 DONE — mint-gate vault lock + recursion enforced on-chain"
echo "   artifacts under $OUT_DIR"
