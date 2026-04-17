#!/usr/bin/env python3
import argparse
import csv
import ipaddress
import time

try:
    from roto_api import open_lookup
except ModuleNotFoundError as err:
    raise SystemExit(
        "roto_api is not installed in this Python environment.\n"
        "Install the package first, for example:\n"
        "  python -m pip install --force-reinstall "
        ".\\target\\wheels\\roto_api_native-*.whl"
    ) from err
except ImportError as err:
    raise SystemExit(
        "roto_api is installed, but it is an older build that does not "
        "expose the current public API.\n"
        "Rebuild and reinstall the current package, for example:\n"
        "  maturin build --release\n"
        "  python -m pip install --force-reinstall "
        ".\\target\\wheels\\roto_api_native-*.whl"
    ) from err


HEADER_VALUES = {"ip", "ipv4", "ipv6"}


def parse_ip(value, source):
    try:
        return str(ipaddress.ip_address(value))
    except ValueError as err:
        raise ValueError(f"invalid IP {value!r} in {source}: {err}") from err


def load_ips(file_path=None, ips=None):
    values = list(ips or [])
    if file_path:
        with open(file_path, "r", encoding="utf-8") as handle:
            reader = csv.reader(handle)
            for line_number, row in enumerate(reader, start=1):
                cells = [cell.strip() for cell in row]
                non_empty = [cell for cell in cells if cell]
                if not non_empty:
                    continue
                if len(non_empty) == 1 and non_empty[0].startswith("#"):
                    continue

                valid_ips = []
                invalid_cells = []
                for cell in non_empty:
                    if cell.startswith("#"):
                        continue
                    if cell.lower() in HEADER_VALUES:
                        continue
                    try:
                        valid_ips.append(parse_ip(cell, f"{file_path}:{line_number}"))
                    except ValueError:
                        invalid_cells.append(cell)

                if valid_ips:
                    values.extend(valid_ips)
                    continue

                if invalid_cells:
                    invalid_value = invalid_cells[0]
                    raise ValueError(
                        f"invalid IP {invalid_value!r} in {file_path}:{line_number}"
                    )
    return values


def build_parser():
    parser = argparse.ArgumentParser(
        description="Query the installed roto_api package for a list of IP addresses."
    )
    parser.add_argument(
        "--data-dir",
        default="./data",
        help=(
            "Directory containing or receiving the generated CSV and "
            "timestamp files. Defaults to ./data."
        ),
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Force re-download of the upstream datasets before running lookups.",
    )
    parser.add_argument(
        "--include-delegated",
        action="store_true",
        help=(
            "Also download/load delegated RIR allocation data. Disabled "
            "by default for RIS/BGP-only lookups."
        ),
    )
    parser.add_argument(
        "--min-peer-count",
        type=int,
        default=10,
        help="Only keep origin ASNs with peer_count >= this value. Defaults to 10.",
    )
    parser.add_argument(
        "--mode",
        choices=["validation", "overview"],
        default="overview",
        help=(
            "Lookup mode: 'validation' keeps the matched prefix, "
            "'overview' aligns to the first visible less-specific prefix "
            "after filtering."
        ),
    )
    parser.add_argument("--file", help="Optional file with one IP per line.")
    parser.add_argument("ips", nargs="*", help="IP addresses to query.")
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        ips = load_ips(file_path=args.file, ips=args.ips)
    except ValueError as err:
        parser.error(str(err))
    if not ips:
        parser.error("provide at least one IP or use --file")

    started_at = time.perf_counter()
    lookup = open_lookup(
        args.data_dir,
        refresh=args.refresh,
        include_delegated=args.include_delegated,
    )
    results = lookup.lookup_ips(
        ips,
        min_peer_count=args.min_peer_count,
        mode=args.mode,
    )

    print("#\tIP\tPrefix\tOrigin ASN(s)\tPeer Count\tFallback")
    for index, result in enumerate(results, start=1):
        origin_text = ", ".join(result["origin_asns"]) if result["origin_asns"] else "-"
        peer_count = result["peer_count"] if result["peer_count"] is not None else "-"
        fallback = (
            result["matched_prefix"]
            if result["is_less_specific"] and result["matched_prefix"]
            else "-"
        )
        print(
            f"{index}\t{result['ip']}\t{result['prefix'] or '-'}\t"
            f"{origin_text}\t{peer_count}\t{fallback}"
        )

    elapsed = time.perf_counter() - started_at
    print(f"Completed {len(results)} lookups in {elapsed:.2f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
