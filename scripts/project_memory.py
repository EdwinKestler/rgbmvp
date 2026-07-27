#!/usr/bin/env python3
"""Compatibility entrypoint; prefer the repository-root project-memory.py."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

_CORE_SPEC = importlib.util.spec_from_file_location(
    "portable_project_memory_core", ROOT / "project_memory" / "core.py"
)
if _CORE_SPEC is None or _CORE_SPEC.loader is None:
    raise RuntimeError("portable Project Memory core could not be loaded")
_core = importlib.util.module_from_spec(_CORE_SPEC)
sys.modules[_CORE_SPEC.name] = _core
_CORE_SPEC.loader.exec_module(_core)


def __getattr__(name: str) -> Any:
    return getattr(_core, name)


if __name__ == "__main__":
    raise SystemExit(_core.main())
