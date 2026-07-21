"""Application configuration boundaries (no secrets)."""

from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Settings:
    """Runtime settings loaded from environment with safe defaults."""

    app_env: str = "development"
    log_level: str = "info"
    redis_url: str = "redis://localhost:6379/0"

    @classmethod
    def from_env(cls) -> Settings:
        return cls(
            app_env=os.environ.get("APP_ENV", "development"),
            log_level=os.environ.get("LOG_LEVEL", "info"),
            redis_url=os.environ.get("REDIS_URL", "redis://localhost:6379/0"),
        )


def is_production(settings: Settings | None = None) -> bool:
    """Return True when the process is running in a production boundary."""
    current = settings or Settings.from_env()
    return current.app_env.lower() in {"prod", "production"}
