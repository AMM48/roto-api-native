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

The convenience loaders expect a directory containing these filenames:

- `delegated_all.csv`
- `pfx_asn_dfz_v4.csv` and/or `pfx_asn_dfz_v6.csv`
- optional metadata files:
  - `del_ext.timestamps.json`
  - `riswhois.timestamps.json`

If you use:

- `load_lookup(data_dir)`, or
- `RotoLookup.from_data_dir(data_dir)`

then those filenames matter. The loader does not search arbitrary names.

If you need custom filenames, use:

```python
from roto_api import RotoLookup

lookup = RotoLookup(
    prefixes_file="./custom/delegated.csv",
    ris_files=["./custom/ris_v4.csv", "./custom/ris_v6.csv"],
    timestamps_dir="./custom",
)
```

## Manual File Format

If you build the snapshot yourself:

`delegated_all.csv`
- pipe-delimited delegated-extended records
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

Functions:

- `ensure_data(data_dir, refresh=False, del_ext_sources=None, riswhois_sources=None)`
- `load_lookup(data_dir)`
- `open_lookup(data_dir, refresh=False, del_ext_sources=None, riswhois_sources=None)`

Class:

- `RotoLookup`

Methods:

- `RotoLookup.from_data_dir(data_dir)`
- `RotoLookup.lookup_ip(ip)`
- `RotoLookup.lookup_ips(ips)`
- `RotoLookup.source_status()`

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
