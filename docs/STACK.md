# Toolchain stack

## Why this stack

| Concern | Choice | Rationale |
|---------|--------|-----------|
| RGB consensus + builders | Rust (`rgb-consensus` patched, `rgb-ops`, `rgb-schemas`) | Matches production RGB stack and kaleidoswap spike |
| Liquid wallet / PSET / CT | **[LWK](https://github.com/Blockstream/lwk)** (Rust workspace ≈0.18) | Official Liquid Wallet Kit; Electrum/Esplora; issue native assets; WASM/CLI path for browser later |
| Chain data | Esplora / Electrum (testnet), optional `elementsd` | LWK clients; no mandatory full node for light mode |
| Public API | `labd` HTTP JSON `/v1` | CLI and web share one contract (RGB-oriented; may sit beside `lwk_cli` server) |
| CLI | `rgbmvp` binary | Lab scenarios; can shell out to or embed LWK |
| Web verifier | Static + `/v1` | Small surface; P3 can load `lwk_wasm` for L-BTC only |
| Project memory | `scripts/project_memory.py` + local Redis | Agent discovery only |
| Lightning | **Not required for P0/P1 core** | CLN = Bitcoin LN; LWK uses **Boltz** for LBTC/LN bridges—orthogonal to RGB-on-Liquid |

---

## LWK deep dive ([Blockstream/lwk](https://github.com/Blockstream/lwk))

LWK is a **modular Rust workspace** for Liquid wallets—not an RGB stack. Pin crates around **0.18.x** (current master). Docs book: [blockstream.github.io/lwk/book](https://blockstream.github.io/lwk/book).

### Crates that matter to `rgbmvp`

| Crate | Role for this lab |
|-------|-------------------|
| **`lwk_wollet`** | Watch-only wallet on **CT descriptors**; balances, addresses, scan, **TxBuilder** / PSET, issue/reissue/burn native assets, Electrum/Esplora clients |
| **`lwk_signer`** | Software (+ Jade/Ledger) signers for PSETs; BIP39; AMP0 feature |
| **`lwk_common`** | Shared types, optional sqlite, QR helpers |
| **`lwk_app` + `lwk_cli` + `lwk_rpc_model` + `lwk_tiny_jrpc`** | Local **JSON-RPC server** (`lwk_cli server start`, default ~`:32111`) + CLI client—useful Phase 0 prototype without writing our own wallet RPC |
| **`lwk_wasm`** | Browser/WASM (npm `lwk_wasm`); reference UI [liquidwebwallet.org](https://liquidwebwallet.org/) — path for **P3** L-BTC watch/sign, not RGB validation |
| **`lwk_bindings`** | UniFFI → Python / Kotlin / Swift / C# / Android; optional `simplicity` + `lightning` features |
| **`lwk_boltz`** | Boltz swap client (LBTC ↔ BTC/LN); uses `boltz-client` + light LN crates—**not** RGB twin swaps |
| **`lwk_simplicity`** | SimplicityHL integration; optional **lending** feature (Blockstream Research lending indexer)—relevant **P2** for covenants, parallel to spike’s Simplicity demos |
| **`lwk_jade` / `lwk_ledger` / `lwk_hwi`** | Hardware signers (later / optional) |
| **`lwk_payment_instructions`** | Payment instruction helpers |
| **`lwk_containers` / `lwk_test_util`** | Test/regtest helpers |

LWK depends on **`elements`** / **`elements-miniscript`** (workspace pins elements ~0.25–0.26). That is the same family of types needed for a Liquid `WitnessTx` adapter.

### Architectural pattern inside LWK (copy for browser-readiness)

1. **Signer** holds keys; **wollet** is watch-only (CT descriptor + chain scan).
2. Transactions are **PSET**-centric (collaborative multi-party friendly).
3. **`lwk_cli` is a thin client** of a local JSON-RPC app server—same split we want for `rgbmvp` CLI vs `labd`.
4. Multi-language surface via bindings/WASM so a future browser UI can talk Liquid without rewriting wallet math.

### Typical LWK flow (Phase 0 / fees / native backing)

```text
Mnemonic → SwSigner → wpkh_slip77_descriptor
        → Wollet + Electrum/Esplora client → sync
        → address → faucet L-BTC
        → TxBuilder (send / issue_asset) → PSET
        → signer.sign → finalize → broadcast
```

CLI equivalent (when `lwk_cli` server is running):

```bash
lwk_cli server start
lwk_cli signer generate
lwk_cli wallet load --wallet lab -d "<ct(...) descriptor>"
lwk_cli wallet address --wallet lab
lwk_cli wallet balance --wallet lab
# issue_asset / send via wallet RPC methods (see lwk_rpc_model / docs)
```

### What LWK does **not** do

| Missing for RGB-on-Liquid | Where we get it |
|---------------------------|-----------------|
| RGB contract issue / transfer / consignment | `rgb-ops` / `rgb-schemas` + lab-core (spike patterns) |
| RGB client-side validation | patched `rgb-consensus` (`WitnessTx` + Liquid adapter) |
| RGB cross-chain twin atomic swap | lab HTLC + consignment exchange (P1) |
| RGB seal as Simplicity covenant target | Combine **lwk_simplicity** or spike `spike-simplicity` with RGB seal UTXOs (P2) |

**Never** use LWK `issue_asset` and call the result “RGB.” That produces a **native Liquid asset ID**, not an `rgb:` contract.

### How `rgbmvp` should consume LWK

| Phase | Integration style |
|-------|-------------------|
| **0** | Depend on `lwk_wollet` + `lwk_signer` **as crates** *or* drive `lwk_cli` RPC for fastest wallet smoke; fund testnet L-BTC |
| **P0** | LWK builds/broadcasts Liquid txs that **spend seals** and carry **tapret/opret** commitment outputs; RGB logic remains in lab-core |
| **P1** | LWK for Liquid leg UTXOs; separate Bitcoin stack for BTC leg; do not use Boltz as the RGB swap |
| **P2** | Evaluate `lwk_simplicity` vs vendored spike SimplicityHL programs for seal covenants |
| **P3** | `lwk_wasm` for browser L-BTC/descriptor UX; RGB verify still via `/v1` |

### Optional: run LWK beside labd

```text
lwk_cli server  (:32111 JSON-RPC)  →  Liquid wallet operations
labd            (:8080  /v1)       →  RGB issue/transfer/verify + proofs
rgbmvp CLI      talks to both as needed
web verifier    talks only to labd /v1 (no keys)
```

For a minimal public demo, labd may **embed** LWK libraries and hide the second RPC; keep the logical separation either way.

### Simplicity + lending note

`lwk_simplicity` integrates **SimplicityHL** and optional **simplicity-lending** contracts. That is Blockstream’s production direction for programmable Liquid.

**P2 R0 decision (see [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) ADR-002):**

| Path | Pin | Use |
|------|-----|-----|
| **A (C0 default)** | `simplicityhl` **0.6** + `simplicity-lang` **0.8** | Spike-faithful RGB anchor covenants in `lab-simplicity` |
| **B (later)** | `lwk_simplicity` **0.18** → simplicityhl **0.5** | Packaging / lending helpers after C0 green |

Regtest node: Elements **23.3.0**, `evbparams=simplicity:-1:::`, RPC **:7042** (`./scripts/regtest_simplicity.sh up`).

### Boltz note

`lwk_boltz` bridges **LBTC ↔ BTC/Lightning** via Boltz. Useful if we ever want “pay LN invoice with L-BTC,” **not** for proving RGB twin swaps. Keep out of P0 success criteria.

---

## Native Liquid asset vs RGB (operational)

| Task | Tool |
|------|------|
| Get L-BTC for fees | Faucet + LWK receive |
| Issue **Liquid** test asset (backing) | LWK `issue_asset` + optional registry metadata |
| Issue **RGB20** on Liquid | RGB stack + Liquid witness tx (often built/signed via LWK PSET) |
| Hide amounts on Liquid | CT (default in LWK); RGB commitment stays in scriptPubKey |
| Swap two **Liquid native** assets | LiquiDEX / PSET (comparison only) |
| Swap **RGB** across chains | HTLC + consignments (P1) |
| LBTC ↔ LN | LWK Boltz (optional, non-goal) |

## Spike reuse

From [rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike):

- `WitnessTx` patch and Liquid `impl`
- `rgb20-transfer` / confidential / HTLC / simplicity demos as **regtest oracles**
- Prefer LWK for wallet/PSET/CT instead of ad-hoc elements RPC where possible
- Absorb RGB anchor builders into `lab-core`; do not require users to run the full spike tree

## Dependency policy

- Pin LWK crates (e.g. `0.18.1`) from crates.io when possible; git pin only if we need unreleased simplicity features.
- Pin/vendored RGB consensus patch separately; document in `vendor/`.
- Align `elements` versions carefully (LWK already juggles 0.25 vs 0.26)—treat as a Phase 0 risk.
- No mainnet private keys in repo.
- Optional Redis: disposable, namespaced; never asset authoritative.

## Human wallet: SideSwap AppImage

Local desktop wallet for **native Liquid** (not RGB):

| | |
|--|--|
| Binary | `SideSwap.AppImage` (repo root, **gitignored**) |
| Source | [sideswap-io/sideswapclient](https://github.com/sideswap-io/sideswapclient) |
| Use | Testnet L-BTC receive, send, Liquid P2P swaps, AMP |
| Testnet | Settings → environment → **Liquid Testnet** ([FAQ](https://sideswap.io/faq/testnet/)) |

```bash
chmod +x SideSwap.AppImage
./SideSwap.AppImage
```

Full notes: [WALLETS.md](./WALLETS.md).

## Local dev checklist

```bash
# Python glue (already)
python3 -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"

# Optional human wallet (native Liquid testnet)
# chmod +x SideSwap.AppImage && ./SideSwap.AppImage
# → Settings → Liquid Testnet → receive address → liquidtestnet.com/faucet

# Optional: stock LWK CLI for wallet smoke (Phase 0)
# cargo install --git https://github.com/Blockstream/lwk --locked lwk_cli
# lwk_cli server start
# lwk_cli signer generate

# Later: this repo’s Rust workspace
# rustup update stable
# cargo build -p lab-cli

# Liquid Testnet L-BTC
# https://liquidtestnet.com/faucet
# Explorer: https://blockstream.info/liquidtestnet/

# Optional project memory
python scripts/project_memory.py index
```

## References

- Source: [github.com/Blockstream/lwk](https://github.com/Blockstream/lwk)
- Book: [blockstream.github.io/lwk/book](https://blockstream.github.io/lwk/book)
- crates.io: `lwk_wollet`, `lwk_signer`, `lwk_cli`, …
- Liquid docs: [Get Started](https://docs.liquid.net/docs/get-started), [Assets](https://docs.liquid.net/docs/assets)
- WASM demo: [liquidwebwallet.org](https://liquidwebwallet.org/)
- SideSwap: [sideswapclient](https://github.com/sideswap-io/sideswapclient) · [downloads](https://sideswap.io/downloads/)
