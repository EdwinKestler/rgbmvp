#!/usr/bin/env python3
"""Disposable, project-scoped Redis retrieval cache for source discovery."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import math
import os
import re
import socket
import struct
import sys
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

SCHEMA = "project-memory:v1"
EMBEDDING_ID = "feature-hash-sha256-unigram-bigram-v1"
DIMENSIONS = 384
DEFAULT_URL = "redis://localhost:6379/0"
URL_ENV = "RGBMVP_PROJECT_MEMORY_URL"
CHUNK_LINES = 80
CHUNK_OVERLAP = 16
ROOT = Path(__file__).resolve().parents[1]
TOKEN_RE = re.compile(r"[A-Za-z0-9_]+", re.UNICODE)

# Dot-directories that may appear in include patterns.
_ALLOWED_DOT_DIRS = frozenset({".github", ".claude", ".agents", ".codex"})


def project_slug(root: Path = ROOT) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", root.resolve().name.lower()).strip("-")
    if not slug:
        raise ValueError("project directory does not produce a valid slug")
    return slug


def namespace(root: Path = ROOT) -> str:
    return f"{project_slug(root)}:{SCHEMA}"


def included_files(root: Path = ROOT) -> list[Path]:
    """Return deterministic, privacy-conscious source corpus paths."""
    exact = [
        "README.md",
        "AGENTS.md",
        "pyproject.toml",
        ".gitignore",
        ".env.example",
    ]
    paths = [root / item for item in exact if (root / item).is_file()]
    patterns = (
        "docs/**/*.md",
        ".github/workflows/*.yml",
        ".github/workflows/*.yaml",
        "src/**/*.py",
        "tests/**/*.py",
        "scripts/**/*.sh",
        "scripts/**/*.py",
        ".agents/**/*.md",
        ".claude/**/*.md",
        ".codex/**/*.md",
    )
    for pattern in patterns:
        paths.extend(path for path in root.glob(pattern) if path.is_file())
    result: list[Path] = []
    for path in sorted(set(paths), key=lambda p: p.relative_to(root).as_posix()):
        rel = path.relative_to(root).as_posix()
        parts = Path(rel).parts
        if any(part.startswith(".") and part not in _ALLOWED_DOT_DIRS for part in parts[:-1]):
            continue
        if path.name == "project_memory.py":
            continue
        if path.is_symlink() or path.stat().st_size > 1_000_000:
            continue
        # Extra privacy guards for accidental glob matches.
        lower = rel.lower()
        if any(
            token in lower
            for token in (
                "/.env",
                "secret",
                "credential",
                "private_key",
                "id_rsa",
                ".pem",
            )
        ):
            if path.name != ".env.example":
                continue
        result.append(path)
    return result


def corpus_fingerprint(files: Iterable[Path], root: Path = ROOT) -> str:
    digest = hashlib.sha256()
    for path in files:
        rel = path.relative_to(root).as_posix().encode("utf-8")
        raw = path.read_bytes()
        digest.update(struct.pack(">I", len(rel)))
        digest.update(rel)
        digest.update(struct.pack(">Q", len(raw)))
        digest.update(raw)
    return digest.hexdigest()


def tokenize(text: str) -> list[str]:
    return [token.lower() for token in TOKEN_RE.findall(text)]


def embedding(text: str, dimensions: int = DIMENSIONS) -> tuple[float, ...]:
    tokens = tokenize(text)
    features = tokens + [f"{a}\x1f{b}" for a, b in zip(tokens, tokens[1:], strict=False)]
    vector = [0.0] * dimensions
    for feature in features:
        hashed = hashlib.sha256(feature.encode("utf-8")).digest()
        index = int.from_bytes(hashed[:8], "big") % dimensions
        vector[index] += 1.0 if hashed[8] & 1 else -1.0
    norm = math.sqrt(sum(value * value for value in vector))
    if norm:
        vector = [value / norm for value in vector]
    return tuple(vector)


def encode_vector(vector: Iterable[float]) -> str:
    values = tuple(vector)
    return base64.b64encode(struct.pack(f">{len(values)}f", *values)).decode("ascii")


def decode_vector(value: str, dimensions: int = DIMENSIONS) -> tuple[float, ...]:
    raw = base64.b64decode(value.encode("ascii"), validate=True)
    if len(raw) != dimensions * 4:
        raise ValueError("invalid vector dimensions")
    return struct.unpack(f">{dimensions}f", raw)


@dataclass(frozen=True)
class Chunk:
    chunk_id: str
    path: str
    start_line: int
    end_line: int
    text: str


def split_chunks(
    path: str, text: str, size: int = CHUNK_LINES, overlap: int = CHUNK_OVERLAP
) -> list[Chunk]:
    if size < 1 or overlap < 0 or overlap >= size:
        raise ValueError("chunk size must be positive and overlap smaller than size")
    lines = text.splitlines()
    if not lines:
        return []
    chunks = []
    step = size - overlap
    for start in range(0, len(lines), step):
        selected = lines[start : start + size]
        if not selected:
            break
        chunk_text = "\n".join(selected)
        identity = hashlib.sha256(
            f"{path}\0{start + 1}\0{start + len(selected)}\0".encode() + chunk_text.encode()
        ).hexdigest()[:24]
        chunks.append(Chunk(identity, path, start + 1, start + len(selected), chunk_text))
        if start + size >= len(lines):
            break
    return chunks


class RedisError(RuntimeError):
    pass


class RedisClient:
    """Tiny RESP2 client; one connection per command keeps failure behavior simple."""

    def __init__(self, url: str, timeout: float = 3.0):
        parsed = urlparse(url)
        if parsed.scheme != "redis" or parsed.username or parsed.password:
            raise RedisError("URL must be redis://host:port/db with no authentication")
        if parsed.query or parsed.fragment or parsed.path.count("/") > 1:
            raise RedisError("unsupported Redis URL")
        try:
            self.db = int(parsed.path.lstrip("/") or "0")
        except ValueError as exc:
            raise RedisError("Redis database must be an integer") from exc
        if self.db < 0:
            raise RedisError("Redis database must be non-negative")
        self.host = unquote(parsed.hostname or "localhost")
        self.port = parsed.port or 6379
        self.timeout = timeout

    @staticmethod
    def _request(parts: tuple[bytes, ...]) -> bytes:
        body = b"".join(b"$%d\r\n%s\r\n" % (len(part), part) for part in parts)
        return b"*%d\r\n" % len(parts) + body

    @staticmethod
    def _read(stream: Any) -> Any:
        marker = stream.read(1)
        if not marker:
            raise RedisError("Redis closed the connection")
        line = stream.readline()
        if not line.endswith(b"\r\n"):
            raise RedisError("malformed RESP2 response")
        payload = line[:-2]
        if marker == b"+":
            return payload.decode("utf-8")
        if marker == b"-":
            raise RedisError(f"Redis error: {payload.decode('utf-8', 'replace')}")
        if marker == b":":
            return int(payload)
        if marker == b"$":
            length = int(payload)
            if length == -1:
                return None
            value = stream.read(length)
            if len(value) != length or stream.read(2) != b"\r\n":
                raise RedisError("truncated RESP2 bulk string")
            return value
        if marker == b"*":
            length = int(payload)
            return None if length == -1 else [RedisClient._read(stream) for _ in range(length)]
        raise RedisError("unsupported RESP2 response type")

    def execute(self, command: str, *args: str | bytes) -> Any:
        parts = (command.encode("ascii"),) + tuple(
            arg if isinstance(arg, bytes) else arg.encode("utf-8") for arg in args
        )
        try:
            with socket.create_connection((self.host, self.port), self.timeout) as conn:
                conn.settimeout(self.timeout)
                stream = conn.makefile("rb")
                if self.db:
                    conn.sendall(self._request((b"SELECT", str(self.db).encode())))
                    if self._read(stream) != "OK":
                        raise RedisError("Redis SELECT failed")
                conn.sendall(self._request(parts))
                return self._read(stream)
        except (OSError, ValueError) as exc:
            raise RedisError(f"Redis connection/protocol failure: {exc}") from exc


def _json_load(raw: bytes | None) -> Any:
    if raw is None:
        return None
    return json.loads(raw.decode("utf-8"))


def _manifest_state(
    client: RedisClient, root: Path = ROOT
) -> tuple[dict[str, Any], dict[str, Any] | None]:
    files = included_files(root)
    current = {
        "files": [path.relative_to(root).as_posix() for path in files],
        "fingerprint": corpus_fingerprint(files, root),
    }
    try:
        manifest = _json_load(client.execute("GET", f"{namespace(root)}:manifest"))
        if not isinstance(manifest, dict):
            return current, None
        required = {
            "schema",
            "namespace",
            "embedding_id",
            "dimensions",
            "files",
            "chunk_count",
            "fingerprint",
            "chunk_keys",
        }
        if not required.issubset(manifest) or manifest["schema"] != SCHEMA:
            return current, None
        if (
            manifest["namespace"] != namespace(root)
            or manifest["embedding_id"] != EMBEDDING_ID
            or manifest["dimensions"] != DIMENSIONS
        ):
            return current, None
        keys = manifest["chunk_keys"]
        if (
            not isinstance(keys, list)
            or len(keys) != manifest["chunk_count"]
            or any(
                not isinstance(k, str) or not k.startswith(namespace(root) + ":chunk:")
                for k in keys
            )
        ):
            return current, None
        if keys:
            values = client.execute("MGET", *keys)
            if not isinstance(values, list) or any(value is None for value in values):
                return current, None
            for value in values:
                chunk = _json_load(value)
                decode_vector(chunk["vector"])
        return current, manifest
    except (KeyError, TypeError, ValueError, UnicodeError, json.JSONDecodeError):
        return current, None


def status(client: RedisClient, root: Path = ROOT) -> tuple[dict[str, Any], bool]:
    current, manifest = _manifest_state(client, root)
    fresh = bool(
        manifest
        and manifest["fingerprint"] == current["fingerprint"]
        and manifest["files"] == current["files"]
    )
    return {
        "status": "fresh" if fresh else ("stale" if manifest else "missing_or_invalid"),
        "fresh": fresh,
        "namespace": namespace(root),
        "schema": SCHEMA,
        "current_fingerprint": current["fingerprint"],
        "manifest": manifest,
    }, fresh


def build_index(client: RedisClient, root: Path = ROOT) -> dict[str, Any]:
    files = included_files(root)
    all_chunks: list[Chunk] = []
    for path in files:
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError as exc:
            raise ValueError(f"indexed file is not UTF-8: {path.relative_to(root)}") from exc
        all_chunks.extend(split_chunks(path.relative_to(root).as_posix(), text))
    prefix = namespace(root)
    registry_key = f"{prefix}:chunk-registry"
    old_keys: list[str] = []
    try:
        registry = _json_load(client.execute("GET", registry_key))
        if isinstance(registry, list):
            old_keys = [
                key
                for key in registry
                if isinstance(key, str) and key.startswith(prefix + ":chunk:")
            ]
    except (ValueError, UnicodeError, json.JSONDecodeError):
        pass
    if old_keys:
        client.execute("DEL", *old_keys)
    chunk_keys = []
    for chunk in all_chunks:
        key = f"{prefix}:chunk:{chunk.chunk_id}"
        payload = {
            "id": chunk.chunk_id,
            "path": chunk.path,
            "start_line": chunk.start_line,
            "end_line": chunk.end_line,
            "text": chunk.text,
            "tokens": sorted(set(tokenize(chunk.text))),
            "vector": encode_vector(embedding(chunk.text)),
        }
        client.execute("SET", key, json.dumps(payload, sort_keys=True, separators=(",", ":")))
        chunk_keys.append(key)
    manifest = {
        "schema": SCHEMA,
        "namespace": prefix,
        "embedding_id": EMBEDDING_ID,
        "dimensions": DIMENSIONS,
        "files": [path.relative_to(root).as_posix() for path in files],
        "chunk_count": len(chunk_keys),
        "fingerprint": corpus_fingerprint(files, root),
        "chunk_keys": chunk_keys,
    }
    client.execute("SET", registry_key, json.dumps(chunk_keys, separators=(",", ":")))
    client.execute(
        "SET", f"{prefix}:manifest", json.dumps(manifest, sort_keys=True, separators=(",", ":"))
    )
    return manifest


def search(client: RedisClient, query: str, limit: int, root: Path = ROOT) -> dict[str, Any]:
    state, fresh = status(client, root)
    if not fresh:
        raise ValueError(f"index is {state['status']}; run index before search")
    manifest = state["manifest"]
    assert manifest is not None
    values = client.execute("MGET", *manifest["chunk_keys"])
    qvec = embedding(query)
    qtokens = set(tokenize(query))
    results = []
    for raw in values:
        try:
            chunk = _json_load(raw)
            vector = decode_vector(chunk["vector"])
            cosine = sum(a * b for a, b in zip(qvec, vector, strict=True))
            tokens = set(chunk["tokens"])
            lexical = len(qtokens & tokens) / len(qtokens) if qtokens else 0.0
            score = 0.82 * cosine + 0.18 * lexical
            results.append(
                {
                    "path": chunk["path"],
                    "start_line": chunk["start_line"],
                    "end_line": chunk["end_line"],
                    "score": round(score, 6),
                    "pointer": f"{chunk['path']}:{chunk['start_line']}-{chunk['end_line']}",
                    "text": chunk["text"],
                }
            )
        except (KeyError, TypeError, ValueError, UnicodeError, json.JSONDecodeError) as exc:
            raise ValueError("index contains malformed chunk data; re-index required") from exc
    results.sort(key=lambda item: (-item["score"], item["path"], item["start_line"]))
    return {"query": query, "fresh": True, "results": results[:limit]}


def clear(client: RedisClient, root: Path = ROOT) -> dict[str, Any]:
    prefix = namespace(root)
    registry_key = f"{prefix}:chunk-registry"
    manifest_key = f"{prefix}:manifest"
    keys = [registry_key, manifest_key]
    for key in (registry_key, manifest_key):
        try:
            value = _json_load(client.execute("GET", key))
            candidates = (
                value
                if isinstance(value, list)
                else value.get("chunk_keys", [])
                if isinstance(value, dict)
                else []
            )
            keys.extend(
                candidate
                for candidate in candidates
                if isinstance(candidate, str) and candidate.startswith(prefix + ":chunk:")
            )
        except (ValueError, UnicodeError, json.JSONDecodeError):
            continue
    unique = sorted(set(keys))
    deleted = client.execute("DEL", *unique) if unique else 0
    return {"status": "cleared", "namespace": prefix, "deleted_keys": deleted}


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument(
        "--url", default=os.environ.get(URL_ENV, DEFAULT_URL), help=f"Redis URL (env: {URL_ENV})"
    )
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("status")
    commands.add_parser("index")
    search_parser = commands.add_parser("search")
    search_parser.add_argument("query")
    search_parser.add_argument("--limit", type=int, default=5)
    commands.add_parser("clear")
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        client = RedisClient(args.url)
        if args.command == "status":
            output, fresh = status(client)
            print(json.dumps(output, sort_keys=True))
            return 0 if fresh else 2
        if args.command == "index":
            print(
                json.dumps({"status": "indexed", "manifest": build_index(client)}, sort_keys=True)
            )
            return 0
        if args.command == "search":
            if args.limit < 1 or args.limit > 100:
                raise ValueError("--limit must be between 1 and 100")
            print(json.dumps(search(client, args.query, args.limit), sort_keys=True))
            return 0
        print(json.dumps(clear(client), sort_keys=True))
        return 0
    except (RedisError, ValueError) as exc:
        print(
            json.dumps({"status": "error", "error": str(exc), "cache_consulted": False}),
            file=sys.stderr,
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
