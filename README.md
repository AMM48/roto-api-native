# roto-api-native

`roto-api-native` exposes the `roto-api` Rust lookup engine as an installable Python package for high-volume longest-prefix ASN/prefix enrichment.

This package is an unofficial Python binding based on `NLnetLabs/roto-api`. It is not affiliated with or endorsed by NLnet Labs. The original project is licensed under BSD-3-Clause, and that upstream license is preserved here.

Install name:
- `roto-api-native`

Import name:
- `roto_api`

## Overview

The package can:

- download and prepare a local routing snapshot when you explicitly call `ensure_data(...)`
- load that snapshot into the Rust lookup engine
- run longest-prefix lookups entirely in-process from Python

The package does not:

- install routing dumps at package install time
- fetch data on plain `import roto_api`
- provide the original HTTP API server
- watch upstream dumps in the background or hot-reload automatically

## Install

From PyPI:

```bash
pip install roto-api-native
```

Build locally:

```powershell
maturin build --release
$wheel = Get-ChildItem .\target\wheels\*.whl | Select-Object -First 1
python -m pip install --force-reinstall $wheel.FullName
```

## Quick Start

One-step bootstrap and open:

```python
from roto_api import open_lookup

lookup = open_lookup("./data")

print(lookup.lookup_ip("8.8.8.8"))
print(lookup.lookup_ips(["8.8.8.8", "1.1.1.1"]))
print(lookup.source_status())
```

Explicit bootstrap and later load:

```python
from roto_api import ensure_data, load_lookup

data_dir = ensure_data("./data", refresh=False)
lookup = load_lookup(data_dir)

print(lookup.lookup_ip("8.8.8.8"))
```

Force a refresh:

```python
from roto_api import open_lookup

lookup = open_lookup("./data", refresh=True)
```

## Snapshot Layout

The convenience loaders expect a directory containing:

- `pfx_asn_dfz_v4.csv` and/or `pfx_asn_dfz_v6.csv`
- optional `delegated_all.csv` when `include_delegated=True`
- optional metadata files:
  - `del_ext.timestamps.json` when `include_delegated=True`
  - `riswhois.timestamps.json`

If you use:

- `load_lookup(data_dir)`, or
- `RotoLookup.from_data_dir(data_dir, include_delegated=False)`

then those filenames matter. The loader does not search arbitrary names.

If you need custom filenames, use:

```python
from roto_api import RotoLookup

lookup = RotoLookup(
    ris_files=["./custom/ris_v4.csv", "./custom/ris_v6.csv"],
    prefixes_file="./custom/delegated.csv",
    timestamps_dir="./custom",
)
```

## Manual File Format

If you build the snapshot yourself:

`delegated_all.csv`
- optional pipe-delimited delegated-extended records
- no header row required
- first row is treated as data

`pfx_asn_dfz_v4.csv`
- comma-separated
- format per row: `prefix,length,asn`
- example: `8.8.8.0,24,15169`
- no header row required

`pfx_asn_dfz_v6.csv`
- comma-separated
- format per row: `prefix,length,asn`
- example: `2001:4860::,32,15169`
- no header row required

`del_ext.timestamps.json` and `riswhois.timestamps.json`
- optional metadata files
- despite the `.json` extension, these are CSV-formatted text files for compatibility with the original naming

## Public API

### ensure_data

Download and build the local routing snapshot if needed.

`ensure_data(data_dir, refresh=False, include_delegated=False, del_ext_sources=None, riswhois_sources=None)`

Parameters

`data_dir`

The directory where snapshot files and cached downloads are stored.

Type: `str | os.PathLike`

`refresh`

When enabled, re-download upstream sources and rebuild the snapshot even if the expected files already exist.

Type: `bool`
Default: `False`

`include_delegated`

When enabled, also download and prepare delegated RIR allocation files. The default path is RIS/BGP-only.

Type: `bool`
Default: `False`

`del_ext_sources`

Optional delegated-extended source URL overrides.

Type: `Mapping[str, str] | None`
Default: `None`

`riswhois_sources`

Optional RIS Whois source URL overrides.

Type: `Mapping[str, str] | None`
Default: `None`

Return value

A `pathlib.Path` pointing to the prepared data directory.

Exceptions

`OSError`

If files cannot be created, replaced, or written.

`urllib.error.URLError`

If an upstream download fails.

Example

```python
from roto_api import ensure_data

data_dir = ensure_data("./data")
```

### load_lookup

Load the native lookup engine from a prepared local data directory.

`load_lookup(data_dir)`

Parameters

`data_dir`

The path to a directory containing a prepared routing snapshot.

Required filenames:
- at least one of `pfx_asn_dfz_v4.csv` or `pfx_asn_dfz_v6.csv`

Optional filenames:
- `del_ext.timestamps.json`
- `riswhois.timestamps.json`

Type: `str | os.PathLike`

Return value

A `RotoLookup` object.

Exceptions

`RuntimeError`

If a required data file cannot be opened or parsed.

`ValueError`

If no RIS Whois CSV files are present in the data directory.

Example

```python
from roto_api import load_lookup

lookup = load_lookup("./data")
```

### open_lookup

Ensure the snapshot exists, then load the native lookup engine.

`open_lookup(data_dir, refresh=False, include_delegated=False, del_ext_sources=None, riswhois_sources=None)`

Parameters

`data_dir`

Target data directory used for snapshot preparation and loading.

Type: `str | os.PathLike`

`refresh`

When enabled, re-download upstream sources before loading the snapshot.

Type: `bool`
Default: `False`

`include_delegated`

When enabled, also bootstrap delegated RIR allocation data before loading.

Type: `bool`
Default: `False`

`del_ext_sources`

Optional delegated-extended source URL overrides.

Type: `Mapping[str, str] | None`
Default: `None`

`riswhois_sources`

Optional RIS Whois source URL overrides.

Type: `Mapping[str, str] | None`
Default: `None`

Return value

A `RotoLookup` object.

Exceptions

`OSError`

If snapshot files cannot be created or written during preparation.

`urllib.error.URLError`

If an upstream download fails during preparation.

`RuntimeError`

If the prepared data files cannot be opened or parsed.

Example

```python
from roto_api import open_lookup

lookup = open_lookup("./data", refresh=True)
```

### RotoLookup

Build a lookup object from explicit file paths.

`RotoLookup(ris_files, prefixes_file=None, timestamps_dir=None)`

Parameters

`ris_files`

One or more RIS Whois CSV files, typically one or both of:
- `pfx_asn_dfz_v4.csv`
- `pfx_asn_dfz_v6.csv`

Type: `list[str]`

`prefixes_file`

Optional path to delegated allocation data, usually `delegated_all.csv`.

Type: `str | None`

`timestamps_dir`

Directory containing optional timestamp metadata files. Defaults to the parent directory of `prefixes_file` when provided, otherwise the first RIS file's parent directory.

Type: `str | None`
Default: `None`

Return value

A `RotoLookup` object.

Exceptions

`RuntimeError`

If a required file cannot be opened or parsed.

`ValueError`

If `ris_files` is empty.

Example

```python
from roto_api import RotoLookup

lookup = RotoLookup(
    ris_files=["./data/pfx_asn_dfz_v4.csv", "./data/pfx_asn_dfz_v6.csv"],
    prefixes_file="./data/delegated_all.csv",
    timestamps_dir="./data",
)
```

### RotoLookup.from_data_dir

Build a lookup object from a prepared data directory.

`RotoLookup.from_data_dir(data_dir, include_delegated=False)`

Parameters

`data_dir`

Directory containing:
- at least one of `pfx_asn_dfz_v4.csv` or `pfx_asn_dfz_v6.csv`

May also contain:
- `delegated_all.csv`
- `del_ext.timestamps.json`
- `riswhois.timestamps.json`

Type: `str`

`include_delegated`

Whether to load `delegated_all.csv` and `del_ext.timestamps.json` from `data_dir`.

Type: `bool`

Default: `False`

Return value

A `RotoLookup` object.

Exceptions

`RuntimeError`

If the data files cannot be opened or parsed.

`ValueError`

If no RIS Whois CSV files are found.

Example

```python
from roto_api import RotoLookup

lookup = RotoLookup.from_data_dir("./data")
lookup_with_delegated = RotoLookup.from_data_dir(
    "./data", include_delegated=True
)
```

### RotoLookup.lookup_ip

Run a single-IP longest-prefix lookup.

`lookup_ip(ip, min_peer_count=10, mode="overview")`

For exact route/origin validation use `mode="validation"` and `min_peer_count=0` so low-visibility RIS routes are not hidden.

Parameters

`ip`

IPv4 or IPv6 address to look up.

Type: `str`

Return value

A dictionary with:
- `ip`
- `prefix`
- `matched_prefix`
- `origin_asns`
- `origin_peer_counts`
- `peer_count`
- `is_less_specific`
- `mode`
- `match_type`

Exceptions

`ValueError`

If `ip` is not a valid IPv4 or IPv6 address.

Example

```python
result = lookup.lookup_ip("8.8.8.8")
print(result["prefix"])
print(result["origin_asns"])

overview = lookup.lookup_ip(
    "151.101.2.133",
    min_peer_count=10,
    mode="overview",
)
print(overview["prefix"], overview["matched_prefix"], overview["is_less_specific"])
```

### RotoLookup.lookup_ips

Run longest-prefix lookup for multiple IPs in one Rust call.

`lookup_ips(ips, min_peer_count=10, mode="overview")`

Parameters

`ips`

IPv4 and/or IPv6 addresses to look up.

Type: `list[str]`

Return value

A list of dictionaries with the same schema as `lookup_ip`. Result order matches input order.

Exceptions

`ValueError`

If any input IP is invalid.

Example

```python
results = lookup.lookup_ips(["8.8.8.8", "1.1.1.1"])
for row in results:
    print(row["ip"], row["prefix"], row["origin_asns"])
```

### RotoLookup.source_status

Return source metadata for the loaded snapshot.

`source_status()`

Return value

A list of dictionaries containing:
- `type`
- `id`
- `serial`
- `last_updated`

If timestamp metadata files are absent, this returns an empty list.

Example

```python
for source in lookup.source_status():
    print(source["id"], source["serial"], source["last_updated"])
```

The detailed reference is in `API.md` in the source repository.

## Upstream Sources

Delegated RIR files:

- `https://ftp.afrinic.net/pub/stats/afrinic/delegated-afrinic-extended-latest`
- `https://ftp.apnic.net/stats/apnic/delegated-apnic-extended-latest`
- `https://ftp.arin.net/pub/stats/arin/delegated-arin-extended-latest`
- `https://ftp.lacnic.net/pub/stats/lacnic/delegated-lacnic-extended-latest`
- `https://ftp.ripe.net/pub/stats/ripencc/delegated-ripencc-extended-latest`

RIS Whois dumps:

- `https://www.ris.ripe.net/dumps/riswhoisdump.IPv4.gz`
- `https://www.ris.ripe.net/dumps/riswhoisdump.IPv6.gz`

You can override these URLs via `ensure_data(...)` / `open_lookup(...)` parameters or environment variables.

## Why It Is Fast

Compared with sending one HTTP request per IP, this package avoids:

- socket I/O per lookup
- HTTP parsing and response generation
- JSON serialization/deserialization per lookup
- Python-to-server process overhead

The hot lookup path stays inside one in-process Rust engine.

## Platform Support

Native wheels are platform-specific.

This repo is currently set up to publish wheels for:

- Linux x86_64
- Windows x86_64

An sdist can also be published so unsupported platforms may build locally if they have a suitable Rust toolchain.

## Publishing

Release and PyPI steps are documented in `PUBLISHING.md` in the source repository.
