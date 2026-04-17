import importlib.util
import shutil
import uuid
from pathlib import Path

SCRIPT_PATH = Path(__file__).resolve().parents[1] / "scripts" / "query_ips_native.py"
WORK_ROOT = Path(__file__).resolve().parent / ".work"
SPEC = importlib.util.spec_from_file_location("query_ips_native", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def make_work_dir() -> Path:
    path = WORK_ROOT / uuid.uuid4().hex
    path.mkdir(parents=True, exist_ok=False)
    return path


def test_load_ips_supports_one_per_line_file():
    tmp_path = make_work_dir()
    try:
        source = tmp_path / "ips.txt"
        source.write_text("8.8.8.8\n# comment\n1.1.1.1\n", encoding="utf-8")

        assert MODULE.load_ips(file_path=source) == ["8.8.8.8", "1.1.1.1"]
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)


def test_load_ips_supports_benchmark_csv():
    tmp_path = make_work_dir()
    try:
        source = tmp_path / "ips.csv"
        source.write_text(
            "ipv4,ipv6\n8.8.8.8,2001:4860:4860::8888\n"
            "1.1.1.1,2606:4700:4700::1111\n",
            encoding="utf-8",
        )

        assert MODULE.load_ips(file_path=source) == [
            "8.8.8.8",
            "2001:4860:4860::8888",
            "1.1.1.1",
            "2606:4700:4700::1111",
        ]
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)


def test_load_ips_rejects_invalid_input():
    tmp_path = make_work_dir()
    try:
        source = tmp_path / "ips.csv"
        source.write_text("ipv4\nnot-an-ip\n", encoding="utf-8")

        try:
            MODULE.load_ips(file_path=source)
        except ValueError as err:
            assert "invalid IP 'not-an-ip'" in str(err)
        else:
            raise AssertionError("expected ValueError")
    finally:
        shutil.rmtree(tmp_path, ignore_errors=True)
