# Repository agent instructions

This repository is **rgbmvp** — a phased public lab for **RGB on Liquid Testnet**
(CLI + web verifier, browser-ready `/v1` API). Repository files are always the
authoritative source of truth.

## Product context (read before large changes)

1. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — layers, API, privacy
2. [docs/SCENARIOS.md](docs/SCENARIOS.md) — Phase 0 / P0 / P1 / P2 / P3 scenarios
3. [docs/STACK.md](docs/STACK.md) — LWK vs RGB vs CLN

**Do not confuse:**

- **Native Liquid issued assets** (LWK `issue_asset`) = on-chain Elements assets / fees / optional backing.
- **RGB contracts** = off-chain consignments + seal UTXO + commitment; genesis bound to one chain.

**Cross-chain** = atomic swap of twins, not moving one RGB contract id.

**Lightning / CLN:** not required for P0 or core P1; do not block work on running `lightningd`.

## Project Memory

- Before broad exploration, run `python scripts/project_memory.py status`. If the index is missing or stale (exit `2`), run `python scripts/project_memory.py index`.
- Use two or three focused `search` queries about the component, behavior, boundary, protocol, or failure. Every hit is only a pointer: open the current file around the returned lines before making a claim or edit, and cite the file rather than Redis.
- Validate edits normally (`python -m compileall`, `pytest -q` when tests apply; later `cargo test`). After changing indexed files, rebuild the cache and confirm that `status` is fresh (exit `0`).
- If Redis is unavailable, continue directly from repository files and explicitly disclose that the optional cache was not consulted or refreshed.
- Never use `FLUSHDB` or `FLUSHALL`, access another namespace, put secrets or runtime/application state in project memory, or depend on raw Redis keys and encoding. Use only `scripts/project_memory.py`.
- Treat stale results, unknown schemas, malformed metadata, and missing chunks as cache misses requiring re-indexing.
- Default endpoint: `redis://localhost:6379/0` (no auth). Override with `--url` or `RGBMVP_PROJECT_MEMORY_URL`.

Full contract: [`docs/PROJECT_MEMORY.md`](docs/PROJECT_MEMORY.md).

## Implementation rules

- Prefer **shared `/v1` JSON** for CLI and web; do not fork validation logic into the UI.
- Public demos target **Liquid Testnet** (and Bitcoin testnet for P1 swaps). Mainnet only behind explicit flags after review.
- Never commit seeds, `.env` secrets, faucet hot keys, or consignments with private material.
- When adding features, map them to a scenario id in `docs/SCENARIOS.md` (e.g. `R4`, `S3`).

## Local development

- Python 3.11+ for glue and project memory; package under `src/rgbmvp/`.
- Install: `pip install -e ".[dev]"`.
- Redis on `localhost:6379` is optional and only for project memory unless a future design says otherwise.
- Rust/LWK workspace will land under `crates/` as Phase 0 proceeds.

## Privacy

Do not commit `.env`, credentials, keys, customer data, or production payloads. Prefer `.env.example` for non-secret templates only.
