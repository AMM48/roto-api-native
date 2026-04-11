# roto-api-native

`roto-api-native` packages the `roto-api` Rust lookup engine as an installable Python module.

This package is an unofficial Python package/binding based on `NLnetLabs/roto-api`.
It is not affiliated with or endorsed by NLnet Labs.
Original project: `https://github.com/NLnetLabs/roto-api`
License: `BSD-3-Clause`

Install name:
- `roto-api-native`

Python import name:
- `roto_api`

It is meant for high-volume offline lookups where HTTP-per-IP is too expensive. The native path keeps the routing data in Rust memory and lets Python call directly into that code.

## What It Does

- Loads `delegated_all.csv`, `pfx_asn_dfz_v4.csv`, `pfx_asn_dfz_v6.csv`, and the timestamp files into the Rust `Store`.
- Exposes a Python class `roto_api.RotoLookup`.

## What It Does Not Do

- It is not the HTTP server.
- It does not include the Rust worker-pool API runtime.
- It does not run a background monitor that watches dump URLs and hot-reloads automatically.
- It does not download routing dumps on install, import, or lookup.

For long-running or production use, prepare the data snapshot separately and then load it without any network access.

## Data Flow

1. An external script prepares a local dataset snapshot.
2. `load_lookup(...)` or `RotoLookup.from_data_dir(...)` asks Rust to load that prepared snapshot into memory.
3. `lookup_ip(...)` and `lookup_ips(...)` execute inside Rust and return compact Python dictionaries.

The package does not download or transform dump data. It only loads already-prepared files from disk.

## Recommended Dump Workflow

The recommended pattern is:

- your Python app or updater job downloads and prepares the local snapshot
- your application code loads that snapshot with `load_lookup(...)`

This separates:

- dump download and transformation
- snapshot validation and promotion
- lookup serving

That way your application process only reads a prepared snapshot and never depends on remote dump availability during startup.
If you omit the optional timestamp metadata files from that snapshot, `source_status()` simply returns an empty list.

## FFI

FFI means Foreign Function Interface. In this project it is the boundary where Python calls compiled Rust code.

Examples of FFI calls here:

- `roto_api.RotoLookup.from_data_dir(...)`
- `roto_api.RotoLookup.lookup_ip(...)`
- `roto_api.RotoLookup.lookup_ips(...)`

Those calls enter the compiled extension module `roto_api._native` built from `src/python.rs`.

## Package Layout

- `src/python.rs`: Rust bindings exposed to Python through PyO3.
- `python/roto_api/__init__.py`: package exports.
- `scripts/bootstrap_data.py`: optional script-side dump preparation helper.
- `scripts/query_ips_native.py`: repo-local test wrapper that uses the package API directly.

Additional docs:

- `API.md`: public API reference
- `PUBLISHING.md`: release and PyPI publishing checklist

## Install

### From PyPI

Once published, install it like any other package:

```bash
pip install roto-api-native
```

On Linux x86_64 this should prefer a prebuilt wheel. On unsupported platforms, `pip` can fall back to the source distribution and build locally if Rust is installed.

### Build a wheel locally

```powershell
maturin build --release
```

### Install the built wheel

```powershell
$wheel = Get-ChildItem .\target\wheels\*.whl | Select-Object -First 1
pip install --force-reinstall $wheel.FullName
```

### Linux source build

```bash
python -m pip install --upgrade pip maturin
maturin build --release
python -m pip install --force-reinstall target/wheels/roto_api_native-0.2.1-cp39-abi3-*.whl
```

## Use From Python

Recommended explicit flow:

```python
from bootstrap_data import prepare_data
from roto_api import load_lookup

prepare_data("./data")
lookup = load_lookup("./data")

print(lookup.lookup_ip("8.8.8.8"))
print(lookup.lookup_ips(["8.8.8.8", "1.1.1.1"]))
print(lookup.source_status())
```

## Use From The Repo Test Script

```powershell
python .\scripts\query_ips_native.py --data-dir .\data 8.8.8.8 1.1.1.1
```

Force fresh downloads:

```powershell
python .\scripts\query_ips_native.py --data-dir .\data --refresh 8.8.8.8 1.1.1.1
```

Override source URLs from Python:

```python
from bootstrap_data import prepare_data

prepare_data(
    "./data",
    refresh=True,
    del_ext_sources={
        "afrinic": "https://example.invalid/delegated-afrinic-extended-latest",
    },
    riswhois_sources={
        "riswhois4": "https://example.invalid/riswhoisdump.IPv4.gz",
    },
)
```

Override source URLs from environment variables:

```bash
export ROTO_API_DEL_EXT_AFRINIC_URL="https://example.invalid/delegated-afrinic-extended-latest"
export ROTO_API_RISWHOIS_RISWHOIS4_URL="https://example.invalid/riswhoisdump.IPv4.gz"
```

## Upstream Data Sources

Delegated RIR files:

- `https://ftp.afrinic.net/pub/stats/afrinic/delegated-afrinic-extended-latest`
- `https://ftp.apnic.net/stats/apnic/delegated-apnic-extended-latest`
- `https://ftp.arin.net/pub/stats/arin/delegated-arin-extended-latest`
- `https://ftp.lacnic.net/pub/stats/lacnic/delegated-lacnic-extended-latest`
- `https://ftp.ripe.net/pub/stats/ripencc/delegated-ripencc-extended-latest`

RIS Whois dumps:

- `https://www.ris.ripe.net/dumps/riswhoisdump.IPv4.gz`
- `https://www.ris.ripe.net/dumps/riswhoisdump.IPv6.gz`

## Size

Typical artifacts are small because the wheel only contains the Python package code and the compiled native extension. The routing dumps are not bundled into the wheel.

Check wheel size:

```powershell
Get-ChildItem .\target\wheels\*.whl | Select-Object Name,Length
```

Check installed package files:

```powershell
python -c "import roto_api, pathlib; p=pathlib.Path(roto_api.__file__).parent; print(p); [print(f'{f.name}`t{f.stat().st_size}') for f in p.rglob('*') if f.is_file()]"
```

## Cross-Platform Support

There is no single native wheel that works on every operating system and CPU architecture. Native extensions must be built per target platform.

This package is structured so you can build wheels for the common targets:

- Windows x86_64
- Linux x86_64
- Linux aarch64
- macOS x86_64
- macOS arm64

The `abi3` configuration means one wheel can cover multiple Python versions on the same platform, but not every platform.

For architectures without a published wheel, the source distribution is the portability fallback. If the target machine has a supported Rust toolchain and Python headers/runtime, `pip install roto-api-native` can build the extension locally.

## Build Wheels For Multiple Platforms

The GitHub Actions workflows do two things:

- `.github/workflows/pkg.yml` runs CI, builds wheels for the main targets, and produces an sdist.
- `.github/workflows/publish.yml` publishes those artifacts to PyPI when you push a `v*` tag.

Before enabling publishing, configure a PyPI trusted publisher for this GitHub repository and the `pypi` environment used by the workflow.

Release flow:

1. Bump the version in `pyproject.toml` and `Cargo.toml`.
2. Commit and tag it, for example `v0.2.2`.
3. Push the branch and tag.
4. The publish workflow builds wheels and the sdist, then uploads them to PyPI.

## Why It Is Fast

The native package is much faster than HTTP-per-IP because it removes:

- socket I/O for every lookup
- HTTP parsing for every lookup
- JSON serialization and parsing for every lookup
- request queueing between Python and a separate server process

The expensive work stays inside one Rust process and Python only crosses the boundary a small number of times.
