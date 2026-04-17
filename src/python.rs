use crate::{
    Addr, MatchOptions, MatchType, Prefix, Store, TimeStamp, TimeStamps,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

struct CompactLookup {
    ip: String,
    prefix: Option<String>,
    matched_prefix: Option<String>,
    origin_asns: Vec<String>,
    origin_peer_counts: BTreeMap<String, u32>,
    peer_count: Option<u32>,
    is_less_specific: bool,
    mode: String,
    match_type: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LookupMode {
    Validation,
    Overview,
}

impl LookupMode {
    fn parse(mode: &str) -> PyResult<Self> {
        match mode {
            "validation" | "exact" | "current" => Ok(Self::Validation),
            "overview" | "ripe" | "ripe-fallback" | "ripe_fallback" => {
                Ok(Self::Overview)
            }
            _ => Err(PyValueError::new_err(format!(
                "invalid lookup mode '{}': expected 'validation' or 'overview'",
                mode
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Overview => "overview",
        }
    }
}

/// Lookup engine backed by the in-memory Rust store.
#[pyclass(unsendable)]
pub struct RotoLookup {
    store: Store,
    timestamps: TimeStamps,
}

#[pymethods]
impl RotoLookup {
    /// Build a lookup object from explicit CSV/timestamp file paths.
    #[new]
    #[pyo3(signature = (ris_files, prefixes_file=None, timestamps_dir=None))]
    #[pyo3(
        text_signature = "(ris_files, prefixes_file=None, timestamps_dir=None)"
    )]
    fn new(
        ris_files: Vec<String>,
        prefixes_file: Option<String>,
        timestamps_dir: Option<String>,
    ) -> PyResult<Self> {
        load_lookup(
            ris_files.iter().map(PathBuf::from).collect(),
            prefixes_file.map(PathBuf::from),
            timestamps_dir.map(PathBuf::from),
        )
    }

    /// Build a lookup object from a generated data directory.
    #[staticmethod]
    #[pyo3(signature = (data_dir, include_delegated=false))]
    #[pyo3(text_signature = "(data_dir, include_delegated=False)")]
    fn from_data_dir(
        data_dir: String,
        include_delegated: bool,
    ) -> PyResult<Self> {
        let data_dir = PathBuf::from(data_dir);
        let ris_files = ["pfx_asn_dfz_v4.csv", "pfx_asn_dfz_v6.csv"]
            .iter()
            .map(|name| data_dir.join(name))
            .filter(|path| path.exists())
            .collect();
        let prefixes_file = if include_delegated {
            let prefixes_file = data_dir.join("delegated_all.csv");
            prefixes_file.exists().then_some(prefixes_file)
        } else {
            None
        };
        load_lookup(ris_files, prefixes_file, Some(data_dir))
    }

    /// Return the longest-prefix-match result for a single IP address.
    #[pyo3(signature = (ip, min_peer_count=10, mode="overview"))]
    #[pyo3(text_signature = "($self, ip, min_peer_count=10, mode=\"overview\")")]
    fn lookup_ip(
        &self,
        py: Python<'_>,
        ip: &str,
        min_peer_count: u32,
        mode: &str,
    ) -> PyResult<PyObject> {
        let result = lookup_ip_impl(
            &self.store,
            ip,
            min_peer_count,
            LookupMode::parse(mode)?,
        )?;
        compact_lookup_to_py(py, result)
    }

    /// Return longest-prefix-match results for many IP addresses.
    #[pyo3(signature = (ips, min_peer_count=10, mode="overview"))]
    #[pyo3(
        text_signature = "($self, ips, min_peer_count=10, mode=\"overview\")"
    )]
    fn lookup_ips(
        &self,
        py: Python<'_>,
        ips: Vec<String>,
        min_peer_count: u32,
        mode: &str,
    ) -> PyResult<Vec<PyObject>> {
        let mode = LookupMode::parse(mode)?;
        let mut results = Vec::with_capacity(ips.len());
        for ip in ips {
            results.push(compact_lookup_to_py(
                py,
                lookup_ip_impl(&self.store, &ip, min_peer_count, mode)?,
            )?);
        }
        Ok(results)
    }

    /// Return source timestamp/status information for the loaded dataset.
    #[pyo3(text_signature = "($self)")]
    fn source_status(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let mut results = Vec::new();
        for entry in [
            self.timestamps.afrinic,
            self.timestamps.apnic,
            self.timestamps.arin,
            self.timestamps.lacnic,
            self.timestamps.ripencc,
            self.timestamps.riswhois,
        ]
        .iter()
        .flatten()
        {
            let obj = PyDict::new_bound(py);
            if let crate::Rir::Unknown = entry.0 {
                obj.set_item("type", "bgp")?;
            } else {
                obj.set_item("type", "rir-alloc")?;
            }
            obj.set_item("id", entry.0.to_json_id())?;
            obj.set_item("serial", entry.1)?;
            obj.set_item("last_updated", entry.2.to_rfc3339())?;
            results.push(obj.into());
        }
        Ok(results)
    }
}

fn load_lookup(
    ris_files: Vec<PathBuf>,
    prefixes_file: Option<PathBuf>,
    timestamps_dir: Option<PathBuf>,
) -> PyResult<RotoLookup> {
    if ris_files.is_empty() {
        return Err(PyValueError::new_err(
            "ris_files must contain at least one RIS Whois CSV path",
        ));
    }

    let mut store: Store = Default::default();
    if let Some(prefixes_file) = prefixes_file.as_ref() {
        store.load_prefixes(prefixes_file).map_err(|err| {
            PyRuntimeError::new_err(format!("failed to load prefixes: {}", err))
        })?;
    }

    for path in &ris_files {
        store.load_riswhois(path).map_err(|err| {
            PyRuntimeError::new_err(format!(
                "failed to load RIS Whois '{}': {}",
                path.display(),
                err
            ))
        })?;
    }

    let timestamps_dir = timestamps_dir.unwrap_or_else(|| {
        prefixes_file
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| {
                ris_files
                    .first()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
            .unwrap_or_else(|| Path::new(".").to_path_buf())
    });
    let timestamps = import_timestamps(&timestamps_dir, prefixes_file.is_some())?;

    Ok(RotoLookup { store, timestamps })
}

fn lookup_ip_impl(
    store: &Store,
    ip: &str,
    min_peer_count: u32,
    mode: LookupMode,
) -> PyResult<CompactLookup> {
    let parsed_ip = IpAddr::from_str(ip).map_err(|err| {
        PyValueError::new_err(format!("invalid IP '{}': {}", ip, err))
    })?;
    let (addr, len) = match parsed_ip {
        IpAddr::V4(addr) => (Addr::from(addr), 32),
        IpAddr::V6(addr) => (Addr::from(addr), 128),
    };

    let query_result = match addr {
        Addr::V4(_) => store.match_longest_prefix::<u32>(
            Prefix::new(addr, len),
            &default_match_options(),
        ),
        Addr::V6(_) => store.match_longest_prefix::<u128>(
            Prefix::new(addr, len),
            &default_match_options(),
        ),
    };

    let matched_prefix_value = query_result.prefix;
    let matched_prefix = matched_prefix_value.map(|prefix| prefix.to_string());
    let mut selection = select_origins(
        matched_prefix.clone(),
        query_result.prefix_meta,
        min_peer_count,
    );

    if mode == LookupMode::Overview && selection.origin_asns.is_empty() {
        if let Some(matched_prefix) = matched_prefix_value {
            for candidate_len in (0..matched_prefix.len).rev() {
                let candidate_prefix = Prefix::new(
                    network_addr(matched_prefix.addr, candidate_len),
                    candidate_len,
                );
                let fallback_query = exact_match_query(store, candidate_prefix);
                if let Some(prefix) = fallback_query.prefix {
                    let fallback = select_origins(
                        Some(prefix.to_string()),
                        fallback_query.prefix_meta,
                        min_peer_count,
                    );
                    if !fallback.origin_asns.is_empty() {
                        selection = fallback;
                        break;
                    }
                }
            }
        }
    }

    let is_less_specific = selection.prefix != matched_prefix;

    Ok(CompactLookup {
        ip: ip.to_string(),
        prefix: selection.prefix,
        matched_prefix,
        origin_asns: selection.origin_asns,
        origin_peer_counts: selection.origin_peer_counts,
        peer_count: selection.peer_count,
        is_less_specific,
        mode: mode.as_str().to_string(),
        match_type: format!("{}", query_result.match_type),
    })
}

fn exact_match_query<'a>(
    store: &'a Store,
    prefix: Prefix,
) -> crate::QueryResult<'a> {
    match prefix.addr {
        Addr::V4(_) => store.match_longest_prefix::<u32>(
            prefix,
            &MatchOptions {
                match_type: MatchType::ExactMatch,
                include_less_specifics: false,
                include_more_specifics: false,
            },
        ),
        Addr::V6(_) => store.match_longest_prefix::<u128>(
            prefix,
            &MatchOptions {
                match_type: MatchType::ExactMatch,
                include_less_specifics: false,
                include_more_specifics: false,
            },
        ),
    }
}

fn network_addr(addr: Addr, len: u8) -> Addr {
    match addr {
        Addr::V4(value) => {
            let masked = match len {
                0 => 0,
                32 => value,
                _ => value & (!0u32 << (32 - len)),
            };
            Addr::V4(masked)
        }
        Addr::V6(value) => {
            let masked = match len {
                0 => 0,
                128 => value,
                _ => value & (!0u128 << (128 - len)),
            };
            Addr::V6(masked)
        }
    }
}

struct OriginSelection {
    prefix: Option<String>,
    origin_asns: Vec<String>,
    origin_peer_counts: BTreeMap<String, u32>,
    peer_count: Option<u32>,
}

fn select_origins(
    prefix: Option<String>,
    meta: Option<&crate::ExtPrefixRecord>,
    min_peer_count: u32,
) -> OriginSelection {
    let mut origin_asns = Vec::new();
    let mut origin_peer_counts = BTreeMap::new();

    if let Some(meta) = meta {
        if let Some(ris) = &meta.1 {
            for origin in &ris.origins {
                if matches!(origin.peer_count, Some(peer_count) if peer_count < min_peer_count) {
                    continue;
                }
                let value = origin.asn.to_string();
                if !origin_asns.contains(&value) {
                    origin_asns.push(value.clone());
                }
                if let Some(peer_count) = origin.peer_count {
                    origin_peer_counts
                        .entry(value)
                        .and_modify(|current: &mut u32| {
                            *current = (*current).max(peer_count)
                        })
                        .or_insert(peer_count);
                }
            }
        }
    }

    let peer_count = origin_peer_counts.values().copied().max();

    OriginSelection {
        prefix,
        origin_asns,
        origin_peer_counts,
        peer_count,
    }
}

fn compact_lookup_to_py(
    py: Python<'_>,
    result: CompactLookup,
) -> PyResult<PyObject> {
    let obj = PyDict::new_bound(py);
    obj.set_item("ip", result.ip)?;
    obj.set_item("prefix", result.prefix)?;
    obj.set_item("matched_prefix", result.matched_prefix)?;
    obj.set_item("origin_asns", result.origin_asns)?;
    obj.set_item("origin_peer_counts", result.origin_peer_counts)?;
    obj.set_item("peer_count", result.peer_count)?;
    obj.set_item("is_less_specific", result.is_less_specific)?;
    obj.set_item("mode", result.mode)?;
    obj.set_item("match_type", result.match_type)?;
    Ok(obj.into())
}

fn default_match_options() -> MatchOptions {
    MatchOptions {
        match_type: MatchType::LongestMatch,
        include_less_specifics: true,
        include_more_specifics: true,
    }
}

fn import_timestamps(
    data_dir: &Path,
    include_delegated: bool,
) -> PyResult<TimeStamps> {
    const TIMESTAMPS_FILE_SUFFIX: &str = ".timestamps.json";
    let mut timestamps: TimeStamps = Default::default();

    for dataset in ["riswhois", "del_ext"] {
        if dataset == "del_ext" && !include_delegated {
            continue;
        }
        let path =
            data_dir.join(format!("{}{}", dataset, TIMESTAMPS_FILE_SUFFIX));
        if !path.exists() {
            continue;
        }
        let ts_file = std::fs::File::open(&path).map_err(|err| {
            PyRuntimeError::new_err(format!(
                "failed to open timestamp file '{}': {}",
                path.display(),
                err
            ))
        })?;
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(b',')
            .flexible(true)
            .trim(csv::Trim::Headers)
            .from_reader(ts_file);

        for (index, record) in rdr.records().enumerate() {
            let record = record.map_err(|err| {
                PyRuntimeError::new_err(format!(
                    "failed to parse timestamp record {} in '{}': {}",
                    index + 1,
                    path.display(),
                    err
                ))
            })?;

            let source = record.get(0).ok_or_else(|| {
                PyRuntimeError::new_err(format!(
                    "missing source identifier in '{}' record {}",
                    path.display(),
                    index + 1
                ))
            })?;
            let serial = record.get(1).ok_or_else(|| {
                PyRuntimeError::new_err(format!(
                    "missing serial in '{}' record {}",
                    path.display(),
                    index + 1
                ))
            })?;
            let last_updated = record.get(2).ok_or_else(|| {
                PyRuntimeError::new_err(format!(
                    "missing timestamp in '{}' record {}",
                    path.display(),
                    index + 1
                ))
            })?;

            timestamps
                .push(TimeStamp(
                    source.into(),
                    serial.parse::<u64>().map_err(|err| {
                        PyRuntimeError::new_err(format!(
                            "invalid serial '{}' in '{}' record {}: {}",
                            serial,
                            path.display(),
                            index + 1,
                            err
                        ))
                    })?,
                    chrono::DateTime::parse_from_rfc2822(last_updated).map_err(|err| {
                        PyRuntimeError::new_err(format!(
                            "invalid RFC2822 timestamp '{}' in '{}' record {}: {}",
                            last_updated,
                            path.display(),
                            index + 1,
                            err
                        ))
                    })?,
                ))
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
        }
    }

    Ok(timestamps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("roto-api-native-python-{}-{}", label, unique));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, name: &str, content: &str) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, content).unwrap();
            path
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn store_with_ris(content: &str) -> Store {
        let tmp = TestDir::new("lookup-mode");
        let csv_path = tmp.write("ris.csv", content);
        let mut store = Store::default();
        store.load_riswhois(&csv_path).unwrap();
        store
    }

    fn sample_ris_timestamp_csv() -> &'static str {
        "rir,file_timestamp,last_modified_header\nriswhois,123,\"Sat, 11 Apr 2026 10:03:01 GMT\"\n"
    }

    fn sample_del_timestamp_csv() -> &'static str {
        "rir,file_timestamp,last_modified_header\nafrinic,123,\"Sat, 11 Apr 2026 10:03:01 GMT\"\n"
    }

    #[test]
    fn validation_mode_keeps_filtered_prefix() {
        let store = store_with_ris(
            "151.101.0.0,16,54113,345\n151.101.0.0,22,54113,354\n151.101.2.0,23,65530,1\n",
        );

        let result = lookup_ip_impl(
            &store,
            "151.101.2.133",
            10,
            LookupMode::Validation,
        )
        .unwrap();

        assert_eq!(result.prefix.as_deref(), Some("151.101.2.0/23"));
        assert_eq!(result.matched_prefix.as_deref(), Some("151.101.2.0/23"));
        assert!(result.origin_asns.is_empty());
        assert!(!result.is_less_specific);
    }

    #[test]
    fn overview_mode_uses_first_visible_less_specific() {
        let store = store_with_ris(
            "151.101.0.0,16,54113,345\n151.101.0.0,22,54113,354\n151.101.2.0,23,65530,1\n",
        );

        let result = lookup_ip_impl(
            &store,
            "151.101.2.133",
            10,
            LookupMode::Overview,
        )
        .unwrap();

        assert_eq!(result.prefix.as_deref(), Some("151.101.0.0/22"));
        assert_eq!(result.matched_prefix.as_deref(), Some("151.101.2.0/23"));
        assert_eq!(result.origin_asns, vec!["AS54113"]);
        assert_eq!(result.peer_count, Some(354));
        assert!(result.is_less_specific);
    }

    #[test]
    fn overview_mode_keeps_exact_match_when_visible() {
        let store = store_with_ris(
            "151.101.0.0,16,54113,345\n151.101.0.0,22,54113,354\n151.101.2.0,23,65530,15\n",
        );

        let result = lookup_ip_impl(
            &store,
            "151.101.2.133",
            10,
            LookupMode::Overview,
        )
        .unwrap();

        assert_eq!(result.prefix.as_deref(), Some("151.101.2.0/23"));
        assert_eq!(result.origin_asns, vec!["AS65530"]);
        assert!(!result.is_less_specific);
    }

    #[test]
    fn overview_mode_keeps_filtered_prefix_when_no_visible_less_specific() {
        let store = store_with_ris(
            "151.101.0.0,16,54113,5\n151.101.0.0,22,54113,6\n151.101.2.0,23,65530,1\n",
        );

        let result = lookup_ip_impl(
            &store,
            "151.101.2.133",
            10,
            LookupMode::Overview,
        )
        .unwrap();

        assert_eq!(result.prefix.as_deref(), Some("151.101.2.0/23"));
        assert_eq!(result.matched_prefix.as_deref(), Some("151.101.2.0/23"));
        assert!(result.origin_asns.is_empty());
        assert!(!result.is_less_specific);
    }

    #[test]
    fn overview_mode_uses_visible_less_specific_for_ipv6() {
        let store = store_with_ris(
            "2001:db8::,32,64500,50\n2001:db8:1::,48,64501,2\n",
        );

        let result = lookup_ip_impl(
            &store,
            "2001:db8:1::1",
            10,
            LookupMode::Overview,
        )
        .unwrap();

        assert_eq!(result.prefix.as_deref(), Some("2001:db8::/32"));
        assert_eq!(result.matched_prefix.as_deref(), Some("2001:db8:1::/48"));
        assert_eq!(result.origin_asns, vec!["AS64500"]);
        assert_eq!(result.peer_count, Some(50));
        assert!(result.is_less_specific);
    }

    #[test]
    fn load_lookup_ignores_delegated_timestamps_when_not_enabled() {
        let tmp = TestDir::new("timestamps-ris-only");
        let ris_path = tmp.write("ris.csv", "8.8.8.0,24,15169,376\n");
        tmp.write("riswhois.timestamps.json", sample_ris_timestamp_csv());
        tmp.write("del_ext.timestamps.json", sample_del_timestamp_csv());

        let lookup = load_lookup(
            vec![ris_path],
            None,
            Some(tmp.path().to_path_buf()),
        )
        .unwrap();

        assert!(lookup.timestamps.afrinic.is_none());
        assert!(lookup.timestamps.riswhois.is_some());
    }

    #[test]
    fn load_lookup_includes_delegated_timestamps_when_enabled() {
        let tmp = TestDir::new("timestamps-with-delegated");
        let ris_path = tmp.write("ris.csv", "8.8.8.0,24,15169,376\n");
        let delegated_path = tmp.write(
            "delegated_all.csv",
            "arin|US|ipv4|8.8.8.0|256|20240410|allocated|google\n",
        );
        tmp.write("riswhois.timestamps.json", sample_ris_timestamp_csv());
        tmp.write("del_ext.timestamps.json", sample_del_timestamp_csv());

        let lookup = load_lookup(
            vec![ris_path],
            Some(delegated_path),
            Some(tmp.path().to_path_buf()),
        )
        .unwrap();

        assert!(lookup.timestamps.afrinic.is_some());
        assert!(lookup.timestamps.riswhois.is_some());
    }
}

#[pymodule]
fn _native(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<RotoLookup>()?;
    Ok(())
}
