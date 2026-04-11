import sys
from types import SimpleNamespace
from pathlib import Path
import shutil
import uuid

import roto_api

WORK_ROOT = Path(__file__).resolve().parent / ".work"


def make_work_dir() -> Path:
    path = WORK_ROOT / uuid.uuid4().hex
    path.mkdir(parents=True, exist_ok=False)
    return path


def test_open_lookup_bootstraps_then_loads_native(monkeypatch):
    captured = {"ensure_data_args": None}

    class FakeLookup:
        @staticmethod
        def from_data_dir(path):
            captured["path"] = path
            return {"loaded_from": path}

    def fake_ensure_data(
        data_dir,
        refresh=False,
        del_ext_sources=None,
        riswhois_sources=None,
    ):
        captured["ensure_data_args"] = {
            "data_dir": data_dir,
            "refresh": refresh,
            "del_ext_sources": del_ext_sources,
            "riswhois_sources": riswhois_sources,
        }
        return Path(data_dir)

    tmp_path = make_work_dir()
    try:
        monkeypatch.setitem(
            sys.modules,
            "roto_api._native",
            SimpleNamespace(RotoLookup=FakeLookup),
        )
        monkeypatch.setattr(roto_api, "ensure_data", fake_ensure_data)

        result = roto_api.open_lookup(tmp_path, refresh=True)

        assert result == {"loaded_from": str(tmp_path)}
        assert captured["path"] == str(tmp_path)
        assert captured["ensure_data_args"] == {
            "data_dir": tmp_path,
            "refresh": True,
            "del_ext_sources": None,
            "riswhois_sources": None,
        }
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)


def test_roto_lookup_is_loaded_lazily(monkeypatch):
    class FakeLookup:
        pass

    monkeypatch.setitem(
        sys.modules,
        "roto_api._native",
        SimpleNamespace(RotoLookup=FakeLookup),
    )

    assert roto_api.RotoLookup is FakeLookup


def test_load_lookup_uses_prepared_directory(monkeypatch):
    captured = {}

    class FakeLookup:
        @staticmethod
        def from_data_dir(path):
            captured["path"] = path
            return {"loaded_from": path}

    monkeypatch.setitem(
        sys.modules,
        "roto_api._native",
        SimpleNamespace(RotoLookup=FakeLookup),
    )

    result = roto_api.load_lookup(Path("prepared") / "snapshot")

    assert result == {"loaded_from": str(Path("prepared") / "snapshot")}
    assert captured["path"] == str(Path("prepared") / "snapshot")


def test_ensure_data_is_exported():
    assert callable(roto_api.ensure_data)
