# Project Memory

> **Humans:** you can skip this file. The lab CLI, web console, and RGB demos do **not** require Redis.  
> **Agents / AI:** this is the full contract for the optional discovery cache. Also follow [M2M.md](./M2M.md) §3 and [AGENTS.md](../AGENTS.md).

Project Memory v2 is an optional local source-discovery index. Repository files are always the authoritative source of truth. Redis is a deterministic, derived, disposable retrieval cache; a hit is only a path and line-range pointer, and the current file must be opened before any claim or edit.

## CLI contract

Agents use the portable root `project-memory.py`; the raw Redis representation is private, opaque, and unstable. `scripts/project_memory.py` remains a compatibility wrapper.

```bash
python3 project-memory.py index --incremental
python3 project-memory.py status
python3 project-memory.py validate
python3 project-memory.py search "health readiness boundary" --limit 5
python3 project-memory.py clear
```

All output is JSON.

| Command | Success behavior | Exit codes |
|---------|------------------|------------|
| `status` | Prints machine-parseable manifest and freshness data | `0` fresh; `2` missing/stale/invalid; `1` connection/protocol/command error |
| `index` | Stages and atomically activates this project's generation | `0` success; `1` error |
| `validate` | Validates the active generation and current corpus | `0` fresh; `2` invalid/stale; `1` error |
| `search QUERY [--limit N]` | Ranked path, line range, score, and text pointers | `0` success; `1` error (rejects missing/stale by default) |
| `clear` | Deletes only this namespace's recorded keys | `0` success; `1` error |

`clear` never runs `FLUSHDB` or `FLUSHALL`. Connection and protocol failures are reported clearly with `cache_consulted: false`.

## Connection configuration

- Default endpoint: `redis://localhost:6379/0` (no authentication).
- Override: `--url redis://host:port/db`, `PROJECT_MEMORY_URL`, or legacy `RGBMVP_PROJECT_MEMORY_URL`.
- Authentication, TLS, query parameters, and non-`redis` schemes are intentionally unsupported.
- If Redis is unavailable, continue from repository files and explicitly report that the optional cache was not consulted or refreshed.

## Namespace, schema, and freshness

The lowercase project directory name produces the isolated namespace:

```text
rgbmvp:project-memory:v2:*
```

Schema id: `project-memory:v2`. Embedding id: `feature-hash-sha256-unigram-bigram-v1` (384 dimensions).

Each generation manifest records schema, namespace, generation, embedding identifier, dimensions, the exact ordered file list, per-file hashes, chunk count, chunk keys, and a SHA-256 corpus fingerprint computed from every included relative path and its exact bytes. Any indexed-file byte change makes the active generation stale.

Source is divided into deterministic 80-line chunks with 16 lines of overlap. Retrieval uses deterministic, locally computed feature-hashed unigrams and bigrams (SHA-256, signed accumulation, L2 normalization) with cosine ranking and a small exact-token lexical component. It uses only Python's standard library: no model download, external embedding API, `redis-py`, NumPy, Redis Stack, RediSearch, RedisJSON, or `redis-cli`.

Re-indexing reuses unchanged content-addressed chunks, pipelines missing chunk writes, stages a complete manifest, registers it for cleanup, then atomically switches `active-generation`. Readers see either the old complete generation or the new complete generation. Unknown schema, malformed metadata/vector data, decoding errors, or missing chunks are cache misses requiring re-indexing.

**Raw Redis layout, hash fields, vector encoding, and stored text formatting are private implementation details, not a stable API.** Agents must not depend on them.

## Corpus and privacy

Included content is deliberately source-oriented:

- root `README.md`, `AGENTS.md`, `pyproject.toml`, `.gitignore`, and non-secret `.env.example`;
- Markdown in `docs/`;
- Python application source and tests;
- Rust workspace crates and project-owned programs;
- web HTML/JavaScript/CSS and deployment/container configuration;
- CI workflow YAML under `.github/workflows/` when present;
- agent instruction markdown under `.agents/`, `.claude/`, or `.codex/` when present;
- other `scripts/**/*.py` and `scripts/**/*.sh` (except the memory tool itself).

Excluded content includes:

- generated builds, dependencies, virtual environments, package caches;
- binaries, archives, editor state, logs, coverage, temporary files;
- environment files with secrets (`.env`), credentials, private keys, tokens, passkeys;
- production/customer data, personal data, operational payloads;
- databases and local `data/` trees;
- symlinks and files larger than 1 MB;
- the portable memory implementation and compatibility wrapper themselves.

Portable bundle v2.0.1 makes sensitive filename/path and private-key suffix
filters mandatory and non-overridable. Known text-source types must also pass a
UTF-8/binary probe, so broad test/schema globs cannot admit images, databases,
archives, or invalid text payloads.

Never cache credentials, tokens, passkeys, device keys, personal data, production payloads, or uncommitted content copied from external systems. Never write application/runtime state into the project-memory namespace.

Inspect actual coverage after indexing:

```bash
python3 project-memory.py status | python3 -m json.tool
```

Confirm the manifest includes representative `src/`, `tests/`, `docs/`, configuration, and agent files, and does not include secrets, data dumps, or the memory tool.

## Machine workflow

1. Run `status` before broad exploration; run `index` when absent or stale.
2. Issue two or three focused intent queries (component, behavior, boundary, protocol, or failure terms) instead of dumping the corpus.
3. Open every returned **current** source location before relying on it; cite the file, never Redis.
4. Make and validate changes with the repository's normal checks (`pytest -q`, syntax checks).
5. Re-index after indexed-file edits and require a final fresh `status`.

Redis may be shared across projects. Never use `FLUSHDB`, `FLUSHALL`, wildcard deletion outside this namespace, or raw-key automation. A failed Redis operation does not prevent direct repository inspection.

## Operator examples

```bash
# Build / refresh
python3 project-memory.py index --incremental

# Freshness check (exit 0 only when fresh)
python3 project-memory.py status; echo exit:$?

# Focused discovery
python3 project-memory.py search "readiness health config boundary" --limit 5
python3 project-memory.py search "project memory namespace fingerprint" --limit 5

# Remove only this project's keys
python3 project-memory.py clear

# Custom endpoint
python3 project-memory.py --url redis://127.0.0.1:6379/0 status
PROJECT_MEMORY_URL=redis://127.0.0.1:6379/0 python3 project-memory.py index --incremental
```

## Failures

| Situation | Behavior |
|-----------|----------|
| Redis down / refused | Exit `1`, JSON error on stderr, `cache_consulted: false` |
| Bad URL / auth present | Exit `1`, clear validation error |
| Missing or stale index | `status` exit `2`; `search` fails with re-index instruction |
| Unknown schema / malformed chunks | Treated as cache miss / invalid; re-index required |
| Shared Redis | Only this namespace's recorded keys are written or deleted |

## Unstable representation

The bytes stored under `rgbmvp:project-memory:v2:*` may change without notice within a future schema revision. Do not document, scrape, or hard-code key names, vector formats, or chunk payloads outside this tool.

## Portable bundle

The reusable unit is `project-memory.py`, `project_memory/`, and `.project-memory.json`. The package discovers the repository root from its own location and has no dependency on rgbmvp runtime code or installation. The configuration preserves this repository's corpus and privacy boundaries; when copying elsewhere, set a unique `project_slug` and review its include patterns and additive exclusions.
