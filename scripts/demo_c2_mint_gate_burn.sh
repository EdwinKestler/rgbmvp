#!/usr/bin/env bash
#
# P2 C2 — Simplicity mint-gate burn variant.
#
# Same program as C1 (mint_gate_covenant.simf), but VAULT_SPK_HASH =
# SHA256(empty script). Consensus enforces:
#   vout[0]  opret RGB-anchor shape
#   vout[1]  exact TRANCHE of BACKING_ASSET → empty SPK (burn / unspendable)
#   vout[3]  re-create same gate covenant (recursion)
#
# Proves: two chained burn-mints + drop-anchor / wrong-amount / no-recreate / not-burn rejects.
# See docs/C2_CLOSED.md · ADR-C2 in ROADMAP_NEXT.md.
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
OUT_DIR="${OUT_DIR:-.rgbmvp/tmp/c2}"
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
echo "  C2: mint-gate BURN (empty SPK + gate recursion)"
echo "════════════════════════════════════════════════════════════"

ACTIVE=$(scli getdeploymentinfo | jq -r '.deployments.simplicity.active')
[ "$ACTIVE" = "true" ] || { echo "✗ simplicity not active"; exit 1; }
GENESIS=$(scli getblockhash 0)
LBTC=$(scli dumpassetlabels | jq -r '.bitcoin')
echo "  simplicity : ACTIVE"
echo "  genesis    : $GENESIS"
echo "  L-BTC      : ${LBTC:0:20}…"

# ── wallet + seed ─────────────────────────────────────────────────
scli createwallet w_c2 >/dev/null 2>&1 || scli loadwallet w_c2 >/dev/null 2>&1 || true
W=$(scli_w w_c2 getaddressinfo "$(scli_w w_c2 getnewaddress)" | jq -r '.unconfidential')
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
  # Prefer high-balance wallets first (w_c1 may hold assets but low free L-BTC).
  for peer in w_c0 w_s w_g w_c1 lab; do
    scli loadwallet "$peer" >/dev/null 2>&1 || true
    local bal
    bal=$(scli_w "$peer" getbalance 2>/dev/null | jq -r 'if type=="object" then (.bitcoin // 0) else . end' 2>/dev/null || echo 0)
    bal=${bal:-0}
    if python3 -c "from decimal import Decimal as D; raise SystemExit(0 if D('$bal')>=D('2') else 1)" 2>/dev/null; then
      echo "== Seed w_c2 from wallet $peer ($bal L-BTC) =="
      # modest amount — confidential fee headroom varies by wallet UTXO shape
      if scli_w "$peer" sendtoaddress "$dest" 2 >/dev/null 2>&1; then
        scli generatetoaddress 1 "$dest" >/dev/null
        return 0
      fi
      echo "  (send 2 failed from $peer, trying next)"
    fi
  done
  return 1
}
if [ "$(scli_w w_c2 listunspent 0 | jq 'length')" -eq 0 ]; then
  if seed_from_op_true "$W"; then
    echo "== Seeded w_c2 from OP_TRUE genesis =="
  elif seed_from_peer_wallet "$W"; then
    :
  else
    echo "✗ No funds available (OP_TRUE spent and no peer wallet with L-BTC)."
    echo "  Tip: ./scripts/regtest_simplicity.sh wipe && up, then re-run demos."
    exit 1
  fi
fi
echo "  w_c2 balance: $(scli_w w_c2 getbalance | jq -r '.bitcoin // .')"

# ── backing asset ─────────────────────────────────────────────────
ISSUE=$(scli_w w_c2 issueasset 100 0)
BACKING=$(echo "$ISSUE" | jq -r '.asset')
scli generatetoaddress 1 "$W" >/dev/null
BEXP=$(scli_w w_c2 getaddressinfo "$(scli_w w_c2 getnewaddress)" | jq -r '.unconfidential')
scli_w w_c2 sendtoaddress "$BEXP" 50 "" "" false false 1 "UNSET" false "$BACKING" >/dev/null
scli generatetoaddress 1 "$W" >/dev/null
echo "  backing asset: ${BACKING:0:20}…"

# ── minter key (no vault address — burn target is empty SPK) ──────
KEY=$("$SIMP" demo-address --label minter-c2)
KEY_ADDR=$(echo "$KEY" | jq -r '.address')
KEY_SPK=$(echo "$KEY" | jq -r '.spk_hex')

GATE=$("$SIMP" address --program "$GATE_PROG" \
  --burn --backing-asset "$BACKING" --tranche "$TRANCHE")
GATE_ADDR=$(echo "$GATE" | jq -r '.address')
GATE_SPK=$(echo "$GATE" | jq -r '.spk_hex')
CMR=$(echo "$GATE" | jq -r '.cmr')
EMPTY_HASH=$("$SIMP" address --program "$GATE_PROG" --burn --backing-asset "$BACKING" --tranche "$TRANCHE" 2>/dev/null | jq -r '.cmr' >/dev/null; python3 -c "import hashlib; print(hashlib.sha256(b'').hexdigest())")
echo "  minter    : $KEY_ADDR"
echo "  burn-gate : $GATE_ADDR"
echo "  CMR       : $CMR"
echo "  burn SPK  : <empty>  sha256=$EMPTY_HASH"
echo "$GATE" > "$OUT_DIR/gate.json"
echo "$EMPTY_HASH" > "$OUT_DIR/burn_spk_hash.txt"

fund_gate() {
  local txid vout
  txid=$(scli_w w_c2 sendtoaddress "$GATE_ADDR" "$(btc $GATE_SAT)")
  scli generatetoaddress 1 "$W" >/dev/null
  vout=$(scli getrawtransaction "$txid" 1 | jq --arg s "$GATE_SPK" '.vout[] | select(.scriptPubKey.hex==$s) | .n')
  echo "$txid:$vout"
}

fund_key() {
  local asset="$1" sat="$2" txid vout
  if [ "$asset" = "$LBTC" ]; then
    txid=$(scli_w w_c2 sendtoaddress "$KEY_ADDR" "$(btc $sat)")
  else
    txid=$(scli_w w_c2 sendtoaddress "$KEY_ADDR" "$(btc $sat)" "" "" false false 1 "UNSET" false "$asset")
  fi
  scli generatetoaddress 1 "$W" >/dev/null
  vout=$(scli getrawtransaction "$txid" 1 | jq --arg s "$KEY_SPK" '.vout[] | select(.scriptPubKey.hex==$s) | .n')
  echo "$txid:$vout"
}

RECIP_SPK=$(scli_w w_c2 getaddressinfo "$(scli_w w_c2 getaddressinfo "$(scli_w w_c2 getnewaddress)" | jq -r '.unconfidential')" | jq -r '.scriptPubKey')

assert_burn() {
  local txid="$1" dec burned want
  dec=$(scli getrawtransaction "$txid" 1)
  # empty scriptPubKey hex is ""
  burned=$(echo "$dec" | jq --arg a "$BACKING" \
    '[.vout[] | select((.scriptPubKey.hex=="" or .scriptPubKey.hex==null) and .asset==$a) | .value] | add // 0')
  want=$(btc $TRANCHE)
  if [ "$(python3 -c "from decimal import Decimal as D; print(D('$burned')>=D('$want'))")" = "True" ]; then
    echo "  ✓ burned $burned backing to empty SPK (>= $want)"
  else
    echo "  ✗ FAIL burned $burned expected >= $want"; exit 1
  fi
  local gspk
  gspk=$(echo "$dec" | jq -r '.vout[3].scriptPubKey.hex')
  [ "$gspk" = "$GATE_SPK" ] || { echo "✗ FAIL vout3 not gate spk"; exit 1; }
  echo "  ✓ vout[3] re-creates burn mint-gate covenant"
}

do_mint() {
  local round="$1" gate="$2" tamper="$3"
  local gate_txid="${gate%%:*}" gate_vout="${gate##*:}"
  local root need a_utxo f_utxo raw
  root=$(printf 'c2-burn-round-%s-%s' "$round" "$RANDOM" | sha256sum | awk '{print $1}')
  need=$((RECIP_SAT + GATE_SAT + FEE_SAT + 5000))
  a_utxo=$(fund_key "$BACKING" "$TRANCHE")
  f_utxo=$(fund_key "$LBTC" "$need")

  raw=$("$SIMP" mint-spend --program "$GATE_PROG" \
    --burn --backing-asset "$BACKING" --tranche "$TRANCHE" \
    --anchor-payload "$root" \
    --gate-txid "$gate_txid" --gate-vout "$gate_vout" --gate-value-sat $GATE_SAT \
    --asset-txid "${a_utxo%%:*}" --asset-vout "${a_utxo##*:}" \
    --fee-txid "${f_utxo%%:*}" --fee-vout "${f_utxo##*:}" --fee-input-sat $need \
    --key-label minter-c2 \
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
  echo "$txid $txid:3"
}

echo
echo "════════ ROUND 1 — burn-mint against genesis gate ════════"
GATE1=$(fund_gate)
echo "  gate UTXO: $GATE1"
read -r TX1 GATE2 < <(do_mint 1 "$GATE1" none)
echo "  ✓ mint tx: $TX1"
echo "  recreated gate: $GATE2"
assert_burn "$TX1"
echo "$TX1" > "$OUT_DIR/mint1_txid.txt"

echo
echo "════════ ROUND 2 — burn-mint against recreated gate ════════"
echo "  spending: $GATE2"
read -r TX2 GATE3 < <(do_mint 2 "$GATE2" none)
echo "  ✓ mint tx: $TX2"
echo "  recreated gate: $GATE3"
assert_burn "$TX2"
echo "$TX2" > "$OUT_DIR/mint2_txid.txt"

echo
echo "════════ NEGATIVES — consensus rejects ════════"
for MODE in drop-anchor wrong-amount no-recreate not-burn; do
  GN=$(fund_gate)
  R=$(do_mint "neg-$MODE" "$GN" "$MODE")
  if [ "$R" = "TAMPER_REJECTED" ]; then
    echo "  ✓ $MODE → rejected by consensus"
  else
    echo "  ✗ FAIL: $MODE was accepted"; exit 1
  fi
done

echo
echo "✅ C2 DONE — mint-gate BURN + recursion enforced on-chain"
echo "   artifacts under $OUT_DIR"
echo "   BFA tip: commit mode=burn + empty vault in elements-backing:v1 terms"
