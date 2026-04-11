#!/usr/bin/env python3
import argparse
import time

try:
    from roto_api import open_lookup
except ModuleNotFoundError as err:
    raise SystemExit(
        "roto_api is not installed in this Python environment.\n"
        "Install the package first, for example:\n"
        "  python -m pip install --force-reinstall .\\target\\wheels\\roto_api_native-*.whl"
    ) from err
except ImportError as err:
    raise SystemExit(
        "roto_api is installed, but it is an older build that does not expose the current public API.\n"
        "Rebuild and reinstall the current package, for example:\n"
        "  maturin build --release\n"
        "  python -m pip install --force-reinstall .\\target\\wheels\\roto_api_native-*.whl"
    ) from err


def load_ips(file_path=None, ips=None):
    values = list(ips or [])
    if file_path:
        with open(file_path, "r", encoding="utf-8") as handle:
            for line in handle:
                value = line.strip()
                if value and not value.startswith("#"):
                    values.append(value)
    return values


def build_parser():
    parser = argparse.ArgumentParser(
        description="Query the installed roto_api package for a list of IP addresses."
    )
    parser.add_argument(
        "--data-dir",
        default="./data",
        help="Directory containing or receiving the generated CSV and timestamp files. Defaults to ./data.",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Force re-download of the upstream datasets before running lookups.",
    )
    parser.add_argument("--file", help="Optional file with one IP per line.")
    parser.add_argument("ips", nargs="*", help="IP addresses to query.")
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)

    ips = load_ips(file_path=args.file, ips=args.ips)
    if not ips:
        parser.error("provide at least one IP or use --file")

    started_at = time.perf_counter()
    lookup = open_lookup(args.data_dir, refresh=args.refresh)
    results = lookup.lookup_ips(ips)

    print("#\tIP\tPrefix\tOrigin ASN(s)")
    for index, result in enumerate(results, start=1):
        origin_text = ", ".join(result["origin_asns"]) if result["origin_asns"] else "-"
        print(f"{index}\t{result['ip']}\t{result['prefix'] or '-'}\t{origin_text}")

    elapsed = time.perf_counter() - started_at
    print(f"Completed {len(results)} lookups in {elapsed:.2f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
