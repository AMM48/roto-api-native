# API Reference

Install name:
- `roto-api-native`

Import name:
- `roto_api`

## Package Functions

### `ensure_data`

Download and build the local routing snapshot if needed.

`ensure_data(data_dir, refresh=False, del_ext_sources=None, riswhois_sources=None)`

Parameters

`data_dir`

- Directory where the snapshot files and cached downloads are stored

Type: `str | os.PathLike`

`refresh`

- Re-download upstream sources and rebuild the snapshot even if the expected files already exist

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
  - `delegated_all.csv`
  - at least one of `pfx_asn_dfz_v4.csv` or `pfx_asn_dfz_v6.csv`
- Optional files:
  - `del_ext.timestamps.json`
  - `riswhois.timestamps.json`

Type: `str | os.PathLike`

Filename behavior:

- `load_lookup(data_dir)` expects those exact filenames inside `data_dir`
- It does not search for alternate names
- If you need custom filenames, use `RotoLookup(prefixes_file, ris_files, timestamps_dir=None)` instead

Return value

- A `RotoLookup` object

Exceptions

- `RuntimeError`
  - If the delegated file cannot be opened or parsed
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

`open_lookup(data_dir, refresh=False, del_ext_sources=None, riswhois_sources=None)`

Parameters

`data_dir`

- Same target data directory used by `ensure_data(...)`

Type: `str | os.PathLike`

`refresh`

- Re-download upstream sources before loading

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

`RotoLookup(prefixes_file, ris_files, timestamps_dir=None)`

Parameters

`prefixes_file`

- Path to the delegated allocations file
- Normally this is `delegated_all.csv`

Type: `str`

`ris_files`

- One or more RIS Whois CSV files
- Typically one or both of:
  - `pfx_asn_dfz_v4.csv`
  - `pfx_asn_dfz_v6.csv`

Type: `list[str]`

`timestamps_dir`

- Directory containing optional timestamp metadata files
- Defaults to the parent directory of `prefixes_file`

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
    prefixes_file="./data/delegated_all.csv",
    ris_files=[
        "./data/pfx_asn_dfz_v4.csv",
        "./data/pfx_asn_dfz_v6.csv",
    ],
    timestamps_dir="./data",
)
```

### `RotoLookup.from_data_dir`

Build a lookup object from a prepared data directory.

`RotoLookup.from_data_dir(data_dir)`

Parameters

`data_dir`

- Directory containing:
  - `delegated_all.csv`
  - at least one of `pfx_asn_dfz_v4.csv` or `pfx_asn_dfz_v6.csv`
- May also contain:
  - `del_ext.timestamps.json`
  - `riswhois.timestamps.json`

Type: `str`

Filename behavior:

- `RotoLookup.from_data_dir(data_dir)` expects those exact filenames
- It automatically looks for:
  - `delegated_all.csv`
  - `pfx_asn_dfz_v4.csv`
  - `pfx_asn_dfz_v6.csv`
  - optional timestamp files in the same directory
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
```

### `RotoLookup.lookup_ip`

Run a single-IP longest-prefix lookup.

`lookup_ip(ip)`

Parameters

`ip`

- IPv4 or IPv6 address to look up

Type: `str`

Return value

A dictionary with these keys:

- `ip`
  - The original input IP
  - Type: `str`
- `prefix`
  - The matched longest prefix
  - Type: `str | None`
- `origin_asns`
  - Origin ASNs attached to the matched RIS entry
  - Type: `list[str]`
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
```

### `RotoLookup.lookup_ips`

Run longest-prefix lookup for multiple IPs in one Rust call.

`lookup_ips(ips)`

Parameters

`ips`

- IPv4 and/or IPv6 addresses to look up

Type: `list[str]`

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

- Pipe-delimited delegated-extended records
- No header row required
- The first row is treated as data

`pfx_asn_dfz_v4.csv`

- Comma-separated records
- Row format: `prefix,length,asn`
- No header row required
- The first row is treated as data

`pfx_asn_dfz_v6.csv`

- Comma-separated records
- Row format: `prefix,length,asn`
- No header row required
- The first row is treated as data

`del_ext.timestamps.json`
`riswhois.timestamps.json`

- Optional metadata files
- Despite the `.json` extension, these are CSV-formatted text files for compatibility
- If present, they should contain a header row and CSV records
