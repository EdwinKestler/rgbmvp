# P1 plan — BTC ↔ Liquid RGB atomic swap (HTLC)

**Status:** plan locked for implementation (2026-07-21)  
**Networks:** Bitcoin **testnet3** (`tb1…`) + Liquid **testnet**  
**Not in scope:** CLN, mainnet, trusted bridges

---

## 1. Assessment (where we are)

| Area | Status | Notes |
|------|--------|--------|
| Liquid RGB issue/transfer/verify | **Done (P0)** | WitnessTx patch, tapret, live proof |
| Liquid wallets alice/bob/carol/maker | **Done** | Reusable fixtures + rebalance |
| Bitcoin testnet stack | **Advanced** | `lab-btc` + RGB issue/transfer/verify on BTC testnet **live** |
| HTLC / hashlock scripts | **Not in repo** | Available in spike `swap/htlc.rs` (copy/adapt) |
| Swap coordinator / state machine | **Missing** | Needed for S4 UX |
| BTC funding for provided key | **Funded** | ~189_026 sats confirmed; import via `rgbmvp btc import-env` |

### Provided BTC testnet key (operator)

| Field | Value |
|-------|--------|
| Address (P2WPKH) | `tb1q85aadpqgzjgrgp69gf2ejf0883yx7s9wy85h4p` |
| Network | Bitcoin testnet3 (WIF prefix `c` = compressed testnet) |
| Role in lab | **`btc-alice`** — seal/funding key for Bitcoin RGB leg |
| Storage | **`.env` only** (`BTC_TESTNET_WIF`) — never commit WIF to git |

Address may be listed in `fixtures/testnet_btc.json` (public). Private material stays in `.env`.

---

## 2. What “atomic swap of twins” means

RGB contracts are **genesis-bound to one chain**. We do **not** move one `rgb:` id across chains.

```text
Alice holds:  RGB-A on Bitcoin testnet   (e.g. ticker bRGB)
Bob holds:    RGB-B on Liquid testnet    (e.g. ticker lRGB)   [or maker]

They swap ownership via linked HTLCs:

  BTC leg:  Alice locks RGB-A under HTLC(H, claim=Bob, refund=Alice, CSV=T)
  LQ  leg:  Bob   locks RGB-B under HTLC(H, claim=Alice, refund=Bob,   CSV=T')

  Alice claims on Liquid by revealing preimage s (H = SHA256(s))
  → s is public on Liquid
  Bob claims on Bitcoin with the same s

  If Alice never claims: both refund after CSV timeouts.
```

Atomicity: Alice cannot take RGB-B without publishing `s`; once `s` is public, Bob can always take RGB-A. Neither ends with both.

Spike reference: `demo_swap.sh`, `demo_htlc.sh`, `demo_htlc_rgb.sh`, `swap/htlc.rs`.

---

## 3. Actors and wallets

| Actor | Liquid | Bitcoin |
|-------|--------|---------|
| **Alice** | fixture `alice` (LWK) | **btc-alice** = provided `tb1q85aad…` key |
| **Bob** | fixture `bob` or `maker` | Optional second BTC key later; v1 can use Alice-funded only for seals if Bob is Liquid-only |
| **Coordinator** | labd process (local) | same |

**Minimal viable cast (v1 demo):**

- Alice: BTC key + Liquid alice  
- Bob: Liquid bob (and, if needed, a second BTC key derived later)  

For a true two-sided BTC+LQ swap Bob also needs a BTC claim key. Plan:

- **P1a:** Alice (BTC) ↔ Bob (Liquid) with Bob’s BTC claim pubkey from a **new fixture** `btc-bob` (generate + document) if Bob must claim on-chain BTC.  
- **P1b:** Alice self-plays both roles with two BTC keys for CI (not public demo).

Recommended public demo: **Alice btc-alice + Liquid alice** vs **Bob Liquid bob + btc-bob** (generate `btc-bob` WIF into `.env`).

---

## 4. Scenario map (implementation order)

### Slice A — Bitcoin foundation (blocking)

| ID | Work | Exit |
|----|------|------|
| B0 | `lab-btc` crate: load WIF, address, UTXO list via Esplora testnet, send/sign P2WPKH | `rgbmvp btc status` / `btc balance` |
| B1 | Import `btc-alice` from `BTC_TESTNET_WIF` | Funded address shows UTXOs after faucet |
| B2 | RGB NIA issue on **Bitcoin testnet** (`ChainNet::BitcoinTestnet3` or Testnet4 — pin after probe) + tapret transfer using BTC seal | Contract id + plan like Liquid P0 |
| B3 | `rgbmvp rgb issue --chain bitcoin-testnet --wallet btc-alice` | Mirror of Liquid CLI |

**Faucet:** [https://bitcoinfaucet.uo1.net/](https://bitcoinfaucet.uo1.net/) or similar testnet faucets → `tb1q85aad…`.

### Slice B — HTLC primitives (S1–S2)

| ID | Work | Exit |
|----|------|------|
| H0 | Port spike `htlc_witness_script` / P2WSH encode (BTC `tb` + Liquid `tex`/Elements P2WSH) | Unit tests: script + address deterministic |
| H1 | Minimal hashlock (spike `swap-hashlock`) then full HTLC (claimer+CSV refund) | `rgbmvp swap htlc-address` |
| H2 | Claim / refund tx builders for BTC and Liquid | Negative tests: wrong preimage, early refund |

### Slice C — RGB-wrapped claim (S3)

| ID | Work | Exit |
|----|------|------|
| R0 | Fund HTLC with RGB-sealed value: transfer that creates new seal = HTLC outpoint | Plan + broadcast each chain |
| R1 | Claim tx: preimage + claimer sig + **tapret/opret anchor at vout0** re-seating RGB | `anchor_verify` on claim witness |
| R2 | Extract preimage from claim witness for counterparty | CLI `swap extract-preimage --txid` |

### Slice D — Coordinator + demo (S4–S5)

| ID | Work | Exit |
|----|------|------|
| C0 | Swap session state machine on disk: `created → funded_btc → funded_lq → claimed_lq → claimed_btc → done` (+ refund paths) | `.rgbmvp/swaps/<id>.json` |
| C1 | CLI: `swap init|fund|claim|refund|status` | One-command guided demo |
| C2 | API: `POST /v1/swap/*` + web status panel | Browser-readable status |
| C3 | Round-trip optional (S5) after happy path | Document supplies conserved |

### Slice E — Hardening

| ID | Work |
|----|------|
| T0 | CSV delays sensible for testnet (e.g. 6–72 blocks BTC; Liquid ~same relative) |
| T1 | Fees/dust: commitment 500–1000 sats; HTLC fund ≥ 5–10k sats testnet |
| T2 | Never log preimage or WIF; store preimage only under `.rgbmvp/swaps/` mode 600 |
| T3 | Document CLN as out of path (S6) |

---

## 5. Architecture (target)

```text
rgbmvp swap …
        │
        ▼
┌───────────────────┐     ┌────────────────────┐
│ lab-swap          │────▶│ lab-rgb (issue/    │
│ state machine     │     │ transfer/verify)   │
│ HTLC scripts      │     └─────────┬──────────┘
└─────────┬─────────┘               │
          │              ┌──────────┴──────────┐
          ▼              ▼                     ▼
   ┌────────────┐  ┌───────────┐        ┌────────────┐
   │ lab-btc    │  │ lab-chain │        │ Esplora    │
   │ WIF/sign   │  │ LWK/LQ    │        │ BTC + LQ   │
   └────────────┘  └───────────┘        └────────────┘
```

Reuse: vendored `WitnessTx` patch; Liquid path already proven.  
New: Bitcoin P2WPKH wallet, BTC RGB seal txs, dual-chain HTLC, session store.

---

## 6. CLI sketch (end state)

```bash
# BTC
rgbmvp btc import-wif --name btc-alice --env BTC_TESTNET_WIF
rgbmvp btc balance --name btc-alice
rgbmvp btc address --name btc-alice

# Twins
rgbmvp rgb issue --chain bitcoin-testnet --wallet btc-alice --ticker bRGB --supply 1000000
rgbmvp rgb issue --chain liquid-testnet  --wallet bob       --ticker lRGB --supply 1000000

# Swap
rgbmvp swap init --alice-btc btc-alice --alice-lq alice --bob-lq bob \
  --btc-contract <bRGB> --lq-contract <lRGB> --csv 24
rgbmvp swap fund-btc --id <swap>
rgbmvp swap fund-lq  --id <swap>
rgbmvp swap claim-lq --id <swap>    # Alice reveals s
rgbmvp swap claim-btc --id <swap>   # Bob uses s
rgbmvp swap status --id <swap>

rgbmvp serve   # /v1/swap/{id} status for web
```

---

## 7. Dependencies / risks

| Risk | Mitigation |
|------|------------|
| BTC address unfunded | Faucet before B1 live tests |
| testnet3 vs testnet4 | Probe Esplora; pin `ChainNet` + HRP `tb` in code |
| BTC RGB broadcast without bitcoind | Build raw/signed txs + Esplora broadcast (like Liquid) |
| TapretFirst output order on BTC | Same discipline as Liquid P0; unit-test vout layout |
| LWK has no BTC | Separate `lab-btc` with `bitcoin` crate (already in tree via rgb-consensus) |
| Timeout griefing | Document CSV; refund path automated in CLI |
| Secret leakage | `.env` + `0600` swap files; never in project memory |

---

## 8. Success criteria (P1 exit)

1. `btc-alice` funded; can issue **bRGB** on Bitcoin testnet and verify anchor.  
2. Bob (or maker) issues **lRGB** on Liquid testnet.  
3. Full HTLC fund both sides; Alice claims Liquid (preimage public); Bob claims BTC.  
4. Both claim txs pass RGB `anchor_verify` (or seal_closure + dbc where applicable).  
5. Refund path tested at least offline or with short CSV on one chain.  
6. Runbook in `docs/TESTNET_WALLETS.md` (or this file) + `swap status` machine-readable.  
7. No CLN required.

---

## 9. Implementation sequence (next coding sessions)

1. **Env + fixture docs** for `btc-alice` (this PR/doc pass).  
2. **lab-btc**: WIF import, balance, UTXOs, send (Esplora).  
3. **RGB on Bitcoin testnet** (extend `lab-rgb` + CLI `--chain`).  
4. **lab-swap** HTLC scripts + unit tests.  
5. **Fund legs** (RGB transfer into HTLC seals).  
6. **Claim/refund** + preimage extract.  
7. **Coordinator CLI + `/v1/swap`**.  
8. **Live testnet demo** + record txids in SCENARIOS.

---

## 10. Operator checklist (before code starts)

```bash
# 1) Put WIF in local .env (gitignored) — never commit real WIF
# BTC_TESTNET_WIF=<testnet-wif-from-your-operator-env>
# BTC_TESTNET_ADDRESS=tb1q85aadpqgzjgrgp69gf2ejf0883yx7s9wy85h4p

# 2) Fund from a testnet faucet
# Address: tb1q85aadpqgzjgrgp69gf2ejf0883yx7s9wy85h4p

# 3) Confirm
curl -s https://blockstream.info/testnet/api/address/tb1q85aadpqgzjgrgp69gf2ejf0883yx7s9wy85h4p | jq .
```

Liquid side already funded (alice/bob/maker).
