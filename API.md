# API Reference

## Package

Install name:
- `roto-api-native`

Import name:
- `roto_api`

## Public Symbols

### `roto_api.__version__`

String package version.

## Functions

### `roto_api.load_lookup(data_dir)`

Loads the Rust lookup engine from an already prepared data directory.

Parameters:
- `data_dir`: directory containing generated CSV files

Returns:
- `RotoLookup`

Example:

```python
from roto_api import load_lookup

lookup = load_lookup("./data")
```

### `roto_api.open_lookup(data_dir)`

Alias for `load_lookup(data_dir)`.

Parameters:
- `data_dir`: directory containing a prepared local dataset snapshot

Returns:
- `RotoLookup`

Example:

```python
from roto_api import open_lookup

lookup = open_lookup("./data")
```

## Class `roto_api.RotoLookup`

### `RotoLookup(prefixes_file, ris_files, timestamps_dir=None)`

Build a lookup object from explicit file paths.

Parameters:
- `prefixes_file`: path to `delegated_all.csv`
- `ris_files`: list of RIS Whois CSV paths, usually IPv4 and IPv6
- `timestamps_dir`: optional directory containing timestamp CSV files; defaults to the prefix file's parent

### `RotoLookup.from_data_dir(data_dir)`

Convenience constructor that expects:

- `delegated_all.csv`
- one or both of `pfx_asn_dfz_v4.csv` and `pfx_asn_dfz_v6.csv`

Optional metadata files:

- `del_ext.timestamps.json`
- `riswhois.timestamps.json`

If those metadata files are absent, the lookup still loads and `source_status()` returns an empty list.

Returns:
- `RotoLookup`

### `RotoLookup.lookup_ip(ip)`

Run a single-IP longest-prefix lookup.

Parameters:
- `ip`: string IPv4 or IPv6 address

Returns:
- dict with:
  - `ip`: original input IP
  - `prefix`: matched prefix string or `None`
  - `origin_asns`: list of origin ASN strings like `["AS15169"]`
  - `match_type`: match type string

Example:

```python
result = lookup.lookup_ip("8.8.8.8")
```

### `RotoLookup.lookup_ips(ips)`

Run longest-prefix lookup for many IPs in one Rust call.

Parameters:
- `ips`: list of string IPv4/IPv6 addresses

Returns:
- list of result dicts with the same schema as `lookup_ip`

Example:

```python
results = lookup.lookup_ips(["8.8.8.8", "1.1.1.1"])
```

### `RotoLookup.source_status()`

Return metadata about the currently loaded sources.

Returns:
- list of dicts containing:
  - `type`: `rir-alloc` or `bgp`
  - `id`: source identifier
  - `serial`: source serial/timestamp integer
  - `last_updated`: RFC3339 timestamp string

If the loaded snapshot does not include timestamp metadata files, this returns an empty list.

## Dump Preparation

Dump download and transformation are intentionally outside the published package.

Use `scripts/bootstrap_data.py` or your own application code to prepare:

- `delegated_all.csv`
- `pfx_asn_dfz_v4.csv`
- `pfx_asn_dfz_v6.csv`
- optional timestamp metadata files
