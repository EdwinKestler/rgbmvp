# Repository agent instructions

This repository is **rgbmvp** — a **public lab** for **RGB on Liquid Testnet**
(CLI + browser lab console + `/v1` API). **Repository files are always the
authoritative source of truth.**

## Machine entry (preferred)

1. **[docs/M2M.md](docs/M2M.md)** — goals, invariants, `/v1` map, project-memory protocol  
2. **[docs/PURPOSE_AND_USAGE.md](docs/PURPOSE_AND_USAGE.md)** — human purpose · **[docs/MANIFESTO.md](docs/MANIFESTO.md)** — community vision  
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) · [docs/SCENARIOS.md](docs/SCENARIOS.md) · [docs/STACK.md](docs/STACK.md)

Phase closures: [P1_CLOSED](docs/P1_CLOSED.md) · [P2_CLOSED](docs/P2_CLOSED.md) · [P3_CLOSED](docs/P3_CLOSED.md).  
**Next (protocol first, localhost):** [docs/ROADMAP_NEXT.md](docs/ROADMAP_NEXT.md) — S3 CLI done ([S3_RGB_WRAP](docs/S3_RGB_WRAP.md)); then C2/C4; **U4 before any public Internet**.  
Headless protocol kit: [docs/HEADLESS.md](docs/HEADLESS.md).

## Do not confuse

- **Native Liquid issued assets** (LWK `issue_asset`) ≠ **RGB contracts** (`rgb:` + consignments + seal + commitment).  
- **Cross-chain** = atomic swap of **twins**, not moving one contract id.  
- **Lightning / CLN:** not required for P0–P3; do not block on `lightningd`.  
- **Wallet name** (`btc-alice`, `bob`) ≠ payment **address** (`tb1…`, `tlq1…`).

## Project Memory (optional Redis discovery cache)

**Humans may ignore this.** Agents: use only `scripts/project_memory.py`.

```bash
python3 scripts/project_memory.py status   # 0=fresh, 2=stale/missing, 1=error
python3 scripts/project_memory.py index    # if status ≠ 0
python3 scripts/project_memory.py search "QUERY" --limit 5
```

- Hits are **pointers only** — open the file before claiming or editing; cite the file, not Redis.  
- After changing indexed files, re-`index` and confirm `status` exit `0`.  
- If Redis is down: continue from files; set/disclose `cache_consulted: false`.  
- **Never** `FLUSHDB` / `FLUSHALL`, other namespaces, secrets in the cache, or raw Redis keys.  
- Default: `redis://localhost:6379/0` · override: `--url` or `RGBMVP_PROJECT_MEMORY_URL`.

Full contract: [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md) · protocol: [docs/M2M.md](docs/M2M.md) §3.

## Implementation rules

- Prefer shared **`/v1` JSON** for CLI and web; no duplicate validation in the UI.  
- Public demos: **Liquid Testnet** + **Bitcoin testnet** for P1/P3 swap. Mainnet only with explicit human flag.  
- Never commit `.env`, `.rgbmvp/`, WIF, real seeds, or private consignments.  
- Map features to scenario ids in `docs/SCENARIOS.md` (`R*`, `S*`, `C*`, `U*`).  
- `GET /v1/swap/*` must keep **preimage redacted**.

## Local development

- Python 3.11+ for glue and project memory: `pip install -e ".[dev]"`.  
- Rust: `cargo build -p lab-cli` → `./target/debug/rgbmvp`.  
- Redis optional (project memory only).  
- P2 Simplicity demos: Docker Elements via `./scripts/regtest_simplicity.sh up`.

## Privacy

Do not commit `.env`, credentials, keys, customer data, or production payloads.  
Prefer `.env.example` for non-secret templates only.
