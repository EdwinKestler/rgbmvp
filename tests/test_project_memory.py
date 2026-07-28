from __future__ import annotations

import importlib.util
import json
import math
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

SPEC = importlib.util.spec_from_file_location(
    "portable_project_memory_core", Path(__file__).parents[1] / "project_memory" / "core.py"
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
            if len(args) >= 3 and args[2] == "NX" and args[0] in self.data:
                return None
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
        if command == "EVAL":
            script = args[0]
            if "pexpire" in script:
                key, owner = args[2], args[3]
                expected = owner if isinstance(owner, bytes) else owner.encode()
                return 1 if self.data.get(key) == expected else 0
            if "KEYS[2]" in script:
                lock_key, active_key, owner, manifest_key = args[2:]
                expected = owner if isinstance(owner, bytes) else owner.encode()
                if self.data.get(lock_key) != expected:
                    return 0
                value = manifest_key if isinstance(manifest_key, bytes) else manifest_key.encode()
                self.data[active_key] = value
                return 1
            key, owner = args[-2:]
            expected = owner if isinstance(owner, bytes) else owner.encode()
            if self.data.get(key) == expected:
                self.data.pop(key, None)
                return 1
            return 0
        raise AssertionError(command)


def make_repo(tmp_path: Path) -> Path:
    (tmp_path / "src" / "pkg").mkdir(parents=True)
    (tmp_path / "tests").mkdir()
    (tmp_path / "docs").mkdir()
    (tmp_path / "reports").mkdir()
    (tmp_path / "README.md").write_text("# Demo\nsource truth\n")
    (tmp_path / "AGENTS.md").write_text("inspect returned source\n")
    (tmp_path / "pyproject.toml").write_text("[project]\nname='demo'\n")
    (tmp_path / "src" / "pkg" / "service.py").write_text(
        "def forecast_protocol():\n    return 'safe'\n"
    )
    (tmp_path / "tests" / "test_service.py").write_text("def test_forecast_protocol(): pass\n")
    (tmp_path / "docs" / "architecture.md").write_text("failure boundary protocol\n")
    (tmp_path / "reports" / "customer.jsonl").write_text('{"personal":"private"}\n')
    (tmp_path / ".env").write_text("TOKEN=secret\n")
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
        "src/pkg/service.py",
        "tests/test_service.py",
        "docs/architecture.md",
    } <= set(names)
    assert ".env" not in names
    assert "reports/customer.jsonl" not in names
    assert all("project_memory.py" not in name for name in names)


def test_mandatory_sensitive_path_filter_cannot_be_overridden(tmp_path):
    root = make_repo(tmp_path)
    sensitive = {
        "credentials.json": "{}\n",
        "service-account.json": "{}\n",
        "api-keys.yaml": "example: synthetic\n",
        "access_token.txt": "synthetic\n",
        "id_rsa": "synthetic\n",
        "client.pem": "synthetic\n",
        "nested/private_key.json": "{}\n",
        "config/secrets/app.toml": "value = 'synthetic'\n",
        "token.json": "{}\n",
        "refresh_token.txt": "synthetic\n",
        "auth-token.yaml": "value: synthetic\n",
        "oauth_token.toml": "value = 'synthetic'\n",
        "client-secret.ini": "value=synthetic\n",
    }
    for relative, content in sensitive.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    (root / pm.CONFIG_FILE).write_text(json.dumps({"include_patterns": ["**/*"]}))

    names = {path.relative_to(root).as_posix() for path in pm.included_files(root)}
    assert not names.intersection(sensitive)
    assert "README.md" in names
    assert "src/pkg/service.py" in names


def test_token_named_source_module_is_not_overexcluded(tmp_path):
    root = make_repo(tmp_path)
    (root / "src" / "pkg" / "token.py").write_text("class Token: pass\n")

    names = {path.relative_to(root).as_posix() for path in pm.included_files(root)}

    assert "src/pkg/token.py" in names


def test_binary_and_unknown_test_fixtures_are_skipped(tmp_path):
    root = make_repo(tmp_path)
    (root / "tests" / "image.png").write_bytes(b"\x89PNG\r\n\x1a\n")
    (root / "tests" / "state.sqlite").write_bytes(b"SQLite format 3\x00")
    (root / "tests" / "archive.zip").write_bytes(b"PK\x03\x04")
    (root / "tests" / "invalid.py").write_bytes(b"def ok(): pass\n\xff")
    (root / "tests" / "valid.json").write_text('{"fixture": true}\n')

    names = {path.relative_to(root).as_posix() for path in pm.included_files(root)}
    assert "tests/valid.json" in names
    assert "tests/image.png" not in names
    assert "tests/state.sqlite" not in names
    assert "tests/archive.zip" not in names
    assert "tests/invalid.py" not in names

    manifest = pm.build_index(FakeRedis(), root)["manifest"]
    assert "tests/valid.json" in manifest["files"]


def test_env_example_and_supported_extensionless_sources_remain_allowed(tmp_path):
    root = make_repo(tmp_path)
    (root / ".env.example").write_text("TOKEN=replace-me\n")
    (root / "Dockerfile.public").write_text("FROM scratch\n")
    (root / "Makefile").write_text("check:\n\ttrue\n")
    (root / pm.CONFIG_FILE).write_text(
        json.dumps({"include_patterns": [".env.example", "Dockerfile*", "Makefile"]})
    )

    names = {path.relative_to(root).as_posix() for path in pm.included_files(root)}
    assert {".env.example", "Dockerfile.public", "Makefile"} <= names


def test_digest_staleness_and_ranking(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)["manifest"]
    state, fresh = pm.status(redis, root)
    assert fresh and state["manifest"]["fingerprint"] == manifest["fingerprint"]
    hits = pm.search(redis, "forecast_protocol", 3, root)["results"]
    assert hits[0]["path"] in {"src/pkg/service.py", "tests/test_service.py"}
    (root / "src" / "pkg" / "service.py").write_text("def changed(): pass\n")
    assert pm.status(redis, root)[1] is False


def test_malformed_or_missing_chunk_is_cache_miss(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)["manifest"]
    redis.data[manifest["chunk_keys"][0]] = b"not-json"
    assert pm.status(redis, root)[1] is True
    assert pm.validate(redis, root, deep=True)[0]["status"] == "missing_or_invalid"
    pm.build_index(redis, root)
    assert pm.status(redis, root)[1] is True
    assert pm.validate(redis, root, deep=True)[1] is True
    repaired_manifest = pm.status(redis, root)[0]["manifest"]
    redis.data.pop(repaired_manifest["chunk_keys"][0], None)
    assert pm.status(redis, root)[1] is True
    assert pm.validate(redis, root, deep=True)[1] is False
    pm.build_index(redis, root)
    assert pm.validate(redis, root, deep=True)[1] is True


def test_clear_and_reindex_delete_only_namespaced_recorded_keys(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    redis.data["other-project:sentinel"] = b"keep"
    first = pm.build_index(redis, root)["manifest"]
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
    active_key = f"{pm.namespace(root)}:active-generation"
    key = redis.data[active_key].decode()
    manifest = json.loads(redis.data[key])
    manifest["schema"] = "project-memory:v999"
    redis.data[key] = json.dumps(manifest).encode()
    assert pm.status(redis, root)[0]["status"] == "missing_or_invalid"


def test_reindex_reuses_unchanged_content_addressed_chunks(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    redis.commands.clear()
    second_result = pm.build_index(redis, root)
    second = second_result["manifest"]
    chunk_sets = [
        command
        for command in redis.commands
        if command[0] == "SET" and ":chunk:" in command[1]
    ]
    assert first["fingerprint"] == second["fingerprint"]
    assert first["generation"] != second["generation"]
    assert chunk_sets == []
    assert second_result["metrics"]["files"] == {
        "total": 6,
        "new": 0,
        "changed": 0,
        "unchanged": 6,
        "deleted": 0,
    }
    assert second_result["metrics"]["chunks"]["generated"] == 0
    assert second_result["metrics"]["chunks"]["reused"] == second["chunk_count"]


def test_incremental_rebuild_parses_and_embeds_only_changed_file(tmp_path, monkeypatch):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    parsed = []
    embedded = []
    original_split = pm.split_chunks
    original_embedding = pm.embedding

    def observed_split(path, text, size=pm.CHUNK_LINES, overlap=pm.CHUNK_OVERLAP):
        parsed.append(path)
        return original_split(path, text, size, overlap)

    def observed_embedding(text, dimensions=pm.DIMENSIONS):
        embedded.append(text)
        return original_embedding(text, dimensions)

    monkeypatch.setattr(pm, "split_chunks", observed_split)
    monkeypatch.setattr(pm, "embedding", observed_embedding)
    changed_path = root / "src" / "pkg" / "service.py"
    changed_path.write_text("def forecast_protocol():\n    return 'updated'\n")
    redis.commands.clear()

    second_result = pm.build_index(redis, root)
    second = second_result["manifest"]

    assert parsed == ["src/pkg/service.py"]
    assert len(embedded) == len(second["file_chunks"]["src/pkg/service.py"])
    assert second_result["metrics"]["files"]["changed"] == 1
    assert second_result["metrics"]["files"]["unchanged"] == 5
    assert second_result["metrics"]["chunks"]["generated"] == len(embedded)
    assert all(command[0] != "GET" or ":chunk:" not in command[1] for command in redis.commands)
    assert any(command[0] == "MGET" for command in redis.commands)
    for path in first["files"]:
        if path != "src/pkg/service.py":
            assert second["file_chunks"][path] == first["file_chunks"][path]


def test_v20_manifest_without_file_chunk_map_migrates_on_next_index(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    active_key = f"{pm.namespace(root)}:active-generation"
    manifest_key = redis.data[active_key].decode()
    legacy = json.loads(redis.data[manifest_key])
    legacy.pop("file_chunks")
    legacy["bundle_version"] = "2.0.1"
    redis.data[manifest_key] = json.dumps(legacy).encode()

    migrated_result = pm.build_index(redis, root)
    migrated = migrated_result["manifest"]

    assert set(migrated["file_chunks"]) == set(first["files"])
    assert migrated["bundle_version"] == "2.1.2"
    assert migrated_result["metrics"]["files"]["changed"] == len(first["files"])
    assert migrated_result["metrics"]["files"]["unchanged"] == 0
    assert pm.validate(redis, root, deep=True)[1] is True


def test_deleted_and_renamed_files_update_chunk_map_and_metrics(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    deleted_keys = set(first["file_chunks"]["docs/architecture.md"])
    (root / "docs" / "architecture.md").rename(root / "docs" / "design.md")

    second_result = pm.build_index(redis, root)
    second = second_result["manifest"]

    assert "docs/architecture.md" not in second["file_chunks"]
    assert "docs/design.md" in second["file_chunks"]
    assert second_result["metrics"]["files"]["new"] == 1
    assert second_result["metrics"]["files"]["deleted"] == 1
    assert second_result["metrics"]["chunks"]["deleted"] == len(deleted_keys)
    assert deleted_keys.isdisjoint(redis.data)


def test_interrupted_rebuild_does_not_switch_active_generation(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    active_key = f"{pm.namespace(root)}:active-generation"
    old_pointer = redis.data[active_key]
    (root / "src" / "pkg" / "service.py").write_text("def replacement(): pass\n")

    original_execute = redis.execute

    def fail_before_commit(command, *args):
        if command == "EVAL" and "KEYS[2]" in args[0]:
            raise RuntimeError("simulated interruption")
        return original_execute(command, *args)

    redis.execute = fail_before_commit
    with pytest.raises(RuntimeError, match="simulated interruption"):
        pm.build_index(redis, root)
    assert redis.data[active_key] == old_pointer
    assert json.loads(redis.data[old_pointer.decode()])["generation"] == first["generation"]


def test_same_fingerprint_manifest_migration_does_not_overwrite_active_manifest(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    pm.build_index(redis, root)
    active_key = f"{pm.namespace(root)}:active-generation"
    old_pointer = redis.data[active_key]
    old_manifest_key = old_pointer.decode()
    legacy = json.loads(redis.data[old_manifest_key])
    legacy.pop("file_chunks")
    legacy["bundle_version"] = "2.0.1"
    redis.data[old_manifest_key] = json.dumps(legacy, sort_keys=True).encode()
    old_manifest_bytes = redis.data[old_manifest_key]
    original_execute = redis.execute

    def fail_activation(command, *args):
        if command == "EVAL" and "KEYS[2]" in args[0]:
            raise RuntimeError("simulated migration interruption")
        return original_execute(command, *args)

    redis.execute = fail_activation
    with pytest.raises(RuntimeError, match="migration interruption"):
        pm.build_index(redis, root)

    assert redis.data[active_key] == old_pointer
    assert redis.data[old_manifest_key] == old_manifest_bytes


def test_same_fingerprint_cache_repair_interruption_preserves_active_keys(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)["manifest"]
    active_key = f"{pm.namespace(root)}:active-generation"
    old_pointer = redis.data[active_key]
    old_manifest_bytes = redis.data[old_pointer.decode()]
    damaged_key = manifest["chunk_keys"][0]
    redis.data[damaged_key] = b"not-json"
    original_execute = redis.execute

    def fail_activation(command, *args):
        if command == "EVAL" and "KEYS[2]" in args[0]:
            raise RuntimeError("simulated repair interruption")
        return original_execute(command, *args)

    redis.execute = fail_activation
    with pytest.raises(RuntimeError, match="repair interruption"):
        pm.build_index(redis, root)

    assert redis.data[active_key] == old_pointer
    assert redis.data[old_pointer.decode()] == old_manifest_bytes
    assert redis.data[damaged_key] == b"not-json"


def test_staged_registry_precedes_chunk_writes(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    active_key = f"{pm.namespace(root)}:active-generation"
    registry_key = f"{pm.namespace(root)}:chunk-registry"
    old_pointer = redis.data[active_key]
    (root / "README.md").write_text("# Changed before staged write\n")
    original_execute = redis.execute

    def fail_first_staged_chunk(command, *args):
        if command == "SET" and ":chunk:" in args[0]:
            raise RuntimeError("simulated staged write interruption")
        return original_execute(command, *args)

    redis.execute = fail_first_staged_chunk
    with pytest.raises(RuntimeError, match="staged write interruption"):
        pm.build_index(redis, root)

    registry = json.loads(redis.data[registry_key])
    assert redis.data[active_key] == old_pointer
    assert set(first["chunk_keys"]) <= set(registry["chunk_keys"])
    assert old_pointer.decode() in registry["manifest_keys"]
    assert len(registry["manifest_keys"]) == 2

    redis.execute = original_execute
    recovered = pm.build_index(redis, root)["manifest"]
    final_registry = json.loads(redis.data[registry_key])
    assert final_registry == {
        "chunk_keys": recovered["chunk_keys"],
        "manifest_keys": [redis.data[active_key].decode()],
    }
    assert {
        key for key in redis.data if ":chunk:" in key
    } == set(recovered["chunk_keys"])


def test_registry_remains_union_if_garbage_collection_is_interrupted(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)["manifest"]
    active_key = f"{pm.namespace(root)}:active-generation"
    registry_key = f"{pm.namespace(root)}:chunk-registry"
    old_pointer = redis.data[active_key]
    (root / "README.md").write_text("# Changed before garbage collection\n")
    original_execute = redis.execute

    def fail_garbage_collection(command, *args):
        if command == "DEL" and any(key in first["chunk_keys"] for key in args):
            raise RuntimeError("simulated garbage collection interruption")
        return original_execute(command, *args)

    redis.execute = fail_garbage_collection
    with pytest.raises(RuntimeError, match="garbage collection interruption"):
        pm.build_index(redis, root)

    registry = json.loads(redis.data[registry_key])
    assert redis.data[active_key] != old_pointer
    interrupted_manifest = json.loads(redis.data[redis.data[active_key].decode()])
    assert old_pointer.decode() in registry["manifest_keys"]
    assert redis.data[active_key].decode() in registry["manifest_keys"]
    assert set(first["chunk_keys"]) <= set(registry["chunk_keys"])

    redis.execute = original_execute
    recovered = pm.build_index(redis, root)["manifest"]
    final_registry = json.loads(redis.data[registry_key])
    assert final_registry == {
        "chunk_keys": recovered["chunk_keys"],
        "manifest_keys": [redis.data[active_key].decode()],
    }
    obsolete_first_keys = set(first["chunk_keys"]) - set(interrupted_manifest["chunk_keys"])
    assert obsolete_first_keys.isdisjoint(redis.data)


@pytest.mark.parametrize("corruption", ["text", "vector", "tokens", "path", "lines", "id"])
def test_deep_validation_rejects_semantic_chunk_corruption(tmp_path, corruption):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)["manifest"]
    key = manifest["chunk_keys"][0]
    chunk = json.loads(redis.data[key])
    if corruption == "text":
        chunk["text"] += "\nchanged"
    elif corruption == "vector":
        chunk["vector"] = pm.encode_vector(pm.embedding("semantically wrong"))
    elif corruption == "tokens":
        chunk["tokens"] = ["wrong"]
    elif corruption == "path":
        chunk["path"] = "docs/architecture.md"
    elif corruption == "lines":
        chunk["start_line"] = 0
    else:
        chunk["id"] = "0" * 24
    redis.data[key] = json.dumps(chunk).encode()

    assert pm.status(redis, root)[1] is True
    assert pm.validate(redis, root, deep=True)[1] is False


def test_deep_repair_regenerates_only_owner_file(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)["manifest"]
    key = manifest["file_chunks"]["README.md"][0]
    chunk = json.loads(redis.data[key])
    chunk["vector"] = pm.encode_vector(pm.embedding("semantically wrong"))
    redis.data[key] = json.dumps(chunk).encode()

    ordinary = pm.build_index(redis, root)
    assert ordinary["metrics"]["files"]["changed"] == 0
    assert pm.validate(redis, root, deep=True)[1] is False

    repaired = pm.build_index(redis, root, repair_deep=True)
    assert repaired["metrics"]["files"]["changed"] == 1
    assert repaired["metrics"]["files"]["unchanged"] == 5
    assert pm.validate(redis, root, deep=True)[1] is True


def test_duplicate_chunk_keys_make_manifest_invalid(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    manifest = pm.build_index(redis, root)["manifest"]
    active_key = f"{pm.namespace(root)}:active-generation"
    manifest_key = redis.data[active_key].decode()
    duplicate = manifest["chunk_keys"][0]
    owner = next(path for path, keys in manifest["file_chunks"].items() if duplicate in keys)
    manifest["chunk_keys"].append(duplicate)
    manifest["file_chunks"][owner].append(duplicate)
    manifest["chunk_count"] += 1
    redis.data[manifest_key] = json.dumps(manifest).encode()

    assert pm.status(redis, root)[0]["status"] == "missing_or_invalid"


def test_index_lock_rejects_competing_indexer(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    lock_key = f"{pm.namespace(root)}:index-lock"
    redis.data[lock_key] = b"other-owner"

    with pytest.raises(ValueError, match="another Project Memory indexer is active"):
        pm.build_index(redis, root)
    with pytest.raises(ValueError, match="another Project Memory indexer is active"):
        pm.clear(redis, root)

    assert redis.data[lock_key] == b"other-owner"


def test_lost_lock_lease_prevents_activation(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    pm.build_index(redis, root)
    active_key = f"{pm.namespace(root)}:active-generation"
    old_pointer = redis.data[active_key]
    (root / "README.md").write_text("# Changed while lock expires\n")
    original_execute = redis.execute
    refreshes = 0

    def lose_lock_on_final_refresh(command, *args):
        nonlocal refreshes
        if command == "EVAL" and "pexpire" in args[0]:
            refreshes += 1
            if refreshes == 3:
                redis.data.pop(f"{pm.namespace(root)}:index-lock", None)
        return original_execute(command, *args)

    redis.execute = lose_lock_on_final_refresh
    with pytest.raises(ValueError, match="lock ownership was lost"):
        pm.build_index(redis, root)

    assert redis.data[active_key] == old_pointer


def test_lock_release_failure_does_not_mask_indexing_failure(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    original_execute = redis.execute

    def fail_build_and_release(command, *args):
        if command == "SET" and ":chunk:" in args[0]:
            raise RuntimeError("primary indexing failure")
        if command == "EVAL" and "redis.call('del'" in args[0]:
            raise RuntimeError("secondary release failure")
        return original_execute(command, *args)

    redis.execute = fail_build_and_release
    with pytest.raises(RuntimeError, match="primary indexing failure"):
        pm.build_index(redis, root)


def test_repository_change_before_activation_preserves_previous_generation(tmp_path, monkeypatch):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    pm.build_index(redis, root)
    active_key = f"{pm.namespace(root)}:active-generation"
    old_pointer = redis.data[active_key]
    (root / "README.md").write_text("# Planned change\n")
    original_fingerprint = pm.corpus_fingerprint
    calls = 0

    def mutate_before_final_fingerprint(files, snapshot_root=root):
        nonlocal calls
        calls += 1
        if calls == 2:
            (root / "README.md").write_text("# Concurrent change\n")
        return original_fingerprint(files, snapshot_root)

    monkeypatch.setattr(pm, "corpus_fingerprint", mutate_before_final_fingerprint)
    with pytest.raises(ValueError, match="repository changed during indexing"):
        pm.build_index(redis, root)

    assert redis.data[active_key] == old_pointer


def test_mget_batches_are_bounded(tmp_path):
    redis = FakeRedis()
    keys = [f"key:{index}" for index in range(pm.MGET_BATCH_SIZE + 5)]
    for key in keys:
        redis.data[key] = key.encode()

    assert len(pm._mget_batched(redis, keys)) == len(keys)
    batches = [command for command in redis.commands if command[0] == "MGET"]
    assert [len(command) - 1 for command in batches] == [pm.MGET_BATCH_SIZE, 5]


def test_configurable_corpus_patterns_preserve_privacy_exclusions(tmp_path):
    root = make_repo(tmp_path)
    (root / "custom").mkdir()
    (root / "custom" / "service.toml").write_text("feature = 'enabled'\n")
    (root / "data").mkdir(exist_ok=True)
    (root / "data" / "private.toml").write_text("token = 'secret'\n")
    (root / pm.CONFIG_FILE).write_text(
        json.dumps({"include_patterns": ["custom/**/*.toml", "data/**/*.toml"]})
    )
    names = [path.relative_to(root).as_posix() for path in pm.included_files(root)]
    assert "custom/service.toml" in names
    assert "data/private.toml" not in names


def test_project_slug_can_be_configured(tmp_path):
    root = make_repo(tmp_path)
    (root / pm.CONFIG_FILE).write_text(json.dumps({"project_slug": "Shared Service API"}))
    assert pm.namespace(root) == "shared-service-api:project-memory:v2"


def test_repository_can_add_exclusions_and_redis_environment_aliases(tmp_path):
    root = make_repo(tmp_path)
    (root / "generated").mkdir()
    (root / "generated" / "public.md").write_text("derived output\n")
    (root / pm.CONFIG_FILE).write_text(
        json.dumps(
            {
                "include_patterns": ["generated/**/*.md"],
                "exclude_directories": ["generated"],
                "redis_url_envs": ["PROJECT_MEMORY_URL", "DEMO_PROJECT_MEMORY_URL"],
            }
        )
    )
    assert "generated/public.md" not in {
        path.relative_to(root).as_posix() for path in pm.included_files(root)
    }
    assert pm.redis_url_envs(root) == ("PROJECT_MEMORY_URL", "DEMO_PROJECT_MEMORY_URL")


def test_portable_bundle_runs_after_copy_to_another_repository(tmp_path):
    source_root = Path(__file__).parents[1]
    target = tmp_path / "copied-repository"
    target.mkdir()
    shutil.copytree(source_root / "project_memory", target / "project_memory")
    shutil.copy2(source_root / "project-memory.py", target / "project-memory.py")
    (target / ".project-memory.json").write_text(
        json.dumps({"project_slug": "portable-fixture"})
    )
    completed = subprocess.run(
        [sys.executable, "project-memory.py", "--help"],
        cwd=target,
        check=False,
        capture_output=True,
        text=True,
    )
    assert completed.returncode == 0
    assert "Portable, project-scoped Redis retrieval cache" in completed.stdout


def test_root_entrypoint_is_canonical_and_script_entrypoint_is_compatibility_only():
    root = Path(__file__).parents[1]
    canonical = (root / "project-memory.py").read_text()
    wrapper = (root / "scripts" / "project_memory.py").read_text()
    assert "from project_memory import main" in canonical
    assert "Compatibility entrypoint" in wrapper
    assert 'ROOT / "project_memory" / "core.py"' in wrapper
    assert "class RedisClient" not in wrapper

    canonical_help = subprocess.run(
        [sys.executable, "project-memory.py", "--help"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    compatibility_help = subprocess.run(
        [sys.executable, "scripts/project_memory.py", "--help"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    assert canonical_help.returncode == compatibility_help.returncode == 0
    assert canonical_help.stdout.replace("project-memory.py", "ENTRYPOINT") == (
        compatibility_help.stdout.replace("project_memory.py", "ENTRYPOINT")
    )


def test_rgbmvp_configuration_preserves_repository_contract():
    root = Path(__file__).parents[1]
    names = {path.relative_to(root).as_posix() for path in pm.included_files(root)}
    assert pm.namespace(root) == "rgbmvp:project-memory:v2"
    assert pm.redis_url_envs(root) == ("PROJECT_MEMORY_URL", "RGBMVP_PROJECT_MEMORY_URL")
    assert {
        ".project-memory.json",
        "AGENTS.md",
        "docs/M2M.md",
        "src/rgbmvp/config.py",
        "tests/test_health.py",
        "crates/lab-core/src/lib.rs",
        "web/index.html",
        "deploy/cloudrun.yaml",
        ".github/workflows/ci.yml",
        "scripts/bootstrap_testnet_wallets.sh",
    } <= names
    parts = {part for name in names for part in Path(name).parts}
    assert {".rgbmvp", "target", "artifacts", "vendor", "data", "fixtures", "secrets"}.isdisjoint(parts)
    assert "project-memory.py" not in names
    assert "scripts/project_memory.py" not in names
    assert not any(name.startswith("project_memory/") for name in names)
