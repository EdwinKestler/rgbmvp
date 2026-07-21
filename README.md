# rgbmvp — RGB on Liquid Testnet Lab

Public lab for **RGB client-side assets anchored on Liquid** (and Bitcoin testnet twins), with a CLI, verify API, and read-only demo board.

Inspired by [KaleidoSwap’s RGB-on-Liquid work](https://github.com/kaleidoswap/rgb-on-liquid-spike) / [writeup](https://x.com/i/status/2077733143428190555).

| | |
|--|--|
| **Networks** | Liquid Testnet · Bitcoin Testnet (P1) · no mainnet |
| **Status** | **P0 + P1 closed** · Demo v0 read-only · P2 Simplicity research next |
| **UI** | Verify + swap status + `/demo` board (no browser keys) |

---

## Try on Liquid Testnet in ~15 minutes

**No sudo required** if you already have Rust and Python 3.11+.  
**No Docker required.** Redis is optional (project-memory only).

### 0) Prerequisites

```bash
rustc --version    # 1.85+ recommended
python3 --version  # 3.11+
# If missing packages later:
#   sudo apt-get install -y build-essential pkg-config libssl-dev clang cmake
```

### 1) Clone & build (~5 min first time)

```bash
git clone https://github.com/EdwinKestler/rgbmvp.git
cd rgbmvp
cargo build -p lab-cli
# binary: ./target/debug/rgbmvp
```

Optional Python glue (project memory / unit tests):

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
pytest -q
```

### 2) Lab wallets (Liquid fixtures)

```bash
./scripts/bootstrap_testnet_wallets.sh
# or: ./target/debug/rgbmvp wallet bootstrap-testnet

./target/debug/rgbmvp wallet address --name alice
# Fund alice with testnet L-BTC: https://liquidtestnet.com/faucet
./target/debug/rgbmvp wallet balance --name alice
```

Fixture roles: **alice / bob / carol / maker** (public testnet BIP39 — see [`fixtures/testnet_wallets.json`](fixtures/testnet_wallets.json)).  
Never use fixture mnemonics on mainnet.

### 3) RGB issue → transfer → verify (Liquid)

```bash
./target/debug/rgbmvp net status
# expect rgb_ready: true

./target/debug/rgbmvp rgb issue \
  --chain liquid-testnet --wallet alice \
  --ticker tRGB --name "Test RGB" --supply 1000000

# Copy contract_id from output, then:
./target/debug/rgbmvp rgb transfer \
  --chain liquid-testnet --wallet alice \
  --contract 'rgb:…' --amount 600000 --broadcast

# After broadcast:
./target/debug/rgbmvp rgb verify \
  --plan tRGB-… --txid <anchor_txid>
# expect status: valid
```

### 4) Web: verify API + demo board

```bash
./target/debug/rgbmvp serve --bind 127.0.0.1:8080
```

| URL | What |
|-----|------|
| http://127.0.0.1:8080/ | RGB verify form + swap status |
| http://127.0.0.1:8080/demo | **Read-only** balances + swaps / plans / proofs |
| `GET /v1/health` | Network + `rgb_ready` |
| `POST /v1/rgb/verify` | `{ "plan_id": "…", "txid": "…" }` |
| `GET /v1/swap/{id}` | Swap status (**preimage redacted**) |
| `GET /v1/demo/wallets` | Lab wallet board JSON |
| `GET /v1/demo/activity` | Swaps, plans, proofs |

Example verify:

```bash
curl -s -X POST http://127.0.0.1:8080/v1/rgb/verify \
  -H 'Content-Type: application/json' \
  -d '{"plan_id":"tRGB-…","txid":"…"}' | jq .
```

### 5) (Optional) P1 HTLC snapshot

P1 is **closed** (fund → claim LQ → claim BTC + refund CLI). Details and live txids:  
**[`docs/P1_CLOSED.md`](docs/P1_CLOSED.md)**

```bash
# Bitcoin testnet key stays in local .env only (never commit WIF)
cp .env.example .env
# set BTC_TESTNET_WIF + BTC_TESTNET_ADDRESS, fund tb1… faucet, then:
./target/debug/rgbmvp btc import-env
./target/debug/rgbmvp swap init --id demo --csv-delay 6 --alice-btc btc-alice --bob-lq bob
# fund-btc / fund-lq / claim-lq / claim-btc  (see P1_CLOSED.md)
```

---

## Phase status

| Phase | Theme | Status |
|-------|--------|--------|
| **0** | LWK + chain health | Done |
| **P0** | RGB on Liquid Testnet | Done |
| **P1** | BTC ↔ Liquid HTLC twin swap | **Closed** — [P1_CLOSED.md](docs/P1_CLOSED.md) |
| **Demo v0** | Read-only `/demo` board | Done |
| **P2** | Simplicity seal covenants | Next (research) |
| **P3** | Full browser wallet UI | Later |

---

## Important distinctions

- **Native Liquid assets** (LWK `issue_asset`) ≠ **RGB contracts** (off-chain consignments + seal UTXO + commitment).
- Cross-chain = **atomic swap of twin RGB contracts**, not moving one contract id.
- **Mainnet is out of scope** until upstream RGB `WitnessTx` review and hardening.

---

## Docs

| Doc | Content |
|-----|---------|
| [docs/P1_CLOSED.md](docs/P1_CLOSED.md) | P1 scope, live evidence, refund notes |
| [docs/P2_PLAN.md](docs/P2_PLAN.md) | P2 Simplicity seal covenants — plan & milestones |
| [docs/P2_SIMPLICITY.md](docs/P2_SIMPLICITY.md) | P2 R0 pins, Docker regtest, ADR |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Layers, `/v1` API, privacy |
| [docs/SCENARIOS.md](docs/SCENARIOS.md) | Scenario ladder |
| [docs/STACK.md](docs/STACK.md) | LWK, RGB, toolchain |
| [docs/TESTNET_WALLETS.md](docs/TESTNET_WALLETS.md) | alice/bob/carol/maker + btc-alice |
| [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md) | Optional Redis source index |
| [AGENTS.md](AGENTS.md) | Agent instructions |

---

## Layout

```text
crates/     lab-core, lab-chain (LWK), lab-btc, lab-rgb, lab-api, lab-cli
vendor/     rgb-consensus WitnessTx patch (Apache-2.0)
web/        index.html (verify/swap) · demo.html (read-only board)
fixtures/   testnet wallet roles (public BIP39 / BTC address only)
docs/       architecture, scenarios, P1 closure
scripts/    bootstrap_testnet_wallets.sh, project_memory.py
```

---

## Privacy & safety

- Never commit `.env`, `.rgbmvp/`, WIF, or real secrets.
- Fixture mnemonics are **testnet-only** and public by design.
- Web UI never receives private keys; preimage redacted on `GET /v1/swap/{id}`.
- Do not `FLUSHDB` / `FLUSHALL` on shared Redis.

---

## License / provenance

- Lab code: see repository license headers as added.
- `vendor/rgb-consensus-patched/`: Apache-2.0 derivative of [rgb-protocol/rgb-consensus](https://github.com/rgb-protocol/rgb-consensus) with KaleidoSwap-style `WitnessTx` patch (see `vendor/.../PATCH.md`).

## References

- [Blockstream/lwk](https://github.com/Blockstream/lwk) · [docs.liquid.net](https://docs.liquid.net/docs/get-started)
- [liquidtestnet.com/faucet](https://liquidtestnet.com/faucet) · [Liquid testnet explorer](https://blockstream.info/liquidtestnet/)
- [kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike)
