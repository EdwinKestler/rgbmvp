#!/usr/bin/env bash
#
# P2 C3 — BFA full-history audit (no oracle).
#
# Proof points:
#   1. two honest chained mints            → audit PASSES
#   2. over-mint chain accepts             → audit FAILS (backing)
#   3. history lies about mint size        → audit FAILS (anchor)
#
# Requires: ./scripts/regtest_simplicity.sh up
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== building lab-cli =="
cargo build -q -p lab-cli
RGBMVP="${RGBMVP:-./target/debug/rgbmvp}"

MAX_SUPPLY=1000000
MINT1=30000;  LOCK1=0.00030000
MINT2=20000;  LOCK2=0.00020000
MINT3=40000;  LOCK3=0.00010000   # under-backed
FEE=0.00010000
ENTROPY=12648430
INTERNAL_KEY="d6889cb081036e0faefa3a35157ad71086b123b2b144b649798b494c300a961d"
OUT_DIR="${OUT_DIR:-.rgbmvp/tmp/c3}"
mkdir -p "$OUT_DIR"

scli() { ./scripts/regtest_simplicity.sh cli "$@"; }
scli_w() {
  local w="$1"; shift
  docker exec rgbmvp-elementsd-simplicity \
    elements-cli -chain=elementsregtest -rpcuser=user -rpcpassword=pass -rpcport=7042 \
    -rpcwallet="$w" "$@"
}
dsub() { python3 -c "from decimal import Decimal as D; print((D('$1')-D('$2')).quantize(D('0.00000001')))"; }
btc_sats() { python3 -c "from decimal import Decimal as D; print(int(D('$1')*D('100000000')))"; }

echo "════════════════════════════════════════════════════════════"
echo "  C3: BFA schema + full-history audit (no oracle)"
echo "════════════════════════════════════════════════════════════"

ACTIVE=$(scli getdeploymentinfo | jq -r '.deployments.simplicity.active // empty')
# Simplicity not required for BFA audit itself, but we share the regtest node.
GENESIS=$(scli getblockhash 0)
LBTC=$(scli dumpassetlabels | jq -r '.bitcoin')
echo "  genesis : $GENESIS"
echo "  L-BTC   : ${LBTC:0:20}…"

# wallet
scli createwallet w_c3 >/dev/null 2>&1 || scli loadwallet w_c3 >/dev/null 2>&1 || true
W=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
if [ "$(scli_w w_c3 listunspent 0 | jq 'length')" -eq 0 ]; then
  scli loadwallet w_c0 >/dev/null 2>&1 || true
  scli loadwallet w_c1 >/dev/null 2>&1 || true
  for peer in w_c0 w_c1 lab; do
    bal=$(scli_w "$peer" getbalance 2>/dev/null | jq -r 'if type=="object" then (.bitcoin//0) else . end' || echo 0)
    if python3 -c "from decimal import Decimal as D; raise SystemExit(0 if D('${bal:-0}')>=D('1') else 1)" 2>/dev/null; then
      echo "== Seed w_c3 from $peer =="
      scli_w "$peer" sendtoaddress "$W" 5 >/dev/null
      scli generatetoaddress 1 "$W" >/dev/null
      break
    fi
  done
fi
echo "  w_c3 balance: $(scli_w w_c3 getbalance | jq -r '.bitcoin // .')"

# backing asset
ISSUE=$(scli_w w_c3 issueasset 100 0)
BACKING=$(echo "$ISSUE" | jq -r '.asset')
scli generatetoaddress 1 "$W" >/dev/null
BEXP=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
scli_w w_c3 sendtoaddress "$BEXP" 50 "" "" false false 1 "UNSET" false "$BACKING" >/dev/null
scli generatetoaddress 1 "$W" >/dev/null
echo "  backing asset: ${BACKING:0:20}…"

# vault (demo P2WPKH)
VAULT_INFO=$(./target/debug/lab-simp demo-address --label bfa-vault 2>/dev/null || {
  cargo build -q -p lab-simplicity
  ./target/debug/lab-simp demo-address --label bfa-vault
})
VAULT_ADDR=$(echo "$VAULT_INFO" | jq -r '.address')
VAULT_SPK=$(echo "$VAULT_INFO" | jq -r '.spk_hex')
TERMS="elements-backing:v1;vault=$VAULT_SPK;asset=$BACKING;rate=1/1"
echo "  vault : $VAULT_ADDR"
echo "  terms : $TERMS"

# gate UTXOs (lock each)
declare -a GATE_ADDR GATE_UTXO
for i in 0 1 2 3; do
  GATE_ADDR[$i]=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
  T=$(scli_w w_c3 sendtoaddress "${GATE_ADDR[$i]}" 0.01)
  V=$(scli getrawtransaction "$T" 1 \
    | jq --arg a "${GATE_ADDR[$i]}" '.vout[] | select(.scriptPubKey.address == $a) | .n')
  GATE_UTXO[$i]="$T:$V"
  scli_w w_c3 lockunspent false "[{\"txid\":\"$T\",\"vout\":$V}]" >/dev/null
done
scli generatetoaddress 1 "$W" >/dev/null

ISSUE_JSON=$("$RGBMVP" bfa issue \
  --name LiquidRgbUSD --ticker LRUSD --max-supply $MAX_SUPPLY \
  --gate-seal "${GATE_UTXO[0]}" --backing "$TERMS" --chain elements-regtest)
CONTRACT=$(echo "$ISSUE_JSON" | jq -r '.contract_id')
echo "  contract: $CONTRACT"

RECIP() { printf 'bfa-mint-%s-recipient' "$1" | sha256sum | awk '{print $1}'; }

# Fund an exact-amount UTXO of asset A to address (returns txid:vout).
fund_exact() {
  local asset="$1" amount="$2" dest="$3"
  local txid vout spk
  if [ "$asset" = "$LBTC" ]; then
    txid=$(scli_w w_c3 sendtoaddress "$dest" "$amount")
  else
    txid=$(scli_w w_c3 sendtoaddress "$dest" "$amount" "" "" false false 1 "UNSET" false "$asset")
  fi
  scli generatetoaddress 1 "$W" >/dev/null
  spk=$(scli_w w_c3 getaddressinfo "$dest" | jq -r '.scriptPubKey')
  vout=$(scli getrawtransaction "$txid" 1 | jq --arg s "$spk" '.vout[] | select(.scriptPubKey.hex==$s) | .n')
  echo "$txid:$vout"
}

run_mint() {
  local n="$1" amount="$2" lock="$3" gate="$4" new_gate="$5" copid="$6" allow="$7"
  local gate_txid="${gate%%:*}" gate_vout="${gate##*:}"
  local recip plan addr opid cspk
  recip="$(RECIP "$n"):2"

  if [ "$copid" != "-" ]; then
    plan=$("$RGBMVP" bfa mint-plan \
      --name LiquidRgbUSD --ticker LRUSD --max-supply $MAX_SUPPLY --backing "$TERMS" \
      --genesis-gate "${GATE_UTXO[0]}" --gate-seal "$gate" \
      --mint "$amount" --recipient-seal "$recip" --new-gate-seal "$new_gate" \
      --consume-opid "$copid" --allowance "$allow" \
      --internal-key "$INTERNAL_KEY" --entropy $ENTROPY --chain elements-regtest)
  else
    plan=$("$RGBMVP" bfa mint-plan \
      --name LiquidRgbUSD --ticker LRUSD --max-supply $MAX_SUPPLY --backing "$TERMS" \
      --genesis-gate "${GATE_UTXO[0]}" --gate-seal "$gate" \
      --mint "$amount" --recipient-seal "$recip" --new-gate-seal "$new_gate" \
      --internal-key "$INTERNAL_KEY" --entropy $ENTROPY --chain elements-regtest)
  fi
  addr=$(echo "$plan" | jq -r '.tapret_address')
  opid=$(echo "$plan" | jq -r '.opid_hex')
  cspk=$(echo "$plan" | jq -r '.commitment_spk_hex')

  # Exact backing tranche UTXO + L-BTC fee UTXO (gate only closes seal @ 0.01).
  local fee_dest a_dest a_utxo f_utxo bob
  fee_dest=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
  a_dest=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
  a_utxo=$(fund_exact "$BACKING" "$lock" "$a_dest")
  # L-BTC: tapret 0.0005 + recip 0.0003 + fee 0.0001 = 0.0009 from fee utxo;
  # gate 0.01 → change 0.01 (or dust change). Simpler: gate funds all L-BTC outs.
  # L-BTC outs total: 0.0005+0.0003+0.0001=0.0009, gate has 0.01 → change 0.0091
  local lchange lrest
  lchange=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
  bob=$(scli_w w_c3 getaddressinfo "$(scli_w w_c3 getnewaddress)" | jq -r '.unconfidential')
  lrest=$(dsub "0.01" "0.0005"); lrest=$(dsub "$lrest" "0.0003"); lrest=$(dsub "$lrest" "$FEE")

  local inputs outputs raw signed txid
  inputs=$(jq -n \
    --arg gt "$gate_txid" --argjson gv "$gate_vout" \
    --arg at "${a_utxo%%:*}" --argjson av "${a_utxo##*:}" \
    '[{txid:$gt,vout:$gv},{txid:$at,vout:$av}]')
  outputs=$(jq -n --arg tap "$addr" --arg v "$VAULT_ADDR" --arg bob "$bob" \
    --arg lch "$lchange" --arg lb "$LBTC" --arg A "$BACKING" \
    --arg lock "$lock" --arg lrest "$lrest" --arg fee "$FEE" \
    '[ {($tap): 0.0005,            "asset": $lb},
       {($v):   ($lock|tonumber),  "asset": $A},
       {($bob): 0.0003,            "asset": $lb},
       {($lch): ($lrest|tonumber), "asset": $lb},
       {"fee":  ($fee|tonumber),   "asset": $lb} ]')
  raw=$(scli createrawtransaction "$inputs" "$outputs")
  signed=$(scli_w w_c3 signrawtransactionwithwallet "$raw")
  if [ "$(echo "$signed" | jq -r '.complete')" != "true" ]; then
    echo "✗ sign incomplete: $signed" >&2
    exit 1
  fi
  txid=$(scli sendrawtransaction "$(echo "$signed" | jq -r '.hex')")
  scli generatetoaddress 2 "$W" >/dev/null

  local found
  found=$(scli getrawtransaction "$txid" 1 | jq --arg s "$cspk" '[.vout[].scriptPubKey.hex] | index($s)')
  [ "$found" != "null" ] || { echo "✗ commitment spk not in mint tx $txid"; exit 1; }

  local hex
  hex=$(scli getrawtransaction "$txid")
  echo "$txid $opid $hex"
}

echo
echo "== Mint 1: $MINT1 LRUSD, $LOCK1 locked (honest) =="
R1=$(run_mint 1 $MINT1 $LOCK1 "${GATE_UTXO[0]}" "${GATE_UTXO[1]}" - -)
TX1=$(echo "$R1" | awk '{print $1}'); OPID1=$(echo "$R1" | awk '{print $2}'); HEX1=$(echo "$R1" | awk '{print $3}')
echo "  witness: $TX1"

echo
echo "== Mint 2 (chained): $MINT2 LRUSD, $LOCK2 locked (honest) =="
R2=$(run_mint 2 $MINT2 $LOCK2 "${GATE_UTXO[1]}" "${GATE_UTXO[2]}" "$OPID1" $((MAX_SUPPLY - MINT1)))
TX2=$(echo "$R2" | awk '{print $1}'); OPID2=$(echo "$R2" | awk '{print $2}'); HEX2=$(echo "$R2" | awk '{print $3}')
echo "  witness: $TX2"

mk_history() {
  local file="$1" mints_json="$2"
  jq -n --arg backing "$TERMS" --arg g0 "${GATE_UTXO[0]}" --argjson mints "$mints_json" \
    --arg ik "$INTERNAL_KEY" --argjson ent "$ENTROPY" \
    '{name:"LiquidRgbUSD", ticker:"LRUSD", max_supply:'"$MAX_SUPPLY"',
      backing:$backing, genesis_gate_seal:$g0,
      internal_key:$ik, entropy:$ent, chain_net:"elements-regtest",
      mints:$mints}' > "$file"
}

echo
echo "== Audit: honest two-mint history =="
mk_history "$OUT_DIR/bfa_history.json" "$(jq -n \
  --arg r1 "$(RECIP 1):2" --arg g1 "${GATE_UTXO[1]}" --arg t1 "$TX1" --arg h1 "$HEX1" \
  --arg r2 "$(RECIP 2):2" --arg g2 "${GATE_UTXO[2]}" --arg t2 "$TX2" --arg h2 "$HEX2" \
  '[{mint:'"$MINT1"', recipient_seal:$r1, new_gate_seal:$g1, witness_txid:$t1, witness_tx_hex:$h1},
    {mint:'"$MINT2"', recipient_seal:$r2, new_gate_seal:$g2, witness_txid:$t2, witness_tx_hex:$h2}]')"
"$RGBMVP" bfa audit --history "$OUT_DIR/bfa_history.json"
echo "✓ honest history passes"

echo
echo "== Mint 3 (over-mint): $MINT3 minted, only $LOCK3 locked =="
R3=$(run_mint 3 $MINT3 $LOCK3 "${GATE_UTXO[2]}" "${GATE_UTXO[3]}" "$OPID2" $((MAX_SUPPLY - MINT1 - MINT2)))
TX3=$(echo "$R3" | awk '{print $1}'); HEX3=$(echo "$R3" | awk '{print $3}')
echo "  witness: $TX3 — CONFIRMED on chain (chain cannot see RGB amounts)"

MINTS3() {
  jq -n \
    --arg r1 "$(RECIP 1):2" --arg g1 "${GATE_UTXO[1]}" --arg t1 "$TX1" --arg h1 "$HEX1" \
    --arg r2 "$(RECIP 2):2" --arg g2 "${GATE_UTXO[2]}" --arg t2 "$TX2" --arg h2 "$HEX2" \
    --arg r3 "$(RECIP 3):2" --arg g3 "${GATE_UTXO[3]}" --arg t3 "$TX3" --arg h3 "$HEX3" \
    --argjson m3 "$1" \
    '[{mint:'"$MINT1"', recipient_seal:$r1, new_gate_seal:$g1, witness_txid:$t1, witness_tx_hex:$h1},
      {mint:'"$MINT2"', recipient_seal:$r2, new_gate_seal:$g2, witness_txid:$t2, witness_tx_hex:$h2},
      {mint:$m3, recipient_seal:$r3, new_gate_seal:$g3, witness_txid:$t3, witness_tx_hex:$h3}]'
}

echo
echo "== Audit: history including over-mint =="
mk_history "$OUT_DIR/bfa_history_overmint.json" "$(MINTS3 $MINT3)"
if "$RGBMVP" bfa audit --history "$OUT_DIR/bfa_history_overmint.json"; then
  echo "✗ FAIL: audit accepted under-backed mint"; exit 1
fi
echo "✓ audit rejected over-mint (backing rule)"

echo
echo "== Audit: history that LIES about mint 3 size (claims 10000) =="
mk_history "$OUT_DIR/bfa_history_lie.json" "$(MINTS3 10000)"
if "$RGBMVP" bfa audit --history "$OUT_DIR/bfa_history_lie.json"; then
  echo "✗ FAIL: audit accepted falsified history"; exit 1
fi
echo "✓ audit rejected falsified history (anchor mismatch)"

echo
echo "✅ C3 DONE — BFA terms in contract id; full-history audit, no oracle"
echo "   artifacts under $OUT_DIR"
echo "   P2 lab-closed criteria: C0 + C3 green"
