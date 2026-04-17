"""Snapshot preparation helpers for ``roto_api``.

These helpers explicitly download and transform upstream routing datasets into
the local CSV/timestamp snapshot expected by the native lookup engine.
Nothing is downloaded at install time or import time. Call ``ensure_data(...)``
when your application wants to bootstrap or refresh its local snapshot.
"""

from __future__ import annotations

import email.utils
import gzip
import os
import tempfile
import urllib.request
from pathlib import Path
from typing import Dict, Mapping, Optional, Tuple

DEFAULT_DEL_EXT_SOURCES = {
    "afrinic": "https://ftp.afrinic.net/pub/stats/afrinic/delegated-afrinic-extended-latest",
    "apnic": "https://ftp.apnic.net/stats/apnic/delegated-apnic-extended-latest",
    "arin": "https://ftp.arin.net/pub/stats/arin/delegated-arin-extended-latest",
    "lacnic": "https://ftp.lacnic.net/pub/stats/lacnic/delegated-lacnic-extended-latest",
    "ripencc": "https://ftp.ripe.net/pub/stats/ripencc/delegated-ripencc-extended-latest",
}

DEFAULT_RISWHOIS_SOURCES = {
    "riswhois4": "https://www.ris.ripe.net/dumps/riswhoisdump.IPv4.gz",
    "riswhois6": "https://www.ris.ripe.net/dumps/riswhoisdump.IPv6.gz",
}

DEL_EXT_OUTPUTS = (
    "delegated_all.csv",
    "del_ext.timestamps.json",
)
RISWHOIS_OUTPUTS = (
    "pfx_asn_dfz_v4.csv",
    "pfx_asn_dfz_v6.csv",
    "riswhois.timestamps.json",
)


def ensure_dir(path: Path) -> None:
    """Create a directory if it does not already exist."""
    path.mkdir(parents=True, exist_ok=True)


def http_date_from_mtime(path: Path) -> str:
    """Return a file mtime formatted as an RFC2822 HTTP-style date."""
    timestamp = path.stat().st_mtime
    return email.utils.formatdate(timestamp, usegmt=True)


def temp_output_path(destination: Path) -> Path:
    """Return a unique temp file path in the destination directory."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, path = tempfile.mkstemp(
        prefix=f"{destination.name}.",
        suffix=".tmp",
        dir=destination.parent,
    )
    os.close(fd)
    return Path(path)


def download_file(url: str, destination: Path) -> str:
    """Download a file atomically and return its Last-Modified value if present."""
    tmp_path = temp_output_path(destination)
    with urllib.request.urlopen(url, timeout=120) as response, open(
        tmp_path, "wb"
    ) as handle:
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            handle.write(chunk)
        last_modified = response.headers.get("Last-Modified")
    os.replace(tmp_path, destination)
    return last_modified or http_date_from_mtime(destination)


def write_del_ext_timestamps(
    data_dir: Path, metadata: Dict[str, Tuple[Path, str]]
) -> None:
    """Write delegated RIR source metadata for ``source_status()``."""
    output_path = data_dir / "del_ext.timestamps.json"
    tmp_path = temp_output_path(output_path)
    with open(tmp_path, "w", encoding="utf-8", newline="") as handle:
        handle.write("rir,file_timestamp,last_modified_header\n")
        for rir in ["afrinic", "apnic", "arin", "lacnic", "ripencc"]:
            source_path, last_modified = metadata[rir]
            handle.write(
                f'{rir},{int(source_path.stat().st_mtime)},"{last_modified}"\n'
            )
    os.replace(tmp_path, output_path)


def write_riswhois_timestamps(
    data_dir: Path, source_path: Path, last_modified: str
) -> None:
    """Write RIS Whois source metadata for ``source_status()``."""
    output_path = data_dir / "riswhois.timestamps.json"
    tmp_path = temp_output_path(output_path)
    with open(tmp_path, "w", encoding="utf-8", newline="") as handle:
        handle.write("rir,file_timestamp,last_modified_header\n")
        handle.write(
            f'riswhois,{int(source_path.stat().st_mtime)},"{last_modified}"\n'
        )
    os.replace(tmp_path, output_path)


def resolve_sources(
    defaults: Mapping[str, str],
    overrides: Optional[Mapping[str, str]],
    env_prefix: str,
) -> Dict[str, str]:
    """Merge default source URLs with caller overrides and env var overrides."""
    resolved = dict(defaults)
    if overrides:
        resolved.update(overrides)
    for key in list(resolved):
        env_name = f"{env_prefix}_{key.upper()}_URL"
        env_value = os.environ.get(env_name)
        if env_value:
            resolved[key] = env_value
    return resolved


def build_delegated_all(
    data_dir: Path,
    downloads_dir: Path,
    refresh: bool,
    sources: Mapping[str, str],
) -> None:
    """Download delegated extended files and concatenate them into one snapshot."""
    metadata = {}
    combined_path = data_dir / "delegated_all.csv"
    tmp_combined = temp_output_path(combined_path)

    for rir, url in sources.items():
        source_path = downloads_dir / f"delegated-{rir}-extended-latest.txt"
        if refresh or not source_path.exists():
            print(f"Downloading {url}")
            last_modified = download_file(url, source_path)
        else:
            last_modified = http_date_from_mtime(source_path)
        metadata[rir] = (source_path, last_modified)

    with open(tmp_combined, "wb") as out_handle:
        for rir in ["afrinic", "apnic", "arin", "lacnic", "ripencc"]:
            with open(metadata[rir][0], "rb") as in_handle:
                while True:
                    chunk = in_handle.read(1024 * 1024)
                    if not chunk:
                        break
                    out_handle.write(chunk)

    os.replace(tmp_combined, combined_path)
    write_del_ext_timestamps(data_dir, metadata)


def _write_riswhois_csv_row(line: str, csv_handle) -> None:
    parts = line.rstrip("\r\n").split("\t")
    if len(parts) < 2 or not parts[0].isdigit() or "/" not in parts[1]:
        return
    prefix, length = parts[1].split("/", 1)
    if not length.isdigit():
        return
    peer_count = parts[2] if len(parts) > 2 else ""
    if peer_count and peer_count.isdigit():
        csv_handle.write(f"{prefix},{length},{parts[0]},{peer_count}\n")
        return
    csv_handle.write(f"{prefix},{length},{parts[0]}\n")


def build_riswhois_csv(gzip_path: Path, raw_path: Path, csv_path: Path) -> None:
    """Expand a RIS gzip dump into raw text and the compact CSV consumed by Rust."""
    tmp_raw = temp_output_path(raw_path)
    tmp_csv = temp_output_path(csv_path)

    with gzip.open(
        gzip_path, "rt", encoding="utf-8", errors="replace"
    ) as gz_handle, open(
        tmp_raw, "w", encoding="utf-8", newline=""
    ) as raw_handle, open(
        tmp_csv, "w", encoding="utf-8", newline=""
    ) as csv_handle:
        for line in gz_handle:
            raw_handle.write(line)
            _write_riswhois_csv_row(line, csv_handle)

    os.replace(tmp_raw, raw_path)
    os.replace(tmp_csv, csv_path)


def build_riswhois_csv_from_raw(raw_path: Path, csv_path: Path) -> None:
    """Rebuild the compact RIS CSV from a previously cached raw dump."""
    tmp_csv = temp_output_path(csv_path)
    with open(
        raw_path, "r", encoding="utf-8", errors="replace", newline=""
    ) as raw_handle, open(tmp_csv, "w", encoding="utf-8", newline="") as csv_handle:
        for line in raw_handle:
            _write_riswhois_csv_row(line, csv_handle)

    os.replace(tmp_csv, csv_path)


def build_riswhois(
    data_dir: Path,
    downloads_dir: Path,
    refresh: bool,
    sources: Mapping[str, str],
) -> None:
    """Download and build IPv4/IPv6 RIS Whois CSV snapshots."""
    csv_v4 = data_dir / "pfx_asn_dfz_v4.csv"
    csv_v6 = data_dir / "pfx_asn_dfz_v6.csv"
    raw_v4 = downloads_dir / "riswhois4"
    raw_v6 = downloads_dir / "riswhois6"

    last_modified_v4 = http_date_from_mtime(raw_v4) if raw_v4.exists() else None
    last_modified_v6 = http_date_from_mtime(raw_v6) if raw_v6.exists() else None

    for key, url in sources.items():
        gzip_path = downloads_dir / f"{key}.gz"
        raw_path = downloads_dir / key
        csv_path = csv_v4 if key == "riswhois4" else csv_v6

        if refresh or not raw_path.exists():
            print(f"Downloading {url}")
            last_modified = download_file(url, gzip_path)
            build_riswhois_csv(gzip_path, raw_path, csv_path)
            gzip_path.unlink(missing_ok=True)
        else:
            if not csv_path.exists():
                build_riswhois_csv_from_raw(raw_path, csv_path)
            last_modified = http_date_from_mtime(raw_path)

        if key == "riswhois4":
            last_modified_v4 = last_modified
        else:
            last_modified_v6 = last_modified

    write_riswhois_timestamps(
        data_dir,
        raw_v6 if raw_v6.exists() else raw_v4,
        last_modified_v6 or last_modified_v4,
    )


def ensure_data(
    data_dir,
    refresh: bool = False,
    include_delegated: bool = False,
    del_ext_sources: Optional[Mapping[str, str]] = None,
    riswhois_sources: Optional[Mapping[str, str]] = None,
) -> Path:
    """Ensure a local routing snapshot exists and return its directory.

    Parameters
    ----------
    data_dir:
        Target directory where snapshot files and cached downloads are stored.
    refresh:
        When true, re-download upstream data and rebuild the snapshot even if
        all expected files already exist.
    include_delegated:
        When true, also download and build delegated RIR allocation files.
        The default RIS/BGP-only flow skips delegated data.
    del_ext_sources:
        Optional mapping of delegated-extended source URL overrides.
    riswhois_sources:
        Optional mapping of RIS Whois source URL overrides.
    """

    data_dir = Path(data_dir)
    del_ext_dir = data_dir / "downloads" / "del_ext"
    ris_dir = data_dir / "downloads" / "riswhois"
    ensure_dir(ris_dir)
    resolved_riswhois_sources = resolve_sources(
        DEFAULT_RISWHOIS_SOURCES, riswhois_sources, "ROTO_API_RISWHOIS"
    )
    ris_required = [data_dir / name for name in RISWHOIS_OUTPUTS]

    if include_delegated:
        ensure_dir(del_ext_dir)
        resolved_del_ext_sources = resolve_sources(
            DEFAULT_DEL_EXT_SOURCES, del_ext_sources, "ROTO_API_DEL_EXT"
        )
        del_ext_required = [data_dir / name for name in DEL_EXT_OUTPUTS]
        if refresh or any(not path.exists() for path in del_ext_required):
            build_delegated_all(
                data_dir, del_ext_dir, refresh, resolved_del_ext_sources
            )
    if refresh or any(not path.exists() for path in ris_required):
        build_riswhois(data_dir, ris_dir, refresh, resolved_riswhois_sources)

    return data_dir
