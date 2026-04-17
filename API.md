# API Reference

Install name:
- `roto-api-native`

Import name:
- `roto_api`

## Package Functions

### `ensure_data`

Download and build the local routing snapshot if needed.

`ensure_data(data_dir, refresh=False, include_delegated=False, del_ext_sources=None, riswhois_sources=None)`

Parameters

`data_dir`

- Directory where the snapshot files and cached downloads are stored

Type: `str | os.PathLike`

`refresh`

- Re-download upstream sources and rebuild the snapshot even if the expected files already exist

Type: `bool`
Default: `False`

`include_delegated`

- When enabled, also download and build delegated RIR allocation files
- The default flow is RIS/BGP-only

Type: `bool`
Default: `False`

`del_ext_sources`

- Optional delegated-extended source URL overrides
- Keys typically include:
  - `afrinic`
  - `apnic`
  - `arin`
  - `lacnic`
  - `ripencc`

Type: `Mapping[str, str] | None`
Default: `None`

`riswhois_sources`

- Optional RIS Whois source URL overrides
- Keys typically include:
  - `riswhois4`
  - `riswhois6`

Type: `Mapping[str, str] | None`
Default: `None`

Return value

- A `pathlib.Path` pointing to the prepared data directory

Exceptions

- `OSError`
  - If files cannot be created, replaced, or written
- `urllib.error.URLError`
  - If an upstream download fails
- Other parse/build exceptions can surface if upstream content is malformed

Example

```python
from roto_api import ensure_data

data_dir = ensure_data("./data")
```

Refresh example:

```python
from roto_api import ensure_data

data_dir = ensure_data("./data", refresh=True)
```

### `load_lookup`

Load the native lookup engine from a prepared local data directory.

`load_lookup(data_dir)`

Parameters

`data_dir`

- The path to a directory containing a prepared routing snapshot.
- Required files:
  - at least one of `pfx_asn_dfz_v4.csv` or `pfx_asn_dfz_v6.csv`
- Optional files:
  - `delegated_all.csv` when `include_delegated=True`
  - `del_ext.timestamps.json` when `include_delegated=True`
  - `riswhois.timestamps.json`

Type: `str | os.PathLike`

Filename behavior:

- `load_lookup(data_dir)` expects those exact filenames inside `data_dir`
- It does not search for alternate names
- If you need custom filenames, use `RotoLookup(ris_files, prefixes_file=None, timestamps_dir=None)` instead

Return value

- A `RotoLookup` object

Exceptions

- `RuntimeError`
  - If an optional delegated file is provided but cannot be opened or parsed
  - If the RIS Whois file cannot be opened or parsed
  - If timestamp metadata exists but is malformed
- `ValueError`
  - If no RIS Whois CSV files are present in the data directory

Example

```python
from roto_api import load_lookup

lookup = load_lookup("./data")
```

### `open_lookup`

Ensure the snapshot exists, then load the native lookup engine.

`open_lookup(data_dir, refresh=False, include_delegated=False, del_ext_sources=None, riswhois_sources=None)`

Parameters

`data_dir`

- Same target data directory used by `ensure_data(...)`

Type: `str | os.PathLike`

`refresh`

- Re-download upstream sources before loading

Type: `bool`
Default: `False`

`include_delegated`

- When enabled, also bootstrap delegated RIR allocation files before loading

Type: `bool`
Default: `False`

`del_ext_sources`

- Optional delegated-extended source URL overrides

Type: `Mapping[str, str] | None`
Default: `None`

`riswhois_sources`

- Optional RIS Whois source URL overrides

Type: `Mapping[str, str] | None`
Default: `None`

Return value

- A `RotoLookup` object

Example

```python
from roto_api import open_lookup

lookup = open_lookup("./data")
```

Refresh example:

```python
lookup = open_lookup("./data", refresh=True)
```

## Class `RotoLookup`

### `RotoLookup`

Build a lookup object from explicit file paths.

`RotoLookup(ris_files, prefixes_file=None, timestamps_dir=None)`

Parameters

`ris_files`

- One or more RIS Whois CSV files
- Typically one or both of:
  - `pfx_asn_dfz_v4.csv`
  - `pfx_asn_dfz_v6.csv`

Type: `list[str]`

`prefixes_file`

- Optional path to delegated allocation data
- Normally this is `delegated_all.csv`

Type: `str | None`

`timestamps_dir`

- Directory containing optional timestamp metadata files
- Defaults to the parent directory of `prefixes_file` when provided, otherwise the first RIS file's parent directory

Type: `str | None`
Default: `None`

Return value

- A `RotoLookup` object

Exceptions

- `RuntimeError`
  - If a required file cannot be opened or parsed
- `ValueError`
  - If `ris_files` is empty

Example

```python
from roto_api import RotoLookup

lookup = RotoLookup(
    ris_files=[
        "./data/pfx_asn_dfz_v4.csv",
        "./data/pfx_asn_dfz_v6.csv",
    ],
    prefixes_file="./data/delegated_all.csv",
    timestamps_dir="./data",
)
```

### `RotoLookup.from_data_dir`

Build a lookup object from a prepared data directory.

`RotoLookup.from_data_dir(data_dir, include_delegated=False)`

Parameters

`data_dir`

- Directory containing:
  - at least one of `pfx_asn_dfz_v4.csv` or `pfx_asn_dfz_v6.csv`
- May also contain:
  - `delegated_all.csv`
  - `del_ext.timestamps.json`
  - `riswhois.timestamps.json`

Type: `str`

`include_delegated`

- Whether to load `delegated_all.csv` and `del_ext.timestamps.json` from `data_dir`

Type: `bool`

Default: `False`

Filename behavior:

- `RotoLookup.from_data_dir(data_dir, include_delegated=False)` expects those exact filenames
- It automatically looks for:
  - `pfx_asn_dfz_v4.csv`
  - `pfx_asn_dfz_v6.csv`
  - optional `delegated_all.csv` only when `include_delegated=True`
  - `riswhois.timestamps.json`
  - `del_ext.timestamps.json` only when `include_delegated=True`
- If your files have different names, use the explicit `RotoLookup(...)` constructor

Return value

- A `RotoLookup` object

Exceptions

- `RuntimeError`
  - If the data files cannot be opened or parsed
- `ValueError`
  - If no RIS Whois CSV files are found

Example

```python
from roto_api import RotoLookup

lookup = RotoLookup.from_data_dir("./data")
lookup_with_delegated = RotoLookup.from_data_dir(
    "./data", include_delegated=True
)
```

### `RotoLookup.lookup_ip`

Run a single-IP longest-prefix lookup.

`lookup_ip(ip, min_peer_count=10, mode="overview")`

For exact route/origin validation, use `mode="validation"` and `min_peer_count=0` so low-visibility RIS routes remain visible.

Parameters

`ip`

- IPv4 or IPv6 address to look up

Type: `str`

`min_peer_count`

- Filter out origin ASNs whose RIS peer count is below this threshold
- Origins without peer-count metadata are kept

Type: `int`
Default: `10`

`mode`

- `validation` keeps the matched prefix even if all origins are filtered out
- `overview` aligns to the first less-specific prefix that still has visible origins after filtering

Type: `str`
Default: `"overview"`

Return value

A dictionary with these keys:

- `ip`
  - The original input IP
  - Type: `str`
- `prefix`
  - The selected prefix after applying the requested lookup mode
  - Type: `str | None`
- `matched_prefix`
  - The exact longest-match prefix before any RIPE-style fallback
  - Type: `str | None`
- `origin_asns`
  - Origin ASNs attached to the matched RIS entry
  - Type: `list[str]`
- `origin_peer_counts`
  - Mapping of origin ASN to the highest RIS peer count seen for that ASN on the matched prefix
  - Type: `dict[str, int]`
- `peer_count`
  - Highest RIS peer count seen among all returned origin ASNs for the matched prefix
  - Type: `int | None`
- `is_less_specific`
  - Whether the returned prefix is a less-specific fallback instead of the exact match
  - Type: `bool`
- `mode`
  - The lookup mode used for the result
  - Type: `str`
- `match_type`
  - Match result type
  - Type: `str`

Exceptions

- `ValueError`
  - If `ip` is not a valid IPv4 or IPv6 address

Example

```python
result = lookup.lookup_ip("8.8.8.8")
print(result["prefix"])
print(result["origin_asns"])
print(result["peer_count"])

all_origins = lookup.lookup_ip("8.8.8.8", min_peer_count=0)
print(all_origins["origin_asns"])

overview = lookup.lookup_ip(
    "151.101.2.133",
    min_peer_count=10,
    mode="overview",
)
print(overview["prefix"], overview["matched_prefix"], overview["is_less_specific"])
```

### `RotoLookup.lookup_ips`

Run longest-prefix lookup for multiple IPs in one Rust call.

`lookup_ips(ips, min_peer_count=10, mode="overview")`

Parameters

`ips`

- IPv4 and/or IPv6 addresses to look up

Type: `list[str]`

`min_peer_count`

- Filter out origin ASNs whose RIS peer count is below this threshold
- Origins without peer-count metadata are kept

Type: `int`
Default: `10`

`mode`

- `validation` keeps the matched prefix even if all origins are filtered out
- `overview` aligns to the first less-specific prefix that still has visible origins after filtering

Type: `str`
Default: `"overview"`

Return value

- A list of dictionaries
- Each dictionary has the same schema as `lookup_ip`
- Result order matches input order

Exceptions

- `ValueError`
  - If any input IP is invalid

Example

```python
results = lookup.lookup_ips(["8.8.8.8", "1.1.1.1"])
for row in results:
    print(row["ip"], row["prefix"], row["origin_asns"])
```

### `RotoLookup.source_status`

Return source metadata for the loaded snapshot.

`source_status()`

Return value

- A list of dictionaries with:
  - `type`
    - `rir-alloc` or `bgp`
    - Type: `str`
  - `id`
    - Source identifier
    - Type: `str`
  - `serial`
    - Source serial / file timestamp
    - Type: `int`
  - `last_updated`
    - Last update timestamp in RFC3339 format
    - Type: `str`

If the snapshot does not include timestamp metadata files, this returns an empty list.

Example

```python
for source in lookup.source_status():
    print(source["id"], source["serial"], source["last_updated"])
```

## Data Preparation

This package can explicitly prepare the snapshot through `ensure_data(...)`.

It does not download anything:

- at install time
- on plain `import roto_api`
- unless your code calls `ensure_data(...)` or `open_lookup(...)`

Typical patterns:

- explicit two-step flow:
  - `ensure_data(...)`
  - `load_lookup(...)`
- one-step convenience flow:
  - `open_lookup(...)`

## Manual Snapshot File Format

If you generate the snapshot yourself and then call `load_lookup(...)` or `RotoLookup.from_data_dir(...)`, the loader expects:

`delegated_all.csv`

- Optional pipe-delimited delegated-extended records, loaded only when delegated data is enabled
- No header row required
- The first row is treated as data

`pfx_asn_dfz_v4.csv`

- Comma-separated records
- Row format: `prefix,length,asn`
- Peer-aware snapshots may include an optional fourth column: `prefix,length,asn,peer_count`
- No header row required
- The first row is treated as data

`pfx_asn_dfz_v6.csv`

- Comma-separated records
- Row format: `prefix,length,asn`
- Peer-aware snapshots may include an optional fourth column: `prefix,length,asn,peer_count`
- No header row required
- The first row is treated as data

`del_ext.timestamps.json`
`riswhois.timestamps.json`

- Optional metadata files
- Despite the `.json` extension, these are CSV-formatted text files for compatibility
- If present, they should contain a header row and CSV records
