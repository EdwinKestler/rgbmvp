#!/usr/bin/env bash
#
# P2 C0 — Simplicity covenant: preimage(H) ∧ opret-shaped anchor.
#
# Proof points:
#   A. wrong preimage        → program cannot satisfy
#   B. anchor stripped after satisfaction → CONSENSUS rejects
#   C. compliant spend       → accepted; vout0 = 6a20||payload
#
# Requires: ./scripts/regtest_simplicity.sh up  (Elements 23.3, :7042)
# Docs: docs/P2_SIMPLICITY.md · docs/P2_PLAN.md
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SIMP="${SIMP:-./target/debug/lab-simp}"
if [ ! -x "$SIMP" ]; then
  echo "== building lab-simp =="
  cargo build -p lab-simplicity
fi

PROGRAM="${PROGRAM:-crates/lab-simplicity/programs/rgb_anchor_covenant.simf}"
FEE_SAT=1000
FUND_AMT=0.001
FUND_SAT=100000
OUT_DIR="${OUT_DIR:-.rgbmvp/tmp/c0}"
mkdir -p "$OUT_DIR"

scli() {
  ./scripts/regtest_simplicity.sh cli "$@"
}
scli_w() {
  local wallet="$1"; shift
  docker exec rgbmvp-elementsd-simplicity \
    elements-cli -chain=elementsregtest -rpcuser=user -rpcpassword=pass -rpcport=7042 \
    -rpcwallet="$wallet" "$@"
}

echo "════════════════════════════════════════════════════════════"
echo "  C0: preimage(H) ∧ opret-shaped RGB anchor (Simplicity)"
echo "════════════════════════════════════════════════════════════"

# ── 0. Node sanity ────────────────────────────────────────────────
ACTIVE=$(scli getdeploymentinfo | jq -r '.deployments.simplicity.active')
if [ "$ACTIVE" != "true" ]; then
  echo "✗ FAIL: simplicity deployment not active (run regtest_simplicity.sh up)"
  exit 1
fi
GENESIS=$(scli getblockhash 0)
LBTC=$(scli dumpassetlabels | jq -r '.bitcoin')
echo "  simplicity : ACTIVE"
echo "  genesis    : $GENESIS"
echo "  L-BTC      : $LBTC"

# ── 1. Wallet + seed from genesis OP_TRUE if needed ───────────────
scli createwallet w_c0 >/dev/null 2>&1 || scli loadwallet w_c0 >/dev/null 2>&1 || true
W_CT=$(scli_w w_c0 getnewaddress)
W_ADDR=$(scli_w w_c0 getaddressinfo "$W_CT" | jq -r '.unconfidential')

SPENDABLE=$(scli_w w_c0 listunspent 0 | jq 'length')
if [ "$SPENDABLE" -eq 0 ]; then
  echo
  echo "== Seeding w_c0 from OP_TRUE genesis output =="
  SCAN=$(scli scantxoutset start '["raw(51)"]')
  GEN_TXID=$(echo "$SCAN" | jq -r '.unspents[0].txid')
  GEN_VOUT=$(echo "$SCAN" | jq -r '.unspents[0].vout')
  # Spend free genesis coins into w_c0 (amount + fee must balance initialfreecoins)
  OUTPUTS=$(jq -n --arg w "$W_ADDR" --arg asset "$LBTC" \
    '[ {($w): 10, "asset": $asset}, {"fee": 20999990, "asset": $asset} ]')
  RAW=$(scli createrawtransaction "[{\"txid\":\"$GEN_TXID\",\"vout\":$GEN_VOUT}]" "$OUTPUTS")
  scli sendrawtransaction "$RAW" 0 >/dev/null
  scli generatetoaddress 2 "$W_ADDR" >/dev/null
  echo "  w_c0 balance: $(scli_w w_c0 getbalance | jq -r '.bitcoin // .') L-BTC"
fi

# ── 2. Covenant address ──────────────────────────────────────────
PREIMAGE=$(openssl rand -hex 32)
HASH=$(printf '%s' "$PREIMAGE" | xxd -r -p | sha256sum | awk '{print $1}')
MPC_ROOT=$(printf 'rgb-mpc-root-%s' "$PREIMAGE" | sha256sum | awk '{print $1}')

echo
echo "== Covenant =="
COV=$("$SIMP" address --program "$PROGRAM" --hash "$HASH")
COV_ADDR=$(echo "$COV" | jq -r '.address')
COV_SPK=$(echo "$COV" | jq -r '.spk_hex')
CMR=$(echo "$COV" | jq -r '.cmr')
echo "  program : $PROGRAM"
echo "  CMR     : $CMR"
echo "  address : $COV_ADDR (leaf 0xbe)"
echo "  hash    : $HASH"
echo "$COV" > "$OUT_DIR/covenant.json"

# ── 3. Fund ───────────────────────────────────────────────────────
echo
echo "== Fund covenant UTXO =="
FUND_TXID=$(scli_w w_c0 sendtoaddress "$COV_ADDR" $FUND_AMT)
scli generatetoaddress 1 "$W_ADDR" >/dev/null
FUND_VOUT=$(scli getrawtransaction "$FUND_TXID" 1 \
  | jq --arg spk "$COV_SPK" '.vout[] | select(.scriptPubKey.hex == $spk) | .n')
echo "  UTXO: $FUND_TXID:$FUND_VOUT ($FUND_SAT sat)"
echo "$FUND_TXID:$FUND_VOUT" > "$OUT_DIR/fund_utxo.txt"

DEST_SPK=$(scli_w w_c0 getaddressinfo "$W_ADDR" | jq -r '.scriptPubKey')

# ── A. Wrong preimage ─────────────────────────────────────────────
echo
echo "-- Negative A: wrong preimage --"
WRONG=$(openssl rand -hex 32)
if "$SIMP" spend --program "$PROGRAM" --hash "$HASH" \
    --preimage "$WRONG" --opret-payload "$MPC_ROOT" \
    --prev-txid "$FUND_TXID" --prev-vout "$FUND_VOUT" --input-value-sat $FUND_SAT \
    --dest-spk "$DEST_SPK" --fee-sat $FEE_SAT --lbtc-asset "$LBTC" \
    --genesis-hash "$GENESIS" 2>/dev/null; then
  echo "✗ FAIL: program satisfied with wrong preimage"
  exit 1
fi
echo "  ✓ program refuses to satisfy"

# ── B. Strip anchor after satisfaction ────────────────────────────
echo
echo "-- Negative B: strip anchor after satisfaction --"
TAMPERED=$("$SIMP" spend --program "$PROGRAM" --hash "$HASH" \
  --preimage "$PREIMAGE" --opret-payload "$MPC_ROOT" \
  --prev-txid "$FUND_TXID" --prev-vout "$FUND_VOUT" --input-value-sat $FUND_SAT \
  --dest-spk "$DEST_SPK" --fee-sat $FEE_SAT --lbtc-asset "$LBTC" \
  --genesis-hash "$GENESIS" --tamper-drop-anchor)
if OUT=$(scli sendrawtransaction "$TAMPERED" 2>&1); then
  echo "✗ FAIL: chain accepted strip-anchor spend: $OUT"
  exit 1
fi
echo "  ✓ CONSENSUS rejected anchor-less spend"
echo "    └─ $(echo "$OUT" | head -1)"
echo "$OUT" > "$OUT_DIR/strip_anchor_reject.txt"

# ── C. Compliant spend ────────────────────────────────────────────
echo
echo "-- Positive C: preimage + opret --"
GOOD=$("$SIMP" spend --program "$PROGRAM" --hash "$HASH" \
  --preimage "$PREIMAGE" --opret-payload "$MPC_ROOT" \
  --prev-txid "$FUND_TXID" --prev-vout "$FUND_VOUT" --input-value-sat $FUND_SAT \
  --dest-spk "$DEST_SPK" --fee-sat $FEE_SAT --lbtc-asset "$LBTC" \
  --genesis-hash "$GENESIS")
SPEND_TXID=$(scli sendrawtransaction "$GOOD")
scli generatetoaddress 1 "$W_ADDR" >/dev/null
echo "  ✓ accepted: $SPEND_TXID"
echo "$SPEND_TXID" > "$OUT_DIR/spend_txid.txt"

DECODED=$(scli getrawtransaction "$SPEND_TXID" 1)
OPRET_SPK=$(echo "$DECODED" | jq -r '.vout[0].scriptPubKey.hex')
if [ "$OPRET_SPK" != "6a20$MPC_ROOT" ]; then
  echo "✗ FAIL: vout[0] not opret: got $OPRET_SPK"
  exit 1
fi
echo "  ✓ vout[0] = OP_RETURN OP_PUSHBYTES_32 <MPC root>"

echo
echo "✅ C0 DONE — Simplicity covenant enforced on-chain"
echo "   artifacts under $OUT_DIR"
