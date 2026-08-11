# T1 — first live swap (evidence)

**Date:** 2026-08-10 · **Session:** `live-20260810-2325` · **Operator-run, CLI**
**Result:** ✅ complete — phase `done`, first attempt, no retries, no stuck funds.

Purpose: validate the assumptions behind
[TESTNET_PUBLIC_SWAPS.md](./TESTNET_PUBLIC_SWAPS.md) before any public exposure.
Two things were unproven until this run: **whether the fee defaults confirm**,
and **whether the cross-chain preimage extraction works end to end live**.

Parameters (value-only path, the T1 public default):

| | |
|---|---|
| Legs | 1,000 sats each side |
| `rgb_wrap` | `false` |
| `csv_delay` | 6 blocks |
| Wallets | `btc-alice` (BTC leg) ↔ `bob` (Liquid leg) |

---

## 1. Transactions

| Step | Chain | Txid | Block |
|---|---|---|---|
| Fund HTLC | BTC | [`ecbe50b8…0e97`](https://blockstream.info/testnet/tx/ecbe50b8ac1e20a1fc43c459b70a6f6bca926d3b88ac640ce227ce055c560e97) | 5,105,513 |
| Fund HTLC | Liquid | [`a04ce3b1…1b17`](https://blockstream.info/liquidtestnet/tx/a04ce3b11137171f9107a4f7d13e6a0f922204487b5a5561b6ca77ffdeb41b17) | 2,567,829 |
| Claim (preimage revealed) | Liquid | [`535c6ea9…8125`](https://blockstream.info/liquidtestnet/tx/535c6ea985a81b834e5d31459f29763bc133a00392b63e4fdfb84717485a8125) | 2,567,831 |
| Claim from witness | BTC | [`486d9cc1…b9d7`](https://blockstream.info/testnet/tx/486d9cc1ce9403d70c1f8b55081b999d80bc2d5c690e7d3491ba5361094cb9d7) | 5,105,513 |

`claim-btc --from-witness` recovered the preimage from the **Liquid claim's
witness stack**, not the local session file — the S3 extraction path, proven
against live chain data.

---

## 2. Measured fees (the primary purpose of this run)

| Tx | vsize | Fee | Rate |
|---|---|---|---|
| BTC fund | 152 vB | 800 sats | **5.25 sat/vB** |
| BTC claim | 138 vB | 500 sats | **3.63 sat/vB** |
| Liquid claim | 219 vB | 300 sats | — |
| (BTC sweep, 5-in/1-out) | 380 vB | 700 sats | **1.84 sat/vB** |

**Findings**

- The repo's long-standing defaults (800 fund / 500 claim) are **3.6–5.3× the
  1 sat/vB relay floor**. They work, with room to spare.
- vbyte prediction was accurate to within ~4% on every transaction (predicted
  153/133/382, actual 152/138/380), so the estimation method is sound — what
  was wrong earlier was the *risk appetite*, not the arithmetic.
- The earlier T1 draft's ~200 sats would have been ~1.3–1.5 sat/vB. The sweep
  relayed at 1.84, so 200 likely *would* have worked — but "likely relays" is
  not an acceptable margin for a transaction holding a live HTLC.
- **Not yet retuned.** A plausible target is ~450 fund / ~300 claim (≈3.0 and
  ≈2.2 sat/vB), which would cut a swap from 1,300 → ~750 sats and roughly double
  budget runway. Deferred until 2–3 more swaps of evidence exist; one sample is
  not a fee policy.

---

## 3. Value flow (confirms the §1a correction)

| Wallet / address | Before | After | Δ |
|---|---|---|---|
| `btc-alice` | 169,531 | 167,731 | **−1,800** |
| `bob-claimer` exit | 0 (swept) | 500 | **+500** |

Exactly the documented model: 1,000 leg + 800 fund fee leaves `btc-alice`;
1,300 burns to miners; 500 lands at the `bob-claimer` demo exit address.

**The 500 sats reappeared at the same address swept ~1 hour earlier**
([`53d5f8aa…73a7`](https://blockstream.info/testnet/tx/53d5f8aae6d8980423c86dff5c4a3f76f8436091d67a8fe5357b7cd0995973a7),
which recovered 35,924 sats of historical strandings). This is live confirmation
that the W5 sweep is **required, not optional** — without it every swap silently
bleeds the scarce BTC wallet.

---

## 4. Operational observations

- **Chained spend relayed and confirmed.** The BTC claim spent the funding
  output while the funding tx was still unconfirmed; both landed in the **same
  block (5,105,513)**. Good robustness signal for the W1 driver, which retries
  rather than requiring confirmations between steps.
- **Liquid is fast, Bitcoin is the pacing item.** Liquid funding confirmed in
  ~1 minute; the BTC leg waited on testnet blocks. The driver's 90-minute wall
  ceiling is adequate for a normal block cadence but remains the component most
  exposed to a slow testnet stretch — still untested under one.
- **`pick_largest_utxo` does not filter for confirmed UTXOs.** It selects purely
  by value, so it will happily chain off unconfirmed change. Harmless here, but
  worth knowing before an automated run.
- No refund path was exercised (the swap completed). The CSV refund and the
  automated W5 watcher remain **unproven live**.

---

## 5. What this run does and does not prove

**Proven**
- The value-only HTLC swap completes end to end on live testnet.
- Cross-chain preimage extraction from a Liquid witness works against real data.
- The fee defaults confirm comfortably.
- The demo-exit drain is real, and the sweep recovers it.

**Still unproven**
- The **W1 automated driver** — this run was CLI, step by step. The
  `POST /v1/demo/swap` orchestration path has still never completed a swap.
- Turnstile in front of a real request.
- The **W5 refund watcher** and CSV refund path.
- Behaviour under a slow-block stretch, and W4 budget persistence across a
  restart *during* a live swap.

A public run needs at minimum the automated driver exercised end to end.
