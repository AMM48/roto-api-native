from pathlib import Path
import shutil
import uuid

import bootstrap_data as data_module

WORK_ROOT = Path(__file__).resolve().parent / ".work"


def touch_required_files(base: Path) -> None:
    required = [
        "delegated_all.csv",
        "pfx_asn_dfz_v4.csv",
        "pfx_asn_dfz_v6.csv",
        "del_ext.timestamps.json",
        "riswhois.timestamps.json",
    ]
    for name in required:
        (base / name).write_text("", encoding="utf-8")


def make_work_dir() -> Path:
    path = WORK_ROOT / uuid.uuid4().hex
    path.mkdir(parents=True, exist_ok=False)
    return path


def test_resolve_sources_applies_overrides_and_env(monkeypatch):
    monkeypatch.setenv("ROTO_API_DEL_EXT_AFRINIC_URL", "https://env.example/afrinic")
    resolved = data_module.resolve_sources(
        data_module.DEFAULT_DEL_EXT_SOURCES,
        {"apnic": "https://override.example/apnic"},
        "ROTO_API_DEL_EXT",
    )

    assert resolved["afrinic"] == "https://env.example/afrinic"
    assert resolved["apnic"] == "https://override.example/apnic"
    assert resolved["arin"] == data_module.DEFAULT_DEL_EXT_SOURCES["arin"]


def test_ensure_data_skips_rebuild_when_all_files_exist(monkeypatch):
    tmp_path = make_work_dir()
    try:
        touch_required_files(tmp_path)

        def fail(*args, **kwargs):
            raise AssertionError("bootstrap should not run when all files exist")

        monkeypatch.setattr(data_module, "build_delegated_all", fail)
        monkeypatch.setattr(data_module, "build_riswhois", fail)

        result = data_module.prepare_data(tmp_path)

        assert result == tmp_path
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)


def test_ensure_data_refresh_uses_resolved_sources(monkeypatch):
    captured = {}

    def fake_del_ext(data_dir, downloads_dir, refresh, sources):
        captured["del_ext"] = {
            "data_dir": data_dir,
            "downloads_dir": downloads_dir,
            "refresh": refresh,
            "sources": dict(sources),
        }

    def fake_ris(data_dir, downloads_dir, refresh, sources):
        captured["ris"] = {
            "data_dir": data_dir,
            "downloads_dir": downloads_dir,
            "refresh": refresh,
            "sources": dict(sources),
        }

    monkeypatch.setattr(data_module, "build_delegated_all", fake_del_ext)
    monkeypatch.setattr(data_module, "build_riswhois", fake_ris)
    monkeypatch.setenv(
        "ROTO_API_RISWHOIS_RISWHOIS4_URL",
        "https://env.example/riswhois4.gz",
    )

    tmp_path = make_work_dir()
    try:
        result = data_module.prepare_data(
            tmp_path,
            refresh=True,
            del_ext_sources={"arin": "https://override.example/arin"},
        )

        assert result == tmp_path
        assert captured["del_ext"]["refresh"] is True
        assert captured["del_ext"]["sources"]["arin"] == "https://override.example/arin"
        assert captured["ris"]["sources"]["riswhois4"] == "https://env.example/riswhois4.gz"
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)


def test_ensure_data_only_rebuilds_missing_component(monkeypatch):
    captured = {"del_ext": 0, "ris": 0}

    def fake_del_ext(*args, **kwargs):
        captured["del_ext"] += 1

    def fake_ris(*args, **kwargs):
        captured["ris"] += 1

    monkeypatch.setattr(data_module, "build_delegated_all", fake_del_ext)
    monkeypatch.setattr(data_module, "build_riswhois", fake_ris)

    tmp_path = make_work_dir()
    try:
        (tmp_path / "delegated_all.csv").write_text("", encoding="utf-8")
        (tmp_path / "del_ext.timestamps.json").write_text("", encoding="utf-8")

        result = data_module.prepare_data(tmp_path)

        assert result == tmp_path
        assert captured["del_ext"] == 0
        assert captured["ris"] == 1
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)


def test_build_riswhois_reuses_cached_raw_dump_when_csv_missing(monkeypatch):
    tmp_path = make_work_dir()
    try:
        data_dir = tmp_path / "data"
        downloads_dir = tmp_path / "downloads"
        data_dir.mkdir()
        downloads_dir.mkdir()

        (downloads_dir / "riswhois4").write_text(
            "15169\t8.8.8.0/24\n",
            encoding="utf-8",
        )
        (downloads_dir / "riswhois6").write_text(
            "13335\t2001:db8::/32\n",
            encoding="utf-8",
        )

        def fail(*args, **kwargs):
            raise AssertionError("download should not run when raw dumps already exist")

        monkeypatch.setattr(data_module, "download_file", fail)

        data_module.build_riswhois(
            data_dir,
            downloads_dir,
            refresh=False,
            sources=data_module.DEFAULT_RISWHOIS_SOURCES,
        )

        assert (data_dir / "pfx_asn_dfz_v4.csv").read_text(encoding="utf-8") == "8.8.8.0,24,15169\n"
        assert (data_dir / "pfx_asn_dfz_v6.csv").read_text(encoding="utf-8") == "2001:db8::,32,13335\n"
        assert (data_dir / "riswhois.timestamps.json").exists()
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)
