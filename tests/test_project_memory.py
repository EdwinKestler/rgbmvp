"""Unit tests for scripts/project_memory.py (no shared Redis mutation)."""

from __future__ import annotations

import importlib.util
import json
import math
import sys
from pathlib import Path

import pytest

SPEC = importlib.util.spec_from_file_location(
    "project_memory", Path(__file__).parents[1] / "scripts" / "project_memory.py"
)
assert SPEC and SPEC.loader
pm = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = pm
SPEC.loader.exec_module(pm)


class FakeRedis:
    def __init__(self):
        self.data: dict[str, bytes] = {}
        self.commands: list[tuple[str, ...]] = []

    def execute(self, command, *args):
        self.commands.append((command, *args))
        if command == "GET":
            return self.data.get(args[0])
        if command == "SET":
            value = args[1] if isinstance(args[1], bytes) else args[1].encode()
            self.data[args[0]] = value
            return "OK"
        if command == "MGET":
            return [self.data.get(key) for key in args]
        if command == "DEL":
            count = 0
            for key in args:
                count += key in self.data
                self.data.pop(key, None)
            return count
        raise AssertionError(command)


def make_repo(tmp_path: Path) -> Path:
    (tmp_path / "src" / "pkg").mkdir(parents=True)
    (tmp_path / "tests").mkdir()
    (tmp_path / "docs").mkdir()
    (tmp_path / "reports").mkdir()
    (tmp_path / "data").mkdir()
    (tmp_path / "scripts").mkdir()
    (tmp_path / "README.md").write_text("# Demo\nsource truth\n")
    (tmp_path / "AGENTS.md").write_text("inspect returned source\n")
    (tmp_path / "pyproject.toml").write_text("[project]\nname='demo'\n")
    (tmp_path / ".gitignore").write_text(".env\n")
    (tmp_path / ".env.example").write_text("APP_ENV=development\n")
    (tmp_path / "src" / "pkg" / "service.py").write_text(
        "def forecast_protocol():\n    return 'safe'\n"
    )
    (tmp_path / "tests" / "test_service.py").write_text("def test_forecast_protocol(): pass\n")
    (tmp_path / "docs" / "architecture.md").write_text("failure boundary protocol\n")
    (tmp_path / "reports" / "customer.jsonl").write_text('{"personal":"private"}\n')
    (tmp_path / "data" / "payload.bin").write_text("binary-ish\n")
    (tmp_path / ".env").write_text("TOKEN=secret\n")
    (tmp_path / "scripts" / "project_memory.py").write_text("# tool itself\n")
    (tmp_path / "scripts" / "helper.sh").write_text("#!/bin/sh\necho ok\n")
    return tmp_path


def test_embedding_is_deterministic_normalized_and_identifier_sensitive():
    first = pm.embedding("ForecastResult forecast_result")
    assert first == pm.embedding("ForecastResult forecast_result")
    assert len(first) == pm.DIMENSIONS
    assert math.sqrt(sum(value * value for value in first)) == pytest.approx(1.0)
    assert first != pm.embedding("unrelated capacity")


def test_chunks_are_bounded_and_overlap():
    chunks = pm.split_chunks("x.py", "\n".join(str(i) for i in range(20)), size=8, overlap=2)
    assert [(chunk.start_line, chunk.end_line) for chunk in chunks] == [(1, 8), (7, 14), (13, 20)]
    assert chunks[0].text.splitlines()[-2:] == chunks[1].text.splitlines()[:2]
    assert chunks == pm.split_chunks("x.py", "\n".join(str(i) for i in range(20)), 8, 2)


def test_corpus_includes_source_tests_config_docs_agents_and_excludes_sensitive(tmp_path):
    root = make_repo(tmp_path)
    names = [path.relative_to(root).as_posix() for path in pm.included_files(root)]
    assert {
        "README.md",
        "AGENTS.md",
        "pyproject.toml",
        ".gitignore",
        ".env.example",
        "src/pkg/service.py",
        "tests/test_service.py",
        "docs/architecture.md",
        "scripts/helper.sh",
    } <= set(names)
    assert ".env" not in names
    assert "reports/customer.jsonl" not in names
    assert "data/payload.bin" not in names
    assert all("project_memory.py" not in name for name in names)


def test_digest_staleness_and_ranking(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)
    state, fresh = pm.status(redis, root)
    assert fresh and state["manifest"]["fingerprint"] == manifest["fingerprint"]
    hits = pm.search(redis, "forecast_protocol", 3, root)["results"]
    assert hits[0]["path"] in {"src/pkg/service.py", "tests/test_service.py"}
    (root / "src" / "pkg" / "service.py").write_text("def changed(): pass\n")
    assert pm.status(redis, root)[1] is False


def test_malformed_or_missing_chunk_is_cache_miss(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)
    redis.data[manifest["chunk_keys"][0]] = b"not-json"
    assert pm.status(redis, root)[0]["status"] == "missing_or_invalid"
    pm.build_index(redis, root)
    redis.data.pop(manifest["chunk_keys"][0], None)
    assert pm.status(redis, root)[1] is False


def test_clear_and_reindex_delete_only_namespaced_recorded_keys(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    redis.data["other-project:sentinel"] = b"keep"
    first = pm.build_index(redis, root)
    old_keys = set(first["chunk_keys"])
    (root / "README.md").write_text("# Changed\n")
    pm.build_index(redis, root)
    assert (
        not (old_keys - set(pm.status(redis, root)[0]["manifest"]["chunk_keys"]))
        & redis.data.keys()
    )
    result = pm.clear(redis, root)
    assert result["status"] == "cleared"
    assert redis.data == {"other-project:sentinel": b"keep"}
    assert all(command[0] != "FLUSHDB" and command[0] != "FLUSHALL" for command in redis.commands)


def test_unknown_schema_is_invalid(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    pm.build_index(redis, root)
    key = f"{pm.namespace(root)}:manifest"
    manifest = json.loads(redis.data[key])
    manifest["schema"] = "project-memory:v999"
    redis.data[key] = json.dumps(manifest).encode()
    assert pm.status(redis, root)[0]["status"] == "missing_or_invalid"


def test_project_slug_and_namespace():
    assert pm.project_slug(Path("/tmp/RGB_MVP")) == "rgb-mvp"
    assert pm.namespace(Path("/tmp/rgbmvp")).startswith("rgbmvp:project-memory:v1")


def test_redis_url_validation():
    with pytest.raises(pm.RedisError):
        pm.RedisClient("redis://user:pass@localhost:6379/0")
    with pytest.raises(pm.RedisError):
        pm.RedisClient("rediss://localhost:6379/0")
    client = pm.RedisClient("redis://localhost:6379/0")
    assert client.host == "localhost" and client.port == 6379 and client.db == 0
