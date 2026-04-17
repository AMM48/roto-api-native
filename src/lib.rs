use rotonda_store::common::{
    AddressFamily, MergeUpdate, Prefix as RotondaPrefix,
};
pub use rotonda_store::{InMemStorage, MatchOptions, MatchType, TreeBitMap};
use std::error::Error;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::str::FromStr;
use std::fmt;

//------------ Addr ----------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum Addr {
    V4(u32),
    V6(u128),
}

impl From<Ipv4Addr> for Addr {
    fn from(addr: Ipv4Addr) -> Self {
        Self::V4(addr.into())
    }
}

impl From<Ipv6Addr> for Addr {
    fn from(addr: Ipv6Addr) -> Self {
        Self::V6(addr.into())
    }
}

impl From<IpAddr> for Addr {
    fn from(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(addr) => addr.into(),
            IpAddr::V6(addr) => addr.into(),
        }
    }
}
impl From<u32> for Addr {
    fn from(addr: u32) -> Self {
        Self::V4(addr)
    }
}

impl FromStr for Addr {
    type Err = <IpAddr as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpAddr::from_str(s).map(Into::into)
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Addr::V4(addr) => {
                write!(f, "{}", std::net::Ipv4Addr::from(*addr))
            }
            Addr::V6(addr) => {
                write!(f, "{}", std::net::Ipv6Addr::from(*addr))
            }
        }
    }
}

//------------ Prefix --------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Prefix {
    pub addr: Addr,
    pub len: u8,
}

impl Prefix {
    pub fn new(addr: Addr, len: u8) -> Self {
        Prefix { addr, len }
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.len)
    }
}

fn invalid_data_error(message: impl Into<String>) -> Box<dyn Error> {
    io::Error::new(ErrorKind::InvalidData, message.into()).into()
}

fn record_field<'a>(
    record: &'a csv::StringRecord,
    field_index: usize,
    path: &Path,
    record_index: usize,
    dataset: &str,
) -> Result<&'a str, Box<dyn Error>> {
    record.get(field_index).ok_or_else(|| {
        invalid_data_error(format!(
            "missing field {} in {} record {} from '{}'",
            field_index,
            dataset,
            record_index,
            path.display()
        ))
    })
}

fn decompose_ipv4_allocation(
    net: Ipv4Addr,
    addresses: u32,
    path: &Path,
    record_index: usize,
) -> Result<Vec<(u32, u8)>, Box<dyn Error>> {
    if addresses == 0 {
        return Err(invalid_data_error(format!(
            "invalid IPv4 allocation size 0 in delegated record {} from '{}'",
            record_index,
            path.display()
        )));
    }

    let start = u32::from(net) as u64;
    let mut current = start;
    let mut remaining = addresses as u64;
    let end_exclusive = start + remaining;

    if end_exclusive > (u32::MAX as u64) + 1 {
        return Err(invalid_data_error(format!(
            "IPv4 allocation {} with size {} exceeds the IPv4 address space in delegated record {} from '{}'",
            net,
            addresses,
            record_index,
            path.display()
        )));
    }

    let mut prefixes = Vec::new();
    while remaining > 0 {
        let max_by_alignment = if current == 0 {
            1_u64 << 32
        } else {
            1_u64 << (current.trailing_zeros())
        };
        let max_by_remaining = 1_u64 << (63 - remaining.leading_zeros());
        let block_size = max_by_alignment.min(max_by_remaining);
        let prefix_len = 32 - block_size.trailing_zeros() as u8;

        prefixes.push((current as u32, prefix_len));
        current += block_size;
        remaining -= block_size;
    }

    Ok(prefixes)
}

//--------------------- Query Results ---------------------------------------------

#[derive(Clone, Debug)]
pub struct QueryResult<'a> {
    pub match_type: MatchType,
    pub prefix: Option<Prefix>,
    pub prefix_meta: Option<&'a ExtPrefixRecord>,
    pub less_specifics: RecordSet<'a>,
    pub more_specifics: RecordSet<'a>,
}

impl<'a>
    From<rotonda_store::QueryResult<'a, InMemStorage<u32, ExtPrefixRecord>>>
    for QueryResult<'a>
{
    fn from(
        result: rotonda_store::QueryResult<
            'a,
            InMemStorage<u32, ExtPrefixRecord>,
        >,
    ) -> QueryResult<'a> {
        match result.prefix {
            Some(prefix) => match prefix.net.into_ipaddr() {
                std::net::IpAddr::V4(net) => QueryResult {
                    match_type: result.match_type,
                    prefix: result.prefix.map(|pfx| Prefix {
                        addr: Addr::from(net),
                        len: pfx.len,
                    }),
                    prefix_meta: if let Some(pfx) = result.prefix {
                        pfx.meta.as_ref()
                    } else {
                        None
                    },
                    less_specifics: RecordSet::from(result.less_specifics),
                    more_specifics: RecordSet::from(result.more_specifics),
                },
                std::net::IpAddr::V6(net) => QueryResult {
                    match_type: result.match_type,
                    prefix: result.prefix.map(|pfx| Prefix {
                        addr: Addr::from(net),
                        len: pfx.len,
                    }),
                    prefix_meta: if let Some(pfx) = result.prefix {
                        pfx.meta.as_ref()
                    } else {
                        None
                    },
                    less_specifics: RecordSet::from(result.less_specifics),
                    more_specifics: RecordSet::from(result.more_specifics),
                },
            },
            None => QueryResult {
                match_type: MatchType::EmptyMatch,
                prefix: None,
                prefix_meta: None,
                less_specifics: RecordSet::from(result.less_specifics),
                more_specifics: RecordSet::from(result.more_specifics),
            },
        }
    }
}

impl<'a>
    From<rotonda_store::QueryResult<'a, InMemStorage<u128, ExtPrefixRecord>>>
    for QueryResult<'a>
{
    fn from(
        result: rotonda_store::QueryResult<
            'a,
            InMemStorage<u128, ExtPrefixRecord>,
        >,
    ) -> QueryResult<'a> {
        match result.prefix {
            Some(prefix) => match prefix.net.into_ipaddr() {
                std::net::IpAddr::V4(net) => QueryResult {
                    match_type: result.match_type,
                    prefix: result.prefix.map(|pfx| Prefix {
                        addr: Addr::from(net),
                        len: pfx.len,
                    }),
                    prefix_meta: if let Some(pfx) = result.prefix {
                        pfx.meta.as_ref()
                    } else {
                        None
                    },
                    less_specifics: RecordSet::from(result.less_specifics),
                    more_specifics: RecordSet::from(result.more_specifics),
                },
                std::net::IpAddr::V6(net) => QueryResult {
                    match_type: result.match_type,
                    prefix: result.prefix.map(|pfx| Prefix {
                        addr: Addr::from(net),
                        len: pfx.len,
                    }),
                    prefix_meta: if let Some(pfx) = result.prefix {
                        pfx.meta.as_ref()
                    } else {
                        None
                    },
                    less_specifics: RecordSet::from(result.less_specifics),
                    more_specifics: RecordSet::from(result.more_specifics),
                },
            },
            None => QueryResult {
                match_type: MatchType::EmptyMatch,
                prefix: None,
                prefix_meta: None,
                less_specifics: RecordSet::from(result.less_specifics),
                more_specifics: RecordSet::from(result.more_specifics),
            },
        }
    }
}

impl<'a> From<Option<Vec<&'a RotondaPrefix<u32, ExtPrefixRecord>>>>
    for RecordSet<'a>
{
    fn from(
        result: Option<Vec<&'a RotondaPrefix<u32, ExtPrefixRecord>>>,
    ) -> Self {
        RecordSet {
            v4: result.unwrap_or_default(),
            v6: Vec::new(),
        }
    }
}

impl<'a> From<Option<Vec<&'a RotondaPrefix<u128, ExtPrefixRecord>>>>
    for RecordSet<'a>
{
    fn from(
        result: Option<Vec<&'a RotondaPrefix<u128, ExtPrefixRecord>>>,
    ) -> Self {
        RecordSet {
            v6: result.unwrap_or_default(),
            v4: Vec::new(),
        }
    }
}

//------------ RecordSet -----------------------------------------------------

#[derive(Clone, Debug)]
pub struct RecordSet<'a> {
    v4: Vec<&'a RotondaPrefix<u32, ExtPrefixRecord>>,
    v6: Vec<&'a RotondaPrefix<u128, ExtPrefixRecord>>,
}

impl<'a> RecordSet<'a> {
    pub fn is_empty(&self) -> bool {
        self.v4.is_empty() && self.v6.is_empty()
    }
}

//------------ Rir -----------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum Rir {
    Afrinic,
    Apnic,
    Arin,
    Lacnic,
    RipeNcc,
    Unknown,
}

impl From<&str> for Rir {
    fn from(str: &str) -> Self {
        match str {
            "afrinic" => Self::Afrinic,
            "apnic" => Self::Apnic,
            "arin" => Self::Arin,
            "lacnic" => Self::Lacnic,
            "ripencc" => Self::RipeNcc,
            _ => Self::Unknown,
        }
    }
}

impl Rir {
    pub fn to_json_id(self) -> String {
        match self {
            Rir::Afrinic => "afrinic".to_string(),
            Rir::Apnic => "apnic".to_string(),
            Rir::Arin => "arin".to_string(),
            Rir::Lacnic => "lacnic".to_string(),
            Rir::RipeNcc => "ripe".to_string(),
            Rir::Unknown => "riswhois".to_string(),
        }
    }
}

impl fmt::Display for Rir {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Rir::Afrinic => write!(f, "AFRINIC"),
            Rir::Apnic => write!(f, "APNIC"),
            Rir::Arin => write!(f, "ARIN"),
            Rir::Lacnic => write!(f, "LACNIC"),
            Rir::RipeNcc => write!(f, "RIPE NCC"),
            Rir::Unknown => write!(f, "Unknown"),
        }
    }
}

//------------ ExtPrefixRecord -----------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ExtPrefixRecord(
    pub Option<RirDelExtRecord>,
    pub Option<RisWhoisRecord>,
);

impl MergeUpdate for ExtPrefixRecord {
    fn merge_update(
        &mut self,
        update_record: ExtPrefixRecord,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if update_record.0.is_some() {
            self.0 = update_record.0
        }

        if update_record.1.is_some() {
            match &mut self.1 {
                Some(ris_whois_rec) => {
                    if let Some(update_ris_rec) = update_record.1 {
                        ris_whois_rec.origins.extend(update_ris_rec.origins);
                    }
                }
                None => {
                    self.1 = update_record.1;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RirDelExtRecord {
    pub rir: Rir,
}

#[derive(Clone, Debug)]
pub struct RisWhoisRecord {
    pub origins: Vec<RisOrigin>,
}

#[derive(Clone, Debug)]
pub struct RisOrigin {
    pub asn: Asn,
    pub peer_count: Option<u32>,
}

#[derive(Copy, Clone, Debug)]
pub struct Asn(u32);

impl fmt::Display for Asn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AS{}", self.0)
    }
}

impl FromStr for Asn {
    fn from_str(
        as_str: &str,
    ) -> std::result::Result<Asn, std::num::ParseIntError> {
        as_str.parse::<u32>().map(Asn)
    }

    type Err = std::num::ParseIntError;
}

// ----------- TimeStamp & TimeStamps ------------------------------------------------

#[derive(Copy, Clone, Debug)]
pub struct TimeStamp(
    pub Rir,
    pub u64,
    pub chrono::DateTime<chrono::FixedOffset>,
);

impl fmt::Display for TimeStamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "\"{}\": \"{} {}\"", self.0, self.1, self.2)
    }
}

#[allow(clippy::inherent_to_string_shadow_display)]
impl TimeStamp {
    pub fn to_string(self) -> String {
        format!("{} {} {}", self.0, self.1, self.2)
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct TimeStamps {
    pub afrinic: Option<TimeStamp>,
    pub apnic: Option<TimeStamp>,
    pub arin: Option<TimeStamp>,
    pub lacnic: Option<TimeStamp>,
    pub ripencc: Option<TimeStamp>,
    pub riswhois: Option<TimeStamp>,
}

impl TimeStamps {
    pub fn push(
        &mut self,
        ts: TimeStamp,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match ts.0 {
            Rir::Afrinic => {
                self.afrinic = Some(ts);
            }
            Rir::Apnic => {
                self.apnic = Some(ts);
            }
            Rir::Arin => {
                self.arin = Some(ts);
            }
            Rir::Lacnic => {
                self.lacnic = Some(ts);
            }
            Rir::RipeNcc => {
                self.ripencc = Some(ts);
            }
            Rir::Unknown => {
                self.riswhois = Some(ts);
            }
        }
        Ok(())
    }

}

//------------ Store ---------------------------------------------------------

pub struct Store {
    v4: TreeBitMap<InMemStorage<u32, ExtPrefixRecord>>,
    v6: TreeBitMap<InMemStorage<u128, ExtPrefixRecord>>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            v4: TreeBitMap::new(vec![4]),
            v6: TreeBitMap::new(vec![4]),
        }
    }
}

impl Store {
    pub fn load_riswhois(
        &mut self,
        path: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let file = File::open(path)?;
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(file);
        for (index, result) in rdr.records().enumerate() {
            let record = result?;
            let record_index = index + 1;
            let addr_str =
                record_field(&record, 0, path, record_index, "RIS Whois")?;
            let len_str =
                record_field(&record, 1, path, record_index, "RIS Whois")?;
            let asn_str =
                record_field(&record, 2, path, record_index, "RIS Whois")?;
            let peer_count = record
                .get(3)
                .filter(|value| !value.is_empty())
                .map(|value| {
                    value.parse::<u32>().map_err(|err| {
                        invalid_data_error(format!(
                            "invalid RIS peer count '{}' in record {} from '{}': {}",
                            value,
                            record_index,
                            path.display(),
                            err
                        ))
                    })
                })
                .transpose()?;

            let net = Addr::from_str(addr_str).map_err(|err| {
                invalid_data_error(format!(
                    "invalid RIS prefix '{}' in record {} from '{}': {}",
                    addr_str,
                    record_index,
                    path.display(),
                    err
                ))
            })?;
            let len = u8::from_str(len_str).map_err(|err| {
                invalid_data_error(format!(
                    "invalid RIS prefix length '{}' in record {} from '{}': {}",
                    len_str,
                    record_index,
                    path.display(),
                    err
                ))
            })?;
            let asn: Asn = Asn::from_str(asn_str).map_err(|err| {
                invalid_data_error(format!(
                    "invalid RIS ASN '{}' in record {} from '{}': {}",
                    asn_str,
                    record_index,
                    path.display(),
                    err
                ))
            })?;

            match net {
                Addr::V4(_) if len > 32 => {
                    return Err(invalid_data_error(format!(
                        "IPv4 RIS prefix length {} exceeds /32 in record {} from '{}'",
                        len,
                        record_index,
                        path.display()
                    )))
                }
                Addr::V6(_) if len > 128 => {
                    return Err(invalid_data_error(format!(
                        "IPv6 RIS prefix length {} exceeds /128 in record {} from '{}'",
                        len,
                        record_index,
                        path.display()
                    )))
                }
                _ => {}
            }

            let meta = ExtPrefixRecord(
                None,
                Some(RisWhoisRecord {
                    origins: vec![RisOrigin { asn, peer_count }],
                }),
            );

            match net {
                Addr::V4(net) => {
                    self.v4
                        .insert(RotondaPrefix::new_with_meta(net, len, meta))
                        .map_err(|err| {
                            invalid_data_error(format!(
                            "failed to insert RIS prefix {}/{} from '{}': {}",
                            addr_str,
                            len,
                            path.display(),
                            err
                        ))
                        })?;
                }
                Addr::V6(net) => {
                    self.v6
                        .insert(RotondaPrefix::new_with_meta(net, len, meta))
                        .map_err(|err| {
                            invalid_data_error(format!(
                            "failed to insert RIS prefix {}/{} from '{}': {}",
                            addr_str,
                            len,
                            path.display(),
                            err
                        ))
                        })?;
                }
            }
        }
        Ok(())
    }

    pub fn load_prefixes(
        &mut self,
        path: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let file = File::open(path)?;
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(b'|')
            .has_headers(false)
            .flexible(true)
            .trim(csv::Trim::Headers)
            .from_reader(file);

        for (index, record) in rdr.records().enumerate() {
            let record = record?;
            let record_index = index + 1;

            let Some(rir) = record.get(0) else {
                continue;
            };
            if rir.starts_with('#') {
                continue;
            }

            let Some(resource_type) = record.get(2) else {
                continue;
            };
            if resource_type != "ipv4" && resource_type != "ipv6" {
                continue;
            }

            let date_field = record.get(5).unwrap_or_default();
            let status = record.get(6).unwrap_or_default();
            if date_field == "summary"
                || status == "reserved"
                || status == "available"
            {
                continue;
            }

            if !matches!(record.get(7), Some(id) if !id.is_empty()) {
                continue;
            }

            let meta = ExtPrefixRecord(
                Some(RirDelExtRecord {
                    rir: rir.into(),
                }),
                None,
            );

            match resource_type {
                "ipv4" => {
                    let net_str = record_field(
                        &record,
                        3,
                        path,
                        record_index,
                        "delegated IPv4",
                    )?;
                    let len_base_str = record_field(
                        &record,
                        4,
                        path,
                        record_index,
                        "delegated IPv4",
                    )?;
                    let net = Ipv4Addr::from_str(net_str).map_err(|err| {
                        invalid_data_error(format!(
                            "invalid delegated IPv4 network '{}' in record {} from '{}': {}",
                            net_str,
                            record_index,
                            path.display(),
                            err
                        ))
                    })?;

                    // record[4] is the number of addresses in the allocation.
                    let len_base = u32::from_str(len_base_str).map_err(|err| {
                        invalid_data_error(format!(
                            "invalid delegated IPv4 allocation size '{}' in record {} from '{}': {}",
                            len_base_str,
                            record_index,
                            path.display(),
                            err
                        ))
                    })?;
                    let prefixes = decompose_ipv4_allocation(
                        net,
                        len_base,
                        path,
                        record_index,
                    )?;
                    for (prefix_net, prefix_len) in prefixes {
                        self.v4.insert(RotondaPrefix::new_with_meta(
                            prefix_net,
                            prefix_len,
                            meta.clone(),
                        ))?;
                    }
                }
                "ipv6" => {
                    let net_str = record_field(
                        &record,
                        3,
                        path,
                        record_index,
                        "delegated IPv6",
                    )?;
                    let len_str = record_field(
                        &record,
                        4,
                        path,
                        record_index,
                        "delegated IPv6",
                    )?;
                    let net = Ipv6Addr::from_str(net_str).map_err(|err| {
                        invalid_data_error(format!(
                            "invalid delegated IPv6 network '{}' in record {} from '{}': {}",
                            net_str,
                            record_index,
                            path.display(),
                            err
                        ))
                    })?;

                    // record[4] is just the prefix length here. No shenanigans
                    // necessary.
                    let len = u8::from_str(len_str).map_err(|err| {
                        invalid_data_error(format!(
                            "invalid delegated IPv6 prefix length '{}' in record {} from '{}': {}",
                            len_str,
                            record_index,
                            path.display(),
                            err
                        ))
                    })?;
                    if len > 128 {
                        return Err(invalid_data_error(format!(
                            "delegated IPv6 prefix length {} exceeds /128 in record {} from '{}'",
                            len,
                            record_index,
                            path.display()
                        )));
                    }

                    self.v6.insert(RotondaPrefix::new_with_meta(
                        net.into(),
                        len,
                        meta,
                    ))?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn match_longest_prefix<AF: AddressFamily>(
        &self,
        prefix: Prefix,
        match_options: &MatchOptions,
    ) -> QueryResult<'_> {
        match prefix.addr {
            Addr::V4(addr) => self
                .v4
                .match_prefix(
                    &RotondaPrefix::new(addr, prefix.len),
                    match_options,
                )
                .into(),
            Addr::V6(addr) => self
                .v6
                .match_prefix(
                    &RotondaPrefix::new(addr, prefix.len),
                    match_options,
                )
                .into(),
        }
    }

}

mod python;

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
                .join(format!("roto-api-native-{}-{}", label, unique));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, name: &str, content: &str) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, content).unwrap();
            path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn default_options() -> MatchOptions {
        MatchOptions {
            match_type: MatchType::LongestMatch,
            include_less_specifics: true,
            include_more_specifics: true,
        }
    }

    fn lookup<'a>(store: &'a Store, ip: &str) -> QueryResult<'a> {
        let addr = Addr::from_str(ip).unwrap();
        let len = match addr {
            Addr::V4(_) => 32,
            Addr::V6(_) => 128,
        };

        match addr {
            Addr::V4(_) => store.match_longest_prefix::<u32>(
                Prefix::new(addr, len),
                &default_options(),
            ),
            Addr::V6(_) => store.match_longest_prefix::<u128>(
                Prefix::new(addr, len),
                &default_options(),
            ),
        }
    }

    fn origin_asns(query: &QueryResult<'_>) -> Vec<String> {
        query
            .prefix_meta
            .and_then(|meta| meta.1.as_ref())
            .map(|record| {
                record
                    .origins
                    .iter()
                    .map(|origin| origin.asn.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn exact_match_without_specific_sets_does_not_panic() {
        let tmp = TestDir::new("exact-no-specifics");
        let csv_path = tmp.write("ris.csv", "8.8.8.0,24,15169,376\n");
        let mut store = Store::default();
        store.load_riswhois(&csv_path).unwrap();

        let result = store.match_longest_prefix::<u32>(
            Prefix::new(Addr::from_str("8.8.8.0").unwrap(), 24),
            &MatchOptions {
                match_type: MatchType::ExactMatch,
                include_less_specifics: false,
                include_more_specifics: false,
            },
        );

        assert_eq!(result.prefix.unwrap().to_string(), "8.8.8.0/24");
        assert!(result.less_specifics.is_empty());
        assert!(result.more_specifics.is_empty());
    }

    fn max_peer_count(query: &QueryResult<'_>) -> Option<u32> {
        query
            .prefix_meta
            .and_then(|meta| meta.1.as_ref())
            .and_then(|record| {
                record
                    .origins
                    .iter()
                    .filter_map(|origin| origin.peer_count)
                    .max()
            })
    }

    #[test]
    fn load_riswhois_keeps_first_headerless_row() {
        let tmp = TestDir::new("ris-headerless");
        let csv_path =
            tmp.write("ris.csv", "8.8.8.0,24,15169,376\n1.1.1.0,24,13335,211\n");
        let mut store = Store::default();

        store.load_riswhois(&csv_path).unwrap();

        assert_eq!(
            lookup(&store, "8.8.8.8").prefix.unwrap().to_string(),
            "8.8.8.0/24"
        );
        assert_eq!(origin_asns(&lookup(&store, "8.8.8.8")), vec!["AS15169"]);
        assert_eq!(max_peer_count(&lookup(&store, "8.8.8.8")), Some(376));
        assert_eq!(origin_asns(&lookup(&store, "1.1.1.1")), vec!["AS13335"]);
        assert_eq!(max_peer_count(&lookup(&store, "1.1.1.1")), Some(211));
    }

    #[test]
    fn load_riswhois_accepts_legacy_three_column_rows() {
        let tmp = TestDir::new("ris-legacy");
        let csv_path = tmp.write("ris.csv", "8.8.8.0,24,15169\n");
        let mut store = Store::default();

        store.load_riswhois(&csv_path).unwrap();

        assert_eq!(origin_asns(&lookup(&store, "8.8.8.8")), vec!["AS15169"]);
        assert_eq!(max_peer_count(&lookup(&store, "8.8.8.8")), None);
    }

    #[test]
    fn load_riswhois_returns_error_on_invalid_row() {
        let tmp = TestDir::new("ris-invalid");
        let csv_path = tmp.write("ris.csv", "bad-ip,24,15169\n");
        let mut store = Store::default();

        let err = store.load_riswhois(&csv_path).unwrap_err();

        assert!(
            err.to_string().contains("invalid RIS prefix 'bad-ip'"),
            "{}",
            err
        );
    }

    #[test]
    fn load_prefixes_accepts_headerless_first_record() {
        let tmp = TestDir::new("delegated-headerless");
        let csv_path = tmp.write(
            "delegated.csv",
            "arin|US|ipv4|8.8.8.0|256|20240410|allocated|google\n",
        );
        let mut store = Store::default();

        store.load_prefixes(&csv_path).unwrap();

        assert_eq!(
            lookup(&store, "8.8.8.8").prefix.unwrap().to_string(),
            "8.8.8.0/24"
        );
    }

    #[test]
    fn load_prefixes_splits_non_cidr_ipv4_allocations() {
        let tmp = TestDir::new("delegated-split");
        let csv_path = tmp.write(
            "delegated.csv",
            "arin|US|ipv4|8.8.8.0|384|20240410|allocated|google\n",
        );
        let mut store = Store::default();

        store.load_prefixes(&csv_path).unwrap();

        assert_eq!(
            lookup(&store, "8.8.8.8").prefix.unwrap().to_string(),
            "8.8.8.0/24"
        );
        assert_eq!(
            lookup(&store, "8.8.9.42").prefix.unwrap().to_string(),
            "8.8.9.0/25"
        );
        assert!(lookup(&store, "8.8.9.200").prefix.is_none());
    }
}
