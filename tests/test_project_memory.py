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
    active_key = f"{pm.namespace(root)}:active-generation"
    key = redis.data[active_key].decode()
    manifest = json.loads(redis.data[key])
    manifest["schema"] = "project-memory:v999"
    redis.data[key] = json.dumps(manifest).encode()
    assert pm.status(redis, root)[0]["status"] == "missing_or_invalid"


def test_reindex_reuses_unchanged_content_addressed_chunks(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)
    redis.commands.clear()
    second = pm.build_index(redis, root)
    chunk_sets = [
        command
        for command in redis.commands
        if command[0] == "SET" and ":chunk:" in command[1]
    ]
    assert first["generation"] == second["generation"]
    assert chunk_sets == []


def test_interrupted_rebuild_does_not_switch_active_generation(tmp_path):
    root = make_repo(tmp_path)
    redis = FakeRedis()
    first = pm.build_index(redis, root)
    active_key = f"{pm.namespace(root)}:active-generation"
    old_pointer = redis.data[active_key]
    (root / "src" / "pkg" / "service.py").write_text("def replacement(): pass\n")

    original_execute = redis.execute

    def fail_before_commit(command, *args):
        if command == "SET" and args[0] == active_key:
            raise RuntimeError("simulated interruption")
        return original_execute(command, *args)

    redis.execute = fail_before_commit
    with pytest.raises(RuntimeError, match="simulated interruption"):
        pm.build_index(redis, root)
    assert redis.data[active_key] == old_pointer
    assert json.loads(redis.data[old_pointer.decode()])["generation"] == first["generation"]


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
