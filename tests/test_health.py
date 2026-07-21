"""Application health unit tests."""

from rgbmvp.config import Settings, is_production
from rgbmvp.health import liveness, readiness


def test_readiness_reports_ready_in_development():
    settings = Settings(app_env="development", log_level="debug")
    report = readiness(settings)
    assert report.status == "ready"
    assert report.production is False
    assert report.checks["config"] == "ok"


def test_is_production_boundary():
    assert is_production(Settings(app_env="production")) is True
    assert is_production(Settings(app_env="development")) is False


def test_liveness():
    assert liveness() == {"status": "alive"}
