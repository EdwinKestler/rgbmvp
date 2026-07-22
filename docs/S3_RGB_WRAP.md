# S3 — RGB-wrapped HTLC claim (CLI-first)

**Status:** Implemented (CLI) — 2026-07-22  
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

## Layout of claim txs

```text
vin0:  HTLC P2WSH (sig + preimage + OP_IF + witness script)
vout0: P2TR tapret commitment (DEMO_INTERNAL_XONLY + MPC root)
vout1: claimer P2WPKH (RGB successor seal = WitnessTx:1)
[+ fee output on Liquid]
```

## Negative checks (manual / future CI)

| Case | Expected |
|------|----------|
| claim without fund-wrap | error: missing `fund_transition_opid_hex` |
| wrong commitment SPK | `claim_verify=invalid` → phase stays `claimed_btc` not `done` |
| extract from refund witness | error (empty IF branch) |
| `--from-witness` hash mismatch | error |
| value-only path | unchanged; no RGB fields required |

## Surfaces

| Surface | S3 status |
|---------|-----------|
| CLI `swap *` | **Primary** |
| `GET /v1/swap/{id}` | Exposes public RGB leg metadata (no preimage) |
| Browser console mutations | Value path; RGB wrap optional later |
| Public Internet | **Blocked until U4** |

## Related

- [P1_CLOSED.md](./P1_CLOSED.md) — value path closed; RGB wrap was deferred  
- [P1_SWAP_PLAN.md](./P1_SWAP_PLAN.md) Slice C (R0–R2)  
- [WITNESS_TX.md](./WITNESS_TX.md) — Liquid witness adapter  
