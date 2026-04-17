#!/usr/bin/env python3
from __future__ import annotations

import argparse
import shutil
from pathlib import Path

from roto_api import load_lookup, open_lookup


def write_snapshot(data_dir: Path) -> None:
    data_dir.mkdir(parents=True, exist_ok=True)
    (data_dir / "pfx_asn_dfz_v4.csv").write_text(
        "8.8.8.0,24,15169,376\n"
        "151.101.0.0,22,54113,354\n"
        "151.101.2.0,23,65530,1\n",
        encoding="utf-8",
    )
    (data_dir / "pfx_asn_dfz_v6.csv").write_text(
        "2001:4860::,32,15169,211\n"
        "2001:db8::,32,64500,50\n"
        "2001:db8:1::,48,64501,2\n",
        encoding="utf-8",
    )
    (data_dir / "delegated_all.csv").write_text(
        "arin|US|ipv4|8.8.8.0|256|20240410|allocated|google\n",
        encoding="utf-8",
    )
    (data_dir / "riswhois.timestamps.json").write_text(
        "rir,file_timestamp,last_modified_header\n"
        'riswhois,123,"Sat, 11 Apr 2026 10:03:01 GMT"\n',
        encoding="utf-8",
    )
    (data_dir / "del_ext.timestamps.json").write_text(
        "rir,file_timestamp,last_modified_header\n"
        'arin,123,"Sat, 11 Apr 2026 10:03:01 GMT"\n',
        encoding="utf-8",
    )


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run artifact-level smoke checks for the built package."
    )
    parser.add_argument(
        "--work-dir",
        default=".",
        help="Directory used to create the temporary snapshot layout.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    work_root = Path(args.work_dir).resolve()
    data_dir = work_root / ".release-smoke-data"

    if data_dir.exists():
        shutil.rmtree(data_dir, ignore_errors=True)

    try:
        write_snapshot(data_dir)

        lookup = load_lookup(data_dir)
        status = lookup.source_status()
        assert_true(
            any(item["id"] == "riswhois" for item in status),
            "RIS timestamp metadata missing in RIS-only load",
        )
        assert_true(
            all(item["id"] != "arin" for item in status),
            "delegated metadata leaked into RIS-only load",
        )

        direct = lookup.lookup_ip("8.8.8.8")
        assert_true(direct["prefix"] == "8.8.8.0/24", "unexpected IPv4 prefix")
        assert_true(
            direct["origin_asns"] == ["AS15169"],
            "unexpected IPv4 origin ASN",
        )

        validation = lookup.lookup_ip(
            "151.101.2.133",
            min_peer_count=0,
            mode="validation",
        )
        assert_true(
            validation["prefix"] == "151.101.2.0/23",
            "validation mode did not keep exact prefix",
        )
        assert_true(
            validation["origin_asns"] == ["AS65530"],
            "validation mode did not keep exact ASN",
        )

        overview = lookup.lookup_ip(
            "151.101.2.133",
            min_peer_count=10,
            mode="overview",
        )
        assert_true(
            overview["prefix"] == "151.101.0.0/22",
            "overview mode did not use less-specific fallback",
        )
        assert_true(
            overview["is_less_specific"] is True,
            "overview mode did not mark less-specific fallback",
        )

        batch = lookup.lookup_ips(
            ["8.8.8.8", "2001:4860::8888"],
            min_peer_count=0,
            mode="validation",
        )
        assert_true(len(batch) == 2, "batch lookup returned wrong result count")
        assert_true(
            batch[1]["origin_asns"] == ["AS15169"],
            "IPv6 batch lookup returned unexpected ASN",
        )

        delegated = load_lookup(data_dir, include_delegated=True)
        delegated_status = delegated.source_status()
        assert_true(
            any(item["id"] == "arin" for item in delegated_status),
            "delegated metadata missing when delegated loading is enabled",
        )

        convenience = open_lookup(
            data_dir,
            refresh=False,
            include_delegated=True,
        )
        convenience_status = convenience.source_status()
        assert_true(
            any(item["id"] == "arin" for item in convenience_status),
            "open_lookup did not preserve delegated metadata",
        )
    finally:
        shutil.rmtree(data_dir, ignore_errors=True)

    print("release smoke passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
