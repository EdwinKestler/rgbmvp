# P2 R0 — Simplicity toolchain pins & ADR

**Status:** R0 complete (pins frozen; regtest path live)  
**Date:** 2026-07-21  
**Plan:** [P2_PLAN.md](./P2_PLAN.md) · scenarios C0+ in [SCENARIOS.md](./SCENARIOS.md)

This document freezes the **toolchain**, **network**, **program source**, and
**architecture decisions** for Phase P2 before C0 implementation.

---

## Quick start (regtest Simplicity node)

**No sudo** if your user is already in the `docker` group (this host is).

```bash
# From repo root
./scripts/regtest_simplicity.sh up
./scripts/regtest_simplicity.sh status
./scripts/regtest_simplicity.sh cli getblockchaininfo

# Stop (keep chain volume) / wipe all volumes
./scripts/regtest_simplicity.sh down
./scripts/regtest_simplicity.sh wipe
```

| Item | Value |
|------|--------|
| Image | `ghcr.io/vulpemventures/elements:23.3.0` |
| Container | `rgbmvp-elementsd-simplicity` |
| RPC host port | **7042** |
| User / pass | `user` / `pass` (regtest only) |
| Chain | `elementsregtest` |
| Simplicity activation | `evbparams=simplicity:-1:::` (genesis) |
| Compose file | [`docker-compose.yml`](../docker-compose.yml) |
| Conf | [`docker/elements-simplicity.conf`](../docker/elements-simplicity.conf) |

Optional profiles:

```bash
# Plain RGB regtest Elements 23.2.4 on :7041
docker compose --profile rgb-regtest up -d elementsd

# Bitcoin regtest on host :18543
docker compose --profile btc-regtest up -d bitcoind
```

---

## Toolchain pins (frozen for C0)

| Component | Pin | Notes |
|-----------|-----|--------|
| **Elements** | `23.3.0` (`ghcr.io/vulpemventures/elements:23.3.0`) | Simplicity deployment support |
| **SimplicityHL** | **`simplicityhl` 0.6.x** (crates.io `0.6.0` verified) | Compile `.simf` → bytecode |
| **rust-simplicity** | **`simplicity-lang` 0.8.x** (`0.8.0` verified) | Runtime / env / jets |
| **Tapleaf version** | **`0xbe`** | Elements Simplicity leaf |
| **NUMS internal key** | BIP-341 NUMS `50929b74…03ac0` | Key-path disabled |
| **C0 program** | [`programs/simplicity/rgb_anchor_covenant.simf`](../programs/simplicity/rgb_anchor_covenant.simf) | Path A source |
| **LWK** (wallet/PSET) | keep **0.18.x** for lab-chain | Orthogonal to covenant driver |
| **`lwk_simplicity`** | **0.18.0** (depends on **simplicityhl 0.5.0**) | Path B — **not** C0 default |

### Why these versions

Spike-faithful C0 ([kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike)) uses:

- `simplicityhl = "0.6"`
- `simplicity-lang = "0.8"`
- Elements **23.3.0** + `evbparams=simplicity:-1:::`

LWK 0.18’s `lwk_simplicity` still tracks **simplicityhl 0.5.0**. Mixing 0.5 and 0.6 in one binary is undesirable for C0. See ADR-002.

---

## Program inventory (C0)

**File:** `programs/simplicity/rgb_anchor_covenant.simf`

| Piece | Role |
|-------|------|
| `param::EXPECTED_HASH` | Compile-time hash; baked into CMR → address |
| `witness::PREIMAGE` | 32-byte preimage; `SHA256` must equal expected hash |
| `witness::ANCHOR_PAYLOAD` | 32-byte RGB MPC root (free content) |
| `jet::output_script_hash(0)` | Introspection: hash of output 0 scriptPubKey |
| Expected SPK shape | `OP_RETURN OP_PUSHBYTES_32 <payload>` = `0x6a20 \|\| payload` |

**Pass criteria (C0):** compliant spend accepted; spend with preimage but **without** opret-shaped vout0 rejected by **consensus**.

C1–C4 programs (mint-gate, burn, staking) are **not** vendored until those scenarios start — source remains the spike until ported.

---

## ADR log

### ADR-001 — Network: regtest-first

| | |
|--|--|
| **Status** | Accepted |
| **Context** | Public Liquid Testnet may not expose the same Simplicity deployment; spike proved C0 on Elements regtest. |
| **Decision** | All C0 consensus tests run against **local** `elementsd-simplicity` (port 7042). Public testnet is optional after a feature probe. |
| **Consequences** | Need Docker on CI/dev machines; no dependency on faucet for C0 unit of work. |

### ADR-002 — Path A vs Path B (program driver)

| | |
|--|--|
| **Status** | Accepted for C0 |
| **Context** | Path A = spike-style `simplicityhl` 0.6 driver. Path B = `lwk_simplicity` 0.18 (simplicityhl 0.5). |
| **Decision** | **Path A for C0.** Implement `crates/lab-simplicity` against **simplicityhl 0.6** + **simplicity-lang 0.8**, driving programs under `programs/simplicity/`. Revisit Path B after C0 green (wrap LWK helpers or bump when LWK tracks 0.6). |
| **Consequences** | Temporary dual-stack awareness; do not force `lwk_simplicity` into the C0 binary. LWK remains for ordinary wallet/PSET if useful later. |

### ADR-003 — Anchor shape: opret for C0

| | |
|--|--|
| **Status** | Accepted |
| **Context** | Lab P0 transfers used **tapret** / explicit P2TR; spike C0 enforces **opret** shape via `output_script_hash`. |
| **Decision** | C0 covenant enforces **opret-shaped** output 0 (`0x6a20 \|\| 32B`). Tapret jet support is a later enhancement, not MVP. |
| **Consequences** | C0.5 linking real RGB plans must produce **opret** commitments (or a dedicated opret path in `lab-rgb`), not only tapret. |

### ADR-004 — Compose topology

| | |
|--|--|
| **Status** | Accepted |
| **Context** | Spike runs three nodes; lab only needs Simplicity for P2 R0. |
| **Decision** | Default compose service = **only** `elementsd-simplicity`. Optional profiles `rgb-regtest` (23.2.4) and `btc-regtest` (bitcoind). Container names prefixed `rgbmvp-` to avoid clashing with spike. |
| **Consequences** | `./scripts/regtest_simplicity.sh up` is the single entry for P2 developers. |

### ADR-005 — Secrets & RPC credentials

| | |
|--|--|
| **Status** | Accepted |
| **Context** | Regtest RPC uses well-known `user`/`pass` matching the spike. |
| **Decision** | Keep fixed regtest credentials in compose conf (not production). Document in `.env.example`. Never reuse on mainnet/testnet public nodes. |
| **Consequences** | Safe for local demos; do not expose port 7042 beyond localhost without a firewall. |

---

## Environment variables (`.env.example`)

```bash
# P2 regtest Elements (Simplicity) — local only
ELEMENTS_SIMPLICITY_RPC_URL=http://127.0.0.1:7042
ELEMENTS_RPC_USER=user
ELEMENTS_RPC_PASSWORD=pass
# Optional non-Simplicity Elements regtest
# ELEMENTS_RPC_URL=http://127.0.0.1:7041
```

---

## C0 implementation checklist

1. ~~Scaffold `crates/lab-simplicity` (`simplicityhl` 0.6, `simplicity-lang` 0.8).~~  
2. ~~`lab-simp` address/spend + `rgbmvp covenant address|demo`.~~  
3. ~~Integration: fund leaf 0xbe → A/B/C proofs — [C0_CLOSED.md](./C0_CLOSED.md).~~  
4. Optional later: `/v1/covenant/*` + demo board.

**C0 closed 2026-07-21.** C1 mint-gate is next.

---

## Sudo / auth notes

| Action | Sudo? |
|--------|--------|
| `docker compose up` (user in `docker` group) | **No** |
| Install Docker Engine / add user to `docker` group | **Yes, once:** `sudo usermod -aG docker $USER` then re-login |
| `apt install` build tools | Only if missing |
| Pull `ghcr.io/vulpemventures/elements:23.3.0` | No (public image) |

---

## Smoke evidence (R0)

**Host smoke (2026-07-21):** `./scripts/regtest_simplicity.sh up` succeeded (no sudo).

| Check | Result |
|-------|--------|
| Image pull | `ghcr.io/vulpemventures/elements:23.3.0` |
| Container | `rgbmvp-elementsd-simplicity` **healthy** |
| Port | `0.0.0.0:7042->7042/tcp` |
| `getblockchaininfo.chain` | `elementsregtest` |
| Blocks | `0` at first boot (expected) |
| Conf | `evbparams=simplicity:-1:::` mounted read-only |

```bash
./scripts/regtest_simplicity.sh up
./scripts/regtest_simplicity.sh status
./scripts/regtest_simplicity.sh cli getblockchaininfo
```

Note: `getblockchaininfo` does not print the string `simplicity`; activation is via conf `evbparams`. C0 positive/negative spends prove the deployment at the consensus layer.

---

## References

- [P2_PLAN.md](./P2_PLAN.md)  
- [STACK.md](./STACK.md) (`lwk_simplicity` note)  
- [BlockstreamResearch/SimplicityHL](https://github.com/BlockstreamResearch/SimplicityHL)  
- [BlockstreamResearch/rust-simplicity](https://github.com/BlockstreamResearch/rust-simplicity)  
- [kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike) (`spike-simplicity`, `docker/elements-simplicity.conf`)  
- [Blockstream/lwk](https://github.com/Blockstream/lwk) `lwk_simplicity` 0.18  
