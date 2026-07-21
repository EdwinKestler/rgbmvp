"""Health and readiness checks for local development."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from rgbmvp.config import Settings, is_production


@dataclass(frozen=True)
class HealthReport:
    """Machine-oriented health payload."""

    status: str
    app_env: str
    production: bool
    checks: dict[str, str]

    def as_dict(self) -> dict[str, Any]:
        return {
            "status": self.status,
            "app_env": self.app_env,
            "production": self.production,
            "checks": dict(self.checks),
        }


def readiness(settings: Settings | None = None) -> HealthReport:
    """Return process readiness without contacting external services."""
    current = settings or Settings.from_env()
    checks = {
        "config": "ok",
        "process": "ok",
    }
    status = "ready" if all(value == "ok" for value in checks.values()) else "degraded"
    return HealthReport(
        status=status,
        app_env=current.app_env,
        production=is_production(current),
        checks=checks,
    )


def liveness() -> dict[str, str]:
    """Minimal liveness signal for orchestration probes."""
    return {"status": "alive"}
