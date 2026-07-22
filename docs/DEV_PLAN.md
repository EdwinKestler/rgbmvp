# Development plan — step by step

Living plan for `rgbmvp` (RGB Liquid Testnet Lab).  
Product shape: **CLI + small web verifier**, browser-ready `/v1`, full ladder in phases.  
**No Docker files in-repo for now** (existing host Redis Docker is optional infra only).

---

## Assessment (current machine / repo)

| Area | Status | Notes |
|------|--------|--------|
| Product docs | **Done** | ARCHITECTURE, SCENARIOS, STACK, WALLETS, PROJECT_MEMORY |
| Python env | **Done** | `.venv`, editable install, 12 tests green |
| Secrets layout | **Done** | `.env` (gitignored, mode 600), `.env.example` |
| Local state dirs | **Done** | `.rgbmvp/{wallets,consignments,tmp}` |
| SideSwap AppImage | **Present** | gitignored, executable; FUSE2 optional (see below) |
| Rust toolchain | **Ready** | rustc/cargo 1.96.1 (user install, no sudo) |
| Build packages | **Mostly ready** | `build-essential`, `libssl-dev`, `clang`, `cmake`, `jq`, `protobuf-compiler`, … |
| Redis (project memory) | **Ready** | `localhost:6379` already up (shared Docker) |
| Git | **Init only** | no commits yet; `.env` / AppImage ignored |
| Rust workspace / LWK CLI | **Done** | net + wallet + utxos |
| RGB / WitnessTx | **P0 done** | vendored patch; issue/transfer/broadcast/verify on Liquid Testnet |
| labd / web verifier | **Done** | `rgbmvp serve` + `web/index.html` |
| Docker Compose for elements | **Deferred** | F2 postponed |
| SideSwap GUI | **Optional** | CLI + faucet path works without it |

### Phase 0 live proof (2026-07-21)

| Check | Result |
|-------|--------|
| Esplora tip | ok |
| Electrum TLS | ok |
| Wallet funded | 100k L-BTC sats (+ faucet side asset) |

### P0 live proof (2026-07-21)

| Check | Result |
|-------|--------|
| Issue NIA | `rgb:JBZ2QrMz-C9WTlyJ-hCUgJkq-yVR5stP-3v_yJmu-Vlci4Rk` (`tRGB`) |
| Broadcast anchor | [2b1b2f04…2635](https://blockstream.info/liquidtestnet/tx/2b1b2f045ab9797ff34dd919293fb5b67e4e123d5557d99fc90fd06cb36c2635) |
| Verify | seal_closure + tapret_dbc + anchor_verify = **valid** |
| `rgb_ready` | true |

### P1 status (2026-07-21)

| Item | Status |
|------|--------|
| Plan | [`docs/P1_SWAP_PLAN.md`](./P1_SWAP_PLAN.md) |
| BTC address fixture | `tb1q85aad…` in `fixtures/testnet_btc.json` |
| BTC WIF | local `.env` only (`BTC_TESTNET_WIF`) |
| Esplora funding check | **~189_026 sats** funded |
| Slice A `lab-btc` | **Done** |
| RGB on Bitcoin testnet | **Done live** |
| HTLC fund + claim | **Done live** (`p1-live` → phase `done`) |
| Refund path | **CLI done** (`refund-btc` / `refund-lq`) |
| P1 | **CLOSED** — [`P1_CLOSED.md`](./P1_CLOSED.md) |
| Demo v0 | Read-only `/demo` board |
| Next | **P3** browser console ([`P3_PLAN.md`](./P3_PLAN.md); monorepo) |

### Sudo requirements

| Need | Required for | When |
|------|----------------|------|
| `libfuse2` (and/or fuse) | SideSwap AppImage **mount** mode | Optional now — extract-and-run often works without it |
| `libusb-1.0-0-dev` | Jade / USB hardware wallets via LWK | **Not** Phase 0 |
| `apt install` of build tools | Already largely installed | Only if a future crate fails to link |
| Passwordless/`sudo` for agent | Optional convenience | Separate from lab |

**Phase 0 core (LWK light client + crates) should need no sudo** if the packages below are installed.

---

## Phase map (what “done” means)

```text
[now]  Env + docs
  ↓
Phase 0   Foundations: health, LWK address/balance, patch strategy, no false RGB claims
  ↓
P0        RGB issue → transfer → verify on Liquid Testnet + web verifier
  ↓
P1        BTC ↔ Liquid RGB atomic swap (HTLC)
  ↓
P2        Simplicity covenants, backed mint, BFA audit
  ↓
P3        Full browser UI on same /v1 API
```

Detailed scenario IDs: [SCENARIOS.md](./SCENARIOS.md).

---

## Step-by-step execution order

### Step 0 — Host prep (you run; includes any sudo)

1. Install optional system packages (commands in § “Sudo / apt commands”).
2. Confirm user tools: `source .venv/bin/activate`, `pytest -q`, `cargo --version`.
3. (Optional) Launch SideSwap → Liquid Testnet → fund with faucet for human checks.
4. (Optional) First git commit of non-secret scaffold (never `.env` / AppImage).

**Exit:** machine can build Rust crates and run Python tests without elevation.

---

### Step 1 — Phase 0 skeleton (no sudo)

1. Create Cargo workspace: `crates/lab-core`, `lab-chain`, `lab-api`, `lab-cli`.
2. Pin LWK (`lwk_wollet`, `lwk_signer` ~0.18) and document `elements` version risk.
3. Implement:
   - `rgbmvp net status` → Esplora/Electrum liquid-testnet reachability (F0)
   - `rgbmvp wallet create|address|balance` via LWK (F1)
4. Wire `.env` / `RGBMVP_*` into CLI config.
5. Document **WitnessTx** vendor strategy (F3) without claiming RGB transfers work yet.
6. Defer F2 (regtest docker) until we reintroduce Compose intentionally.

**Exit:** CLI shows a Liquid Testnet address; after faucet, balance visible in CLI (and/or SideSwap).

---

### Step 2 — Phase P0 RGB core

1. Vendor or path-patch `rgb-consensus` (`WitnessTx` + Liquid `impl`).
2. `rgb issue` / `invoice` / `transfer` / `consign` / `verify` (R0–R4, R6).
3. `labd` HTTP `/v1` + static web verifier + proof page (R4–R5).
4. Confidential output path (R7).
5. Public testnet smoke runbook; re-index project memory after source lands.

**Exit:** two testnet wallets complete issue→send→verify; third party validates via web without keys.

---

### Step 3 — Phase P1 interop

1. Bitcoin testnet/signet wallet leg + twin RGB issue (S0–S1).
2. HTLC + RGB-wrapped claim (S2–S3).
3. Coordinator API + optional maker bot (S4–S5).
4. CLN remains optional docs-only (S6).

---

### Step 4 — Phase P2 programmable seals

1. Follow **[`P2_PLAN.md`](./P2_PLAN.md)** (R0 → C0 → C1/C3; C0+C3 = lab closed).
2. Simplicity anchor covenant (C0); mint-gate / BFA / staking (C1–C4) per SCENARIOS.
3. Prefer regtest CI first; public testnet if tooling allows.

---

### Step 5 — Phase P3 browser UI

1. Wallet/swap wizards on `/v1`; optional `lwk_wasm` for L-BTC only (U0–U3).

---

## What we deliberately do *not* do yet

- Mainnet endpoints or real value.
- Docker Compose for elements/bitcoind (until F2 is scheduled).
- Treating SideSwap or LWK `issue_asset` as “RGB”.
- Flushing shared Redis; storing seeds in Redis/project memory.
- Core Lightning as a P0/P1 dependency.

---

## Sudo / apt commands (run first if you want a clean host)

Copy-paste in a terminal where you can enter your password.  
Paste the full output back if anything fails.

### A. Recommended before SideSwap GUI + future USB HW (safe, small)

```bash
sudo apt-get update
sudo apt-get install -y \
  libfuse2t64 \
  fuse3 \
  libusb-1.0-0-dev
```

Notes:

- On some Ubuntu versions the package is `libfuse2` instead of `libfuse2t64`. If A fails, run B.
- If you only care about headless/lab work, you can **skip A** and use:

```bash
cd ~/rgbmvp
./SideSwap.AppImage --appimage-extract-and-run
# or: ./SideSwap.AppImage --appimage-extract && ./squashfs-root/AppRun
```

### B. Fallback if `libfuse2t64` is not found

```bash
sudo apt-get update
apt-cache search libfuse2
sudo apt-get install -y libfuse2 fuse3 libusb-1.0-0-dev
```

### C. Only if a later `cargo build` complains about missing SSL/clang (usually already OK)

```bash
sudo apt-get install -y \
  build-essential pkg-config libssl-dev clang cmake \
  libclang-dev protobuf-compiler libudev-dev
```

### D. Optional: passwordless sudo for agent automation (your choice)

```bash
echo 'kestl ALL=(ALL) NOPASSWD: ALL' | sudo tee /etc/sudoers.d/kestl-nopasswd
sudo chmod 440 /etc/sudoers.d/kestl-nopasswd
sudo visudo -cf /etc/sudoers.d/kestl-nopasswd
sudo -n true && echo 'passwordless sudo OK'
```

Not required for Phase 0 if you run rare apt commands yourself.

---

## Verification checklist (after Step 0, no sudo)

```bash
cd ~/rgbmvp
source .venv/bin/activate
pytest -q
cargo --version
python scripts/project_memory.py status   # exit 0 if Redis up
# optional:
# ./SideSwap.AppImage --appimage-extract-and-run
```

---

## Immediate next action after you return sudo/apt output

**Start Step 1 (Phase 0 skeleton)** in-tree: Cargo workspace + `rgbmvp net status` + LWK wallet address on Liquid Testnet — still without Docker.
