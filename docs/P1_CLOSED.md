# P1 closed — BTC ↔ Liquid HTLC atomic swap (lab)

**Status: CLOSED** for protocol completeness of the public-testnet lab path  
**Date: 2026-07-21**

## In scope (done)

| Capability | Evidence |
|------------|----------|
| Bitcoin testnet wallet | `btc-alice` via `BTC_TESTNET_WIF` |
| RGB on Bitcoin testnet | issue `bRGB`, transfer broadcast, verify **valid** |
| RGB on Liquid testnet | P0 path (issue/transfer/verify) |
| Dual HTLC scripts | BTC claimer=bob / LQ claimer=alice |
| Fund both legs | `swap fund-btc` / `swap fund-lq` |
| Claim path | Alice `claim-lq` → Bob `claim-btc` → phase **done** |
| Refund path | `swap refund-btc` / `swap refund-lq` (CSV maturity required) |
| Web status | `GET /v1/swap/{id}` (preimage redacted) |

Live happy-path session: **`p1-live`** (phase `done`).

## Explicitly out of P1 closure (deferred)

- RGB-**wrapped** claim (re-anchor RGB on the claim tx itself) — polish  
- Round-trip S5  
- Public multi-BTC cast beyond `btc-alice`  
- Full browser send/swap theatre (Demo v1+)  
- CLN / Lightning  

## Operator notes — refund

Refund spends use `nSequence = csv_delay` (BIP-68 relative blocks). Nodes reject early refunds.

```bash
# After fund, wait ≥ csv_delay confirmations, then (if not claimed):
rgbmvp swap refund-btc --id <swap>
rgbmvp swap refund-lq --id <swap>
```

If claim already happened, refund correctly fails.

## Next track

1. **Demo v0** — read-only `/demo` board (balances + activity).  
2. **P2 research** — Simplicity seal covenants — full plan: [`P2_PLAN.md`](./P2_PLAN.md) (scenarios C0+ in [`SCENARIOS.md`](./SCENARIOS.md)).
