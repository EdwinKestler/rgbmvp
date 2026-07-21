# Architecture — RGB Liquid Testnet Lab (`rgbmvp`)

Public, phased MVP to **exercise RGB-on-Liquid** (and related patterns from
[kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike))
on **Liquid Testnet**, with a **CLI + small web verifier** surface that is
ready to grow into a full browser UI.

## Product intent

| Goal | Approach |
|------|----------|
| Prove RGB assets can live **natively on Liquid** | Issue + transfer + client-side verify on Liquid Testnet |
| Prove **interop without bridges** | Atomic swap of twin RGB contracts (BTC testnet ↔ Liquid testnet) |
| Prove **programmable seals** | Simplicity covenants on seal UTXOs (later phases) |
| Stay public but safe | **Testnet only** for public demos; regtest for CI |
| Stay simple | CLI is the control plane; web is a thin verifier + status UI |

Repository files are always authoritative for this project. Redis project
memory (see [PROJECT_MEMORY.md](./PROJECT_MEMORY.md)) is only a discovery
cache for agents—not asset or consignment storage.

---

## Two asset systems (do not confuse them)

Liquid and RGB both move “tokens,” but they are **different layers**.

### A. Native Liquid issued assets (LWK / Elements)

Documented at [docs.liquid.net assets](https://docs.liquid.net/docs/assets) and
[Liquid Assets](https://docs.liquid.net/docs/liquid-assets):

- First-class **Elements** assets: L-BTC (policy asset) + any issued asset ID.
- Lifecycle via LWK: **issue / reissue / burn**, PSET pipeline, CT descriptors.
- Amounts and asset types are **confidential by default** (Pedersen + range proofs).
- Asset ID is on-chain; metadata may be registered in the Liquid Asset Registry.
- Tooling: **[LWK](https://github.com/Blockstream/lwk)** (`lwk_wollet` + `lwk_signer`, optional `lwk_cli` JSON-RPC server, `lwk_wasm` for browsers), `elementsd`, explorers.
- LWK is **not** RGB: it manages Liquid CT wallets, PSETs, native issued assets, Boltz, and Simplicity helpers. See [STACK.md](./STACK.md) for crate-level mapping (≈0.18.x).

Use this layer for:

- Paying **fees in L-BTC**.
- Optional **backing assets** (e.g. test USDt-like asset) for backed-mint demos.
- Wallet connectivity that future browser UIs already understand (**`lwk_wasm`**, Marina, Jade).
- Building/signing the **Liquid transactions** that carry RGB seals and commitments (PSET path).

### B. RGB client-side assets (this project’s focus)

From KaleidoSwap’s RGB-on-Liquid work:

- Contract history lives **off-chain** (consignments); holders validate.
- Chain only sees a **single-use seal** (UTXO) + **32-byte commitment** (tapret/opret).
- Contract genesis is bound to **one** `ChainNet` (Bitcoin *or* Liquid)—no teleport.
- Cross-chain effect = **atomic swap of twins**, not one shared contract ID.
- Needs patched `rgb-consensus` `WitnessTx` so Liquid txs verify like Bitcoin txs.

Use this layer for:

- Demo RGB20 / IFA / BFA contracts on Liquid Testnet.
- Consignment exchange + public verification pages.
- Swaps and Simplicity seal programs.

```text
┌──────────────────────────────────────────────────────────────┐
│  RGB contract (off-chain consignments, client validation)    │
└───────────────────────────┬──────────────────────────────────┘
                            │ seal UTXO + commitment
┌───────────────────────────▼──────────────────────────────────┐
│  Liquid UTXO / L-BTC fee / optional native issued assets     │
│  LWK · Elements · CT · (later) Simplicity covenants          │
└──────────────────────────────────────────────────────────────┘
```

**MVP rule:** native Liquid assets are **infrastructure and backing**.
Product claims about “RGB on Liquid” always mean **RGB contracts anchored on Liquid**, not “we issued a Liquid registry asset and called it RGB.”

---

## Lightning / Core Lightning placement

[Core Lightning (CLN)](https://github.com/ElementsProject/lightning) is the
spec-compliant Lightning implementation for **Bitcoin** (and related Bitcoin
networks). Relevant facts for this lab:

- CLN talks to **bitcoind**, not `elementsd`.
- Invoice / pay / channel lifecycle is BOLT-based (BOLT11, etc.).
- LWK documents **Boltz** integration for LBTC↔BTC / Lightning bridges—not RGB itself.
- KaleidoSwap’s long-term story includes RGB on Bitcoin **and** Lightning; the
  spike’s public interop story for Liquid is **on-chain HTLC / RGB-wrapped claim**,
  not “CLN channels on Liquid.”

**Phased policy for `rgbmvp`:**

| Phase | Lightning role |
|-------|----------------|
| P0 | **None required.** Liquid Testnet + RGB only. |
| P1 | Optional: CLN on **Bitcoin testnet/signet** only if we want LN-adjacent UX later; default interop is **on-chain HTLC atomic swap** BTC↔Liquid RGB. |
| P2+ | Research only: RGB-over-Lightning is out of scope for the public Liquid lab until P0/P1 are solid. |

Do not block the MVP on running CLN.

---

## Delivery shape: CLI + small web (browser-ready)

### Principles

1. **All consensus and RGB logic lives behind a stable machine API** (JSON in/out).
2. **CLI** is a thin client of that API (and can call libraries in-process for local mode).
3. **Web verifier** is a thin client of the same API (no duplicate validation rules).
4. Future **browser wallet UI** reuses the same API + (optional) LWK WASM for L-BTC/PSET only;
   RGB consignments still go through the lab API until a full RGB browser stack exists.

### Components

```text
                    ┌─────────────────────┐
   Humans / agents  │  rgbmvp CLI         │
                    └──────────┬──────────┘
                               │ same JSON contract
                    ┌──────────▼──────────┐
   Public browsers  │  Web verifier (P0)  │  + later full UI
                    │  static + fetch API │
                    └──────────┬──────────┘
                               │ HTTP JSON
                    ┌──────────▼──────────┐
                    │  labd (API service) │
                    │  validate · faucet  │
                    │  consignment store  │
                    │  scenario runners   │
                    └─────┬─────────┬─────┘
                          │         │
              ┌───────────▼──┐   ┌──▼────────────────┐
              │ RGB core     │   │ Chain adapters      │
              │ (patched     │   │ LWK / elements RPC  │
              │  consensus,  │   │ Esplora testnet     │
              │  ops, build) │   │ BTC testnet (P1)    │
              └──────────────┘   └─────────────────────┘
```

### Suggested repository layout (target)

```text
rgbmvp/
  docs/                 # this ladder + memory docs
  scripts/              # project_memory, smoke, regtest helpers
  crates/               # Rust workspace (preferred for RGB + LWK)
    lab-core/           # issue, transfer, verify, types
    lab-chain/          # Liquid + Bitcoin adapters, WitnessTx
    lab-api/            # labd HTTP service
    lab-cli/            # rgbmvp binary
  web/                  # small verifier (static or minimal SSR)
  vendor/               # optional rgb-consensus patch (from spike)
  tests/                # python unit tests for non-Rust glue; e2e later
```

Python remains for project-memory and light tooling; **RGB/Liquid paths should be Rust** (aligned with the spike and LWK).

---

## Networks

| Network | Role |
|---------|------|
| **Liquid Testnet** | Public P0/P1 demos; faucet L-BTC; explorer links |
| **Bitcoin Testnet / Signet** | P1 swap leg only |
| **Elements + bitcoind regtest** | CI, Simplicity demos, offline reproduction of spike scripts |
| **Liquid / Bitcoin mainnet** | Explicitly out of scope until upstream patch + security review |

Liquid Testnet has **no peg** to Bitcoin; L-BTC comes from faucets
([liquidtestnet.com](https://liquidtestnet.com/faucet)). That is ideal for a public tech test.

LWK quickstart path: signer → wollet → Electrum/Esplora sync → PSET → sign → broadcast
([Get Started](https://docs.liquid.net/docs/get-started),
[Quickstart](https://docs.liquid.net/docs/quickstart)).

---

## API contract (browser-ready from day one)

Stable, versioned JSON. Web and CLI share it. Breaking changes bump `/v1` → `/v2`.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/health` | Liveness + configured networks |
| GET | `/v1/networks` | liquid-testnet / btc-testnet / regtest flags |
| POST | `/v1/rgb/issue` | Issue demo RGB20 (testnet keys / faucet policy) |
| POST | `/v1/rgb/invoice` | Build receive intent (seal + invoice payload) |
| POST | `/v1/rgb/transfer` | Build/broadcast transfer + return consignment ref |
| POST | `/v1/rgb/consignments` | Upload consignment (ephemeral store) |
| GET | `/v1/rgb/consignments/{id}` | Fetch consignment metadata + blob URL |
| POST | `/v1/rgb/verify` | Validate consignment against chain witness |
| GET | `/v1/proofs/{id}` | Public proof page data (contract, txids, status) |
| POST | `/v1/swap/*` | P1 atomic-swap coordinator endpoints |
| POST | `/v1/covenant/*` | P2 Simplicity demo endpoints |

### Verify request/response (illustrative)

```json
// POST /v1/rgb/verify
{
  "network": "liquid-testnet",
  "consignment": "<hex or multipart>",
  "txid": "optional override if not embedded"
}

// 200
{
  "status": "valid" | "invalid" | "pending_witness",
  "contract_id": "rgb:...",
  "chain_net": "liquid-testnet",
  "anchor_txid": "...",
  "explorer_url": "https://blockstream.info/liquidtestnet/tx/...",
  "checks": [
    {"name": "schema", "ok": true},
    {"name": "seal_closure", "ok": true},
    {"name": "anchor_verify", "ok": true}
  ],
  "errors": []
}
```

Web verifier: paste consignment or `proof id` → show checks + explorer link.
No private keys in the browser for P0 verifier mode.

---

## Data and privacy

| Data | Storage | Notes |
|------|---------|-------|
| User seeds / signers | Local CLI only (or user wallet) | Never in labd logs |
| Faucet / demo issuer keys | Server secret (testnet) | Rotate; rate-limit |
| Consignments | Ephemeral object store + TTL | Not a permanent ledger |
| Blinding factors (Liquid CT) | Stay with wallet that owns outputs | CT ≠ RGB privacy |
| Redis | Optional job cache / rate limits | Never FLUSH; never asset truth |

Confidential Transactions hide **Liquid** amounts/assets; RGB hides **contract history**.
They compose: seal UTXO can be confidential while RGB still verifies commitment in scriptPubKey
(spike’s load-bearing result).

---

## Security boundaries (public testnet)

- Rate-limit faucet and issue endpoints.
- No mainnet endpoints compiled into public builds without a feature flag.
- Consignment size limits; virus-scan not required but size/DoS caps are.
- Proof pages are public; do not embed seeds or master blinding keys.
- Document that demo assets have **no economic value**.

---

## Success criteria (whole ladder)

A stranger can:

1. Fund a Liquid Testnet wallet with faucet L-BTC.
2. Obtain a demo RGB asset on Liquid Testnet (faucet or self-issue).
3. Transfer to a second party; receiver **verifies** via CLI or web.
4. Open the anchor tx on a public explorer.
5. (P1) Complete a BTC↔Liquid RGB atomic swap with a demo maker or second user.
6. (P2) Run at least one Simplicity seal scenario and one backed-mint/audit scenario on regtest or testnet.
7. CI replays P0 (and critical P1) on regtest using spike-compatible docker.

---

## References

- [KaleidoSwap thread](https://x.com/i/status/2077733143428190555) / [rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike)
- [Blockstream/lwk](https://github.com/Blockstream/lwk) / [LWK book](https://blockstream.github.io/lwk/book)
- [Liquid Get Started (LWK)](https://docs.liquid.net/docs/get-started)
- [LWK Features](https://docs.liquid.net/docs/features)
- [Liquid Assets / Issuance](https://docs.liquid.net/docs/assets)
- [Confidential Transactions](https://docs.liquid.net/docs/confidential-transactions)
- [Core Lightning](https://github.com/ElementsProject/lightning)
- [SCENARIOS.md](./SCENARIOS.md) — phased ladder
- [STACK.md](./STACK.md) — toolchain choices (includes full LWK crate map)
