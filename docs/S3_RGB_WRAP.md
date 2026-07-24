# S3 — RGB-wrapped HTLC claim (CLI-first)

**Status:** Implemented (CLI + HTTP/browser) — live HTTP path `s3-browser-20260724-0112` → phase=done  
**Scenario:** [SCENARIOS.md](./SCENARIOS.md) `S3`  
**Roadmap:** [ROADMAP_NEXT.md](./ROADMAP_NEXT.md)

## Goal

One claim transaction per leg:

1. reveals the HTLC **preimage**
2. **closes** the RGB seal bound to the HTLC outpoint
3. creates a **successor seal** on the claimer output (`WitnessTx` vout)
4. carries a **tapret commitment** (vout0)
5. passes **`anchor_verify`**

Value-only HTLC (`rgb_wrap=false`) remains the P1 path.

## ADR-S3 (accepted defaults)

| Topic | Choice |
|-------|--------|
| Scope | CLI-first; BTC + Liquid |
| Fund | Value fund → RGB transfer onto **HTLC outpoint** |
| Claim | Multi-out: `vout0=tapret`, `vout1=claimer` + fee |
| Successor seal | `GraphSeal::with_blinded_vout(1)` |
| Leg 2 preimage | Prefer `extract-preimage` / `claim-btc --from-witness` |
| Done | Both value claims **and** required `claim_verify=valid` |
| Regression | Default `rgb_wrap=false` |

## Session schema (v2)

`.rgbmvp/swaps/<id>.json` gains:

- `version: 2`, `rgb_wrap: true`
- `btc_rgb` / `lq_rgb`: contract, amounts, fund/claim plan ids, seals, verify status

Legacy v1 sessions still load (`serde` defaults).

## CLI runbook (testnet / localhost)

Prerequisites: issued contracts on each chain (genesis seal still in the funding wallet).

```bash
# 1. Issue twins (if needed)
rgbmvp rgb issue --chain bitcoin-testnet --wallet btc-alice --ticker bRGB --supply 1000000
rgbmvp rgb issue --chain liquid-testnet  --wallet bob       --ticker lRGB --supply 1000000

# 2. Init S3 session
rgbmvp swap init --id s3-demo \
  --alice-btc btc-alice --bob-lq bob \
  --btc-contract '<bRGB contract id>' \
  --lq-contract  '<lRGB contract id>' \
  --rgb-wrap --csv-delay 6

# 3. Fund value + wrap RGB onto HTLC seals
rgbmvp swap fund-btc --id s3-demo --rgb-wrap
rgbmvp swap fund-lq  --id s3-demo --rgb-wrap

# 4. Alice claims Liquid (publishes preimage + re-anchors RGB)
rgbmvp swap claim-lq --id s3-demo

# 5. Bob extracts preimage from chain and claims BTC
rgbmvp swap extract-preimage --chain liquid --txid <lq_claim_txid> --id s3-demo
rgbmvp swap claim-btc --id s3-demo --from-witness

# 6. Inspect
rgbmvp swap status --id s3-demo
# phase should be "done" only when both claim_verify == "valid"
```

### Value-only regression

```bash
rgbmvp swap init --id value-only --csv-delay 6   # no --rgb-wrap
rgbmvp swap fund-btc --id value-only
# … same as P1
```

## Browser / HTTP runbook (localhost labd)

Prerequisites: funded `btc-alice` + `bob`, Axum labd (`rgbmvp serve`), two UTXOs on bob so LWK coin selection does not spend the RGB **issue seal** when funding the HTLC.

### Seal safety (important)

LWK may consolidate **all** L-BTC UTXOs when sending HTLC value. If the only large UTXO is the RGB issue seal, wrap fails with `Missing wallet UTXO` for the seal.

**Recommended order:**

1. Split bob: `rgbmvp wallet send --from bob --to bob --amount-sats 20000`
2. Issue Liquid RGB on the **smaller** UTXO (`--seal txid:vout`)
3. Keep a **larger non-seal** UTXO for HTLC value  
4. Or: fund HTLC value first, re-issue RGB on remaining change, then `fund_lq` with `rgb_wrap` (wrap-only / reuse HTLC)

### Console steps

1. Open `http://127.0.0.1:8080` → **Swap**
2. Mode = **RGB-wrapped S3**
3. Issue twins (Issue tab or CLI), **Load contracts from lab…**
4. Init swap with both `rgb:` ids
5. Guided buttons: Fund BTC+wrap → Fund LQ+wrap → Claim LQ → Claim BTC (from_witness)
6. Phase **done** only when both legs show `claim_verify=valid`
7. Preimage never appears in UI / `GET /v1/swap/*`

### HTTP equivalent (operator)

```bash
# init
curl -sS -X POST http://127.0.0.1:8080/v1/swap/init -H 'content-type: application/json' -d '{
  "id": "s3-browser-demo",
  "csv_delay": 6,
  "alice_btc": "btc-alice",
  "bob_lq": "bob",
  "btc_contract": "rgb:…",
  "lq_contract": "rgb:…",
  "rgb_wrap": true
}'

# fund + wrap (idempotent re-run OK)
curl -sS -X POST http://127.0.0.1:8080/v1/swap/s3-browser-demo/action \
  -H 'content-type: application/json' \
  -d '{"action":"fund_btc","amount_sats":5000,"fee_sats":800,"rgb_wrap":true}'

curl -sS -X POST http://127.0.0.1:8080/v1/swap/s3-browser-demo/action \
  -H 'content-type: application/json' \
  -d '{"action":"fund_lq","amount_sats":5000,"rgb_wrap":true}'

curl -sS -X POST http://127.0.0.1:8080/v1/swap/s3-browser-demo/action \
  -H 'content-type: application/json' \
  -d '{"action":"claim_lq","fee_sats":300}'

# from_witness defaults true for S3 sessions
curl -sS -X POST http://127.0.0.1:8080/v1/swap/s3-browser-demo/action \
  -H 'content-type: application/json' \
  -d '{"action":"claim_btc","from_witness":true,"fee_sats":500}'

curl -sS http://127.0.0.1:8080/v1/swap/s3-browser-demo | jq '{phase, mode, not_done_reason, btc: .btc_rgb.claim_verify, lq: .lq_rgb.claim_verify, preimage_hex}'
```

### Live evidence (HTTP path, 2026-07-24)

| Field | Value |
|-------|--------|
| Session | `s3-browser-20260724-0112` |
| Phase | `done` |
| BTC `claim_verify` | `valid` |
| LQ `claim_verify` | `valid` |
| GET preimage | `null` / redacted |
| BTC fund | `cd07bf4a…7995` |
| LQ fund | `4064c864…75bd` |
| LQ claim | `ee39d13b…d6af` |
| BTC claim | (session `btc_claim_txid` after claim) |

Recover path used when LWK spent the first LQ issue seal during fund: re-issue on change UTXO + `set_contracts` + wrap-only `fund_lq`.

## Layout of claim txs

```text
vin0:  HTLC P2WSH (sig + preimage + OP_IF + witness script)
vout0: P2TR tapret commitment (DEMO_INTERNAL_XONLY + MPC root)
vout1: claimer P2WPKH (RGB successor seal = WitnessTx:1)
[+ fee output on Liquid]
```

## Negative checks

| Case | Expected | Automation |
|------|----------|------------|
| claim without fund-wrap | error: missing `fund_transition_opid_hex` | `lab_rgb::swap::require_fund_wrap_for_claim` unit test |
| wrong / invalid claim_verify | phase stays `claimed_btc` not `done` | `invalid_claim_verify_never_done` |
| one valid + one invalid leg | not `done` | `one_valid_one_invalid_leg_blocks_done` |
| extract from refund witness | error (empty IF / short stack) | `htlc` refund extract tests |
| wrong preimage length | error | `extract_rejects_wrong_preimage_length` |
| `--from-witness` hash mismatch | error | `check_preimage_matches_session` |
| malformed tx hex | error | `extract_rejects_malformed_tx_hex` |
| contract id mismatch | error | `check_leg_contract_matches_session` |
| value-only path | unchanged; no RGB fields required | `value_only_done_without_rgb_fields` |
| GET public view | preimage never present | `lab_api::public_swap_view` test |

Still **manual / live CI (optional):** full RGB client verify with wrong commitment SPK on testnet, dual-leg broadcast+verify matrix, consignment corruption end-to-end.

Run offline matrix:

```bash
cargo test -p lab-rgb -p lab-api --lib
```

## Surfaces

| Surface | S3 status |
|---------|-----------|
| CLI `swap *` | **Primary** — fund-wrap/claim via `lab_api::SwapService` / `lab_api::s3` |
| `GET /v1/swap/{id}` | Public RGB leg metadata + `not_done_reason` / `mode` (no preimage) |
| `POST /v1/swap/init` | `rgb_wrap`, `btc_contract`, `lq_contract` |
| `POST /v1/swap/{id}/action` | `fund_btc`/`fund_lq` honor `rgb_wrap` (idempotent wrap); `claim_*` via SwapService; S3 `claim_btc` defaults `from_witness` |
| Browser console | Mode selector: value HTLC vs S3; RGB leg panel; guided actions ([web/index.html](../web/index.html)) |
| Public Internet | U4 read-only; mutations require Bearer |

## Related

- [P1_CLOSED.md](./P1_CLOSED.md) — value path closed; RGB wrap was deferred  
- [P1_SWAP_PLAN.md](./P1_SWAP_PLAN.md) Slice C (R0–R2)  
- [WITNESS_TX.md](./WITNESS_TX.md) — Liquid witness adapter  
