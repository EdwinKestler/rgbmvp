# P3 closed — Browser lab console

**Status: CLOSED** for the lab definition of done (U0–U2 + audit gallery)  
**Date:** 2026-07-21  
**Plan:** [P3_PLAN.md](./P3_PLAN.md) · ADRs 001–004 accepted  
**Depends on:** [P1_CLOSED.md](./P1_CLOSED.md) · [P2_CLOSED.md](./P2_CLOSED.md)

---

## In scope (done)

| ID | Capability | Evidence |
|----|------------|----------|
| **U0** | Lab console shell + board + phase chips | `web/index.html`, `/demo`, `GET /v1/phases` |
| **U1** | Issue / transfer wizards | `POST /v1/rgb/issue`, `transfer`, console tabs; live issue/transfer/verify on Liquid Testnet |
| **U2** | Guided HTLC swap | `POST /v1/swap/init`, `POST /v1/swap/{id}/action`; session **demo-u2** phase **done** |
| **Audit** | BFA history UI | `/audit` · `POST /v1/audit/bfa` |
| **Security** | No browser seeds; preimage redacted | ADR-P3-001; `preimage_hex: null` on all GET swap responses |

### Live U2 proof (`demo-u2`)

| Step | Tx |
|------|-----|
| BTC fund | [`89a0f660…d721`](https://blockstream.info/testnet/tx/89a0f660beca4716ad69d169ea3d91cabd52dce87382066f52a8f0d0a815d721) |
| LQ fund | [`f97f3b4e…6c94`](https://blockstream.info/liquidtestnet/tx/f97f3b4e1b77bb07ff1f5c77e975a5984638ae3f2abbda82019fe52808e06c94) |
| LQ claim | [`f094a747…18c8`](https://blockstream.info/liquidtestnet/tx/f094a74764b5dbce03b9582d874ec1e7ebefa8daaec7925e57a17fbfdc5318c8) |
| BTC claim | [`e159bf6e…0ec9`](https://blockstream.info/testnet/tx/e159bf6eaffb256d65c7f0063c0cde6b55bc36d29d8c516f7d6412d9dceb0ec9) |

Order: claim Liquid first (preimage on-chain), then claim BTC. UI never displayed preimage.

---

## Explicitly out of P3 closure

| Item | Note |
|------|------|
| **U3** Marina / Jade / browser signer | Deferred |
| Full RGB-wrapped HTLC claim (re-anchor on claim tx) | Contract ids optional on session; claim path remains value HTLC |
| Public hosted labd | Localhost operator model only |
| Mainnet | Out of scope |
| Heavy SPA framework | Static HTML/JS by design |

---

## Browser tour (~15 minutes)

```bash
cargo build -p lab-cli
# Liquid: alice funded; optional BTC: btc-alice via .env + import-env
./target/debug/rgbmvp serve --bind 127.0.0.1:8080
```

| Step | URL / action |
|------|----------------|
| 1 | http://127.0.0.1:8080/demo — board + phase chips |
| 2 | http://127.0.0.1:8080/ — **Issue** NIA with alice |
| 3 | **Transfer** (+ optional broadcast) → **Verify** |
| 4 | **Swap** — init with wallet **names** `btc-alice` / `bob` (not addresses) |
| 5 | Guided: fund BTC → fund LQ → claim LQ → claim BTC |
| 6 | http://127.0.0.1:8080/audit — optional BFA history JSON |

API catalog: `GET /v1`.

---

## Operator notes

- Alice BTC field = wallet **name** `btc-alice`, not `tb1…`.  
- Optional twin `rgb:` contract ids are stored on the swap session for documentation; they do not change HTLC script paths in this lab cut.  
- Done swaps show a green celebrate banner (`phase === done`).

---

## Next track

- **S3** RGB-wrapped claim (CLI, testnet/localhost) — see [`ROADMAP_NEXT.md`](./ROADMAP_NEXT.md)  
- **U4** public-hosting security gate — **required before any Internet bind** (not a reopening of this P3 close)  
- Optional U3 hardware / external signer docs  
- C2 burn / C4 staking after ADRs (regtest)
