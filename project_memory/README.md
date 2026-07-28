# Portable Project Memory v2.0.1 bundle

Copy these paths to another repository while preserving their relative locations:

```text
project-memory.py
project_memory/
.project-memory.json
```

The root entrypoint discovers the repository from the copied package location. The bundle uses
only Python 3.11+ standard-library modules. Redis remains optional and defaults to
`redis://localhost:6379/0`; set `PROJECT_MEMORY_URL` or pass `--url` to override it.

Start with a minimal configuration:

```json
{
  "project_slug": "my-repository",
  "redis_url_envs": ["PROJECT_MEMORY_URL"],
  "exclude_directories": ["generated", "vendor"]
}
```

Add `include_patterns` only when the default multi-language corpus is unsuitable. Configured
patterns replace the defaults. `exclude_directories` only adds repository-specific exclusions;
mandatory secret, runtime-data, dependency, build, symlink, and size exclusions remain enforced.
`redis_url_envs` lists the portable and any legacy repository-specific Redis URL variables.

The privacy boundary is not configurable: sensitive credential/key/token path names and key
container suffixes are always rejected, even when a custom include glob matches them. Only known
text-source types that pass an initial UTF-8/binary probe are admitted; binary fixtures under broad
directories such as `tests/` are skipped rather than aborting an index build. These filters reduce
accidental admission, but repositories must still keep real secrets outside source-oriented paths.

```bash
python3 project-memory.py status
python3 project-memory.py index --incremental
python3 project-memory.py validate
python3 project-memory.py search "authentication boundary" --limit 5
```

Copy the Project Memory operating contract from the source repository's `AGENTS.md` into the
target repository instructions. Repository files always remain authoritative; Redis results are
candidate pointers only.
