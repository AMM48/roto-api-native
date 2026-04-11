use crate::{
    Addr, MatchOptions, MatchType, Prefix, Store, TimeStamp, TimeStamps,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

struct CompactLookup {
    ip: String,
    prefix: Option<String>,
    origin_asns: Vec<String>,
    match_type: String,
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
    #[pyo3(signature = (prefixes_file, ris_files, timestamps_dir=None))]
    #[pyo3(
        text_signature = "(prefixes_file, ris_files, timestamps_dir=None)"
    )]
    fn new(
        prefixes_file: String,
        ris_files: Vec<String>,
        timestamps_dir: Option<String>,
    ) -> PyResult<Self> {
        load_lookup(
            Path::new(&prefixes_file),
            ris_files.iter().map(PathBuf::from).collect(),
            timestamps_dir.map(PathBuf::from),
        )
    }

    /// Build a lookup object from a generated data directory.
    #[staticmethod]
    #[pyo3(signature = (data_dir))]
    #[pyo3(text_signature = "(data_dir)")]
    fn from_data_dir(data_dir: String) -> PyResult<Self> {
        let data_dir = PathBuf::from(data_dir);
        let ris_files = ["pfx_asn_dfz_v4.csv", "pfx_asn_dfz_v6.csv"]
            .iter()
            .map(|name| data_dir.join(name))
            .filter(|path| path.exists())
            .collect();
        load_lookup(
            &data_dir.join("delegated_all.csv"),
            ris_files,
            Some(data_dir),
        )
    }

    /// Return the longest-prefix-match result for a single IP address.
    #[pyo3(text_signature = "($self, ip)")]
    fn lookup_ip(&self, py: Python<'_>, ip: &str) -> PyResult<PyObject> {
        let result = lookup_ip_impl(&self.store, ip)?;
        compact_lookup_to_py(py, result)
    }

    /// Return longest-prefix-match results for many IP addresses.
    #[pyo3(text_signature = "($self, ips)")]
    fn lookup_ips(
        &self,
        py: Python<'_>,
        ips: Vec<String>,
    ) -> PyResult<Vec<PyObject>> {
        let mut results = Vec::with_capacity(ips.len());
        for ip in ips {
            results.push(compact_lookup_to_py(
                py,
                lookup_ip_impl(&self.store, &ip)?,
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
    prefixes_file: &Path,
    ris_files: Vec<PathBuf>,
    timestamps_dir: Option<PathBuf>,
) -> PyResult<RotoLookup> {
    if ris_files.is_empty() {
        return Err(PyValueError::new_err(
            "ris_files must contain at least one RIS Whois CSV path",
        ));
    }

    let mut store: Store = Default::default();
    store.load_prefixes(prefixes_file).map_err(|err| {
        PyRuntimeError::new_err(format!("failed to load prefixes: {}", err))
    })?;

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
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    });
    let timestamps = import_timestamps(&timestamps_dir)?;

    Ok(RotoLookup { store, timestamps })
}

fn lookup_ip_impl(store: &Store, ip: &str) -> PyResult<CompactLookup> {
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

    let prefix = query_result.prefix.map(|prefix| prefix.to_string());
    let mut origin_asns = Vec::new();
    if let Some(meta) = query_result.prefix_meta {
        if let Some(ris) = &meta.1 {
            for asn in &ris.origin_asns.0 {
                let value = asn.to_string();
                if !origin_asns.contains(&value) {
                    origin_asns.push(value);
                }
            }
        }
    }

    Ok(CompactLookup {
        ip: ip.to_string(),
        prefix,
        origin_asns,
        match_type: format!("{}", query_result.match_type),
    })
}

fn compact_lookup_to_py(
    py: Python<'_>,
    result: CompactLookup,
) -> PyResult<PyObject> {
    let obj = PyDict::new_bound(py);
    obj.set_item("ip", result.ip)?;
    obj.set_item("prefix", result.prefix)?;
    obj.set_item("origin_asns", result.origin_asns)?;
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

fn import_timestamps(data_dir: &Path) -> PyResult<TimeStamps> {
    const TIMESTAMPS_FILE_SUFFIX: &str = ".timestamps.json";
    let mut timestamps: TimeStamps = Default::default();

    for dataset in ["del_ext", "riswhois"] {
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

#[pymodule]
fn _native(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<RotoLookup>()?;
    Ok(())
}
