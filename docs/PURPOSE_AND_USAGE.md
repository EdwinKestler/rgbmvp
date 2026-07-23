# Purpose and usage (humans)

This document explains **what `rgbmvp` is for**, **what it is not**, and **how a person uses it**.  
For the community vision and real-world use cases, see **[MANIFESTO.md](./MANIFESTO.md)**.  
For a short quickstart, see the root [README.md](../README.md).  
For AI/agent workflows (including Redis project memory), see [M2M.md](./M2M.md).

---

## 1. Purpose of the repository

**rgbmvp** is a **public, testnet-only lab** for proving that:

1. **RGB client-side assets** can be issued, transferred, and verified when anchored on **Liquid** (not only Bitcoin).  
2. **Cross-chain value** can move without a custodian by **atomically swapping twin RGB/HTLC legs** (Bitcoin testnet ↔ Liquid testnet).  
3. **Liquid-native programmability** (Simplicity covenants + backed-asset audit) can harden seals and mint rules beyond plain Script.  
4. A **browser lab console** can drive the same flows through a shared **`/v1` JSON API** without putting seeds in the browser.

It is inspired by [kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike) and related public writeups, and is structured as a **phased ladder** (P0 → P3) so each claim is demonstrable.

### Status (lab closed through P3)

| Phase | Theme | Status |
|-------|--------|--------|
| **0 / P0** | LWK + RGB issue/transfer/verify on Liquid Testnet | Done |
| **P1** | BTC ↔ Liquid HTLC twin swap | Closed — [P1_CLOSED.md](./P1_CLOSED.md) |
| **P2** | Simplicity seal covenants + BFA audit | Closed — [P2_CLOSED.md](./P2_CLOSED.md) |
| **P3** | Browser lab console (issue, transfer, verify, guided swap, audit) | Closed — [P3_CLOSED.md](./P3_CLOSED.md) |

### What this repo is **not**

| Not this | Why |
|----------|-----|
| A production mainnet wallet | Mainnet is out of scope until upstream RGB/Liquid review and hardening |
| A custodian or bridge that “moves” one RGB contract id across chains | Cross-chain = **swap of twins**, separate contract ids |
| Liquid **native** issued assets branded as RGB | LWK `issue_asset` ≠ `rgb:` contracts |
| A Lightning product | CLN is optional/docs only; not required for P0–P3 |
| A hosted multi-user service | Default is **local operator labd** (`127.0.0.1`) |

---

## 2. Two asset systems (do not mix them up)

```text
┌─────────────────────────────────────────────────────────┐
│  RGB contracts (off-chain consignments + client verify) │
│  seal UTXO + 32-byte commitment (tapret / opret)        │
└───────────────────────────┬─────────────────────────────┘
                            │ anchors on
┌───────────────────────────▼─────────────────────────────┐
│  Liquid / Bitcoin chain layer                            │
│  L-BTC fees · optional native assets (backing) · HTLCs   │
│  later: Simplicity covenants on seal UTXOs               │
└─────────────────────────────────────────────────────────┘
```

| System | Tooling | Product claim |
|--------|---------|----------------|
| **RGB** | `lab-rgb`, patched `rgb-consensus`, CLI `rgb` / `bfa` | “RGB on Liquid / BTC testnet” |
| **Native Liquid assets** | LWK `issue_asset` | Fees, optional **backing** for BFA/mint demos only |

---

## 3. Who should use this

| Audience | How they use it |
|----------|-----------------|
| **Protocol engineers / researchers** | Headless crates + regtest demos ([HEADLESS.md](./HEADLESS.md)) |
| **Operators / demo hosts** | CLI + `rgbmvp serve` + browser console |
| **Agents / AI assistants** | Repo files as truth + optional project memory ([M2M.md](./M2M.md)) |

---

## 4. How to use it (human path)

### 4.1 Prerequisites

- Rust **1.88+** (CI uses `stable`), Python **3.11+** (optional for glue/tests)
- No Docker required for P0/P1/P3 public testnet demos  
- Docker **optional** for P2 Simplicity regtest (`./scripts/regtest_simplicity.sh up`)
- Redis **optional** — only for agent project-memory cache (you can ignore it)

### 4.2 Install and build

```bash
git clone https://github.com/EdwinKestler/rgbmvp.git
cd rgbmvp
cargo build -p lab-cli
# binary: ./target/debug/rgbmvp
```

Optional Python:

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
pytest -q
```

Config template (never commit real secrets):

```bash
cp .env.example .env
# BTC_TESTNET_WIF only for P1/P3 swap BTC leg — local .env only
```

### 4.3 Lab wallets (Liquid)

```bash
./target/debug/rgbmvp wallet bootstrap-testnet   # alice/bob/carol/maker
./target/debug/rgbmvp wallet address --name alice
# Fund: https://liquidtestnet.com/faucet
./target/debug/rgbmvp wallet balance --name alice
```

Fixture mnemonics are **public testnet-only** ([fixtures/testnet_wallets.json](../fixtures/testnet_wallets.json)). Never use them on mainnet.

Bitcoin leg for swaps: import WIF into **`btc-alice`** via `.env` + `rgbmvp btc import-env` (see [TESTNET_WALLETS.md](./TESTNET_WALLETS.md)).

### 4.4 CLI: RGB on Liquid (P0)

```bash
./target/debug/rgbmvp net status          # rgb_ready: true
./target/debug/rgbmvp rgb issue --wallet alice --ticker tRGB --name "Test RGB" --supply 1000000
./target/debug/rgbmvp rgb transfer --wallet alice --contract 'rgb:…' --amount 600000 --broadcast
./target/debug/rgbmvp rgb verify --plan tRGB-… --txid <anchor_txid>
# expect status: valid
```

### 4.5 CLI: atomic swap (P1)

```bash
./target/debug/rgbmvp btc import-env
./target/debug/rgbmvp swap init --id demo --csv-delay 6 --alice-btc btc-alice --bob-lq bob
./target/debug/rgbmvp swap fund-btc --id demo
./target/debug/rgbmvp swap fund-lq --id demo
./target/debug/rgbmvp swap claim-lq --id demo   # reveals preimage on Liquid
./target/debug/rgbmvp swap claim-btc --id demo
# refund-btc / refund-lq after CSV if not claimed
```

### 4.6 Browser lab console (P3)

```bash
./target/debug/rgbmvp serve --bind 127.0.0.1:8080
```

| URL | Purpose |
|-----|---------|
| http://127.0.0.1:8080/ | Issue · Transfer · Verify · guided Swap |
| http://127.0.0.1:8080/demo | Read-only board (balances, activity, phases) |
| http://127.0.0.1:8080/audit | BFA audit (upload history JSON) |
| http://127.0.0.1:8080/v1 | Machine API catalog |

**Security model:** keys stay on **labd** / CLI. The browser never receives seeds or swap preimages.

**Swap tab:** use wallet **names** (`btc-alice`, `bob`), not payment addresses.

### 4.7 Protocol kit without UI (P2)

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c0_simplicity.sh    # Simplicity preimage ∧ opret
./scripts/demo_c1_mint_gate.sh     # mint-gate vault + recursion
./scripts/demo_c3_bfa_audit.sh     # BFA full-history audit
```

See [HEADLESS.md](./HEADLESS.md).

---

## 5. Repository layout (human map)

```text
rgbmvp/
  README.md              # quickstart + status
  AGENTS.md              # short agent rules (points to M2M)
  docs/                  # architecture, scenarios, phase closures
  crates/                # Rust: lab-cli, lab-rgb, lab-chain, lab-btc, lab-simplicity, lab-api
  vendor/                # rgb-consensus WitnessTx patch
  web/                   # static lab console
  fixtures/              # public testnet roles (no real secrets)
  scripts/               # bootstrap, regtest, demos, project_memory.py
  src/rgbmvp/            # Python package (health, config, memory glue)
  .rgbmvp/               # LOCAL only (gitignored): wallets, swaps, plans
```

---

## 6. Documentation index

| Document | Audience | Content |
|----------|----------|---------|
| [README.md](../README.md) | Everyone | Quickstart |
| [MANIFESTO.md](./MANIFESTO.md) | Bitcoin / Liquid community | Vision, innovation, use cases |
| **This file** | Humans | Purpose + how to use |
| [M2M.md](./M2M.md) | Agents / AI | Goals, workflows, project memory protocol |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | Both | Layers, API, privacy |
| [SCENARIOS.md](./SCENARIOS.md) | Both | Scenario ladder IDs |
| [STACK.md](./STACK.md) | Both | LWK / RGB / toolchain |
| [P1_CLOSED.md](./P1_CLOSED.md) · [P2_CLOSED.md](./P2_CLOSED.md) · [P3_CLOSED.md](./P3_CLOSED.md) | Both | What “done” means + evidence |
| [HEADLESS.md](./HEADLESS.md) | Protocol users | Crates + demos without UI |
| [PROJECT_MEMORY.md](./PROJECT_MEMORY.md) | Agents (detail) | Redis discovery cache contract |
| [TESTNET_WALLETS.md](./TESTNET_WALLETS.md) | Operators | alice/bob/btc-alice roles |

---

## 7. Safety checklist

- Never commit `.env`, `.rgbmvp/`, real WIF, or private consignments.  
- Fixture BIP39 phrases are **testnet-only** and public by design.  
- Prefer Liquid Testnet + Bitcoin testnet; mainnet only with explicit flags after review.  
- Do not treat Redis project memory as a product database (optional agent cache only).

---

## 8. Getting help

- Phase evidence and live txids: closed docs under `docs/*_CLOSED.md`.  
- Network/stack questions: [STACK.md](./STACK.md), [WITNESS_TX.md](./WITNESS_TX.md).  
- Remotes: https://github.com/EdwinKestler/rgbmvp · https://github.com/ffwd-org/rgbmvp  
