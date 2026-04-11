include!(concat!(env!("OUT_DIR"), "/version.rs"));

use ansi_term::Colour;
use chrono::{DateTime, Utc};
use num::PrimInt;
use rotonda_store::common::{
    AddressFamily, MergeUpdate, Prefix as RotondaPrefix,
};
pub use rotonda_store::{
    InMemNodeId, InMemStorage, MatchOptions, MatchType, SizedStrideNode,
    TreeBitMap,
};
use std::error::Error;
use std::fmt::Write;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::str::FromStr;
use std::{fmt, slice};

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
        addr.into()
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
                std::net::IpAddr::V4(net) => {
                    return QueryResult {
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
                        less_specifics: RecordSet::from(
                            result.less_specifics.unwrap(),
                        ),
                        more_specifics: RecordSet::from(
                            result.more_specifics.unwrap(),
                        ),
                    }
                }
                std::net::IpAddr::V6(net) => {
                    return QueryResult {
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
                        less_specifics: RecordSet::from(
                            result.less_specifics.unwrap(),
                        ),
                        more_specifics: RecordSet::from(
                            result.more_specifics.unwrap(),
                        ),
                    }
                }
            },
            None => QueryResult {
                match_type: MatchType::EmptyMatch,
                prefix: None,
                prefix_meta: None,
                less_specifics: RecordSet::from(
                    result.less_specifics.unwrap(),
                ),
                more_specifics: RecordSet::from(
                    result.more_specifics.unwrap(),
                ),
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
                std::net::IpAddr::V4(net) => {
                    return QueryResult {
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
                        less_specifics: RecordSet::from(
                            result.less_specifics.unwrap(),
                        ),
                        more_specifics: RecordSet::from(
                            result.more_specifics.unwrap(),
                        ),
                    }
                }
                std::net::IpAddr::V6(net) => {
                    return QueryResult {
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
                        less_specifics: RecordSet::from(
                            result.less_specifics.unwrap(),
                        ),
                        more_specifics: RecordSet::from(
                            result.more_specifics.unwrap(),
                        ),
                    }
                }
            },
            None => QueryResult {
                match_type: MatchType::EmptyMatch,
                prefix: None,
                prefix_meta: None,
                less_specifics: RecordSet::from(
                    result.less_specifics.unwrap(),
                ),
                more_specifics: RecordSet::from(
                    result.more_specifics.unwrap(),
                ),
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
            v4: result.unwrap(),
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
            v6: result.unwrap(),
            v4: Vec::new(),
        }
    }
}

// -------------- AsnQueryResult ---------------------------------------------

#[derive(Clone, Debug)]
pub struct AsnQueryResult<'a> {
    pub prefixes: RecordSet<'a>,
}

pub enum SearchType {
    PrefixesByBgpAsn,
}

impl fmt::Display for SearchType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "by-asns")
    }
}

pub struct SearchByAsnOptions {
    pub search_type: SearchType,
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

    pub fn iter(&self) -> RecordSetIter<'_, '_> {
        RecordSetIter {
            v4: if self.v4.is_empty() {
                None
            } else {
                Some(self.v4.iter())
            },
            v6: self.v6.iter(),
        }
    }

    pub fn reverse(mut self) -> RecordSet<'a> {
        self.v4.reverse();
        self.v6.reverse();
        self
    }
}

impl<'a> From<Vec<&'a RotondaPrefix<u32, ExtPrefixRecord>>>
    for RecordSet<'a>
{
    fn from(vec: Vec<&'a RotondaPrefix<u32, ExtPrefixRecord>>) -> Self {
        Self {
            v4: vec,
            v6: vec![],
        }
    }
}

impl<'a> From<Vec<&'a RotondaPrefix<u128, ExtPrefixRecord>>>
    for RecordSet<'a>
{
    fn from(vec: Vec<&'a RotondaPrefix<u128, ExtPrefixRecord>>) -> Self {
        Self {
            v4: vec![],
            v6: vec,
        }
    }
}

//------------ RecordSetIter -------------------------------------------------

#[derive(Clone, Debug)]
pub struct RecordSetIter<'a, 'b> {
    v4: Option<slice::Iter<'a, &'b RotondaPrefix<u32, ExtPrefixRecord>>>,
    v6: slice::Iter<'a, &'b RotondaPrefix<u128, ExtPrefixRecord>>,
}

impl<'a, 'b> Iterator for RecordSetIter<'a, 'b> {
    type Item = (Prefix, Option<&'b ExtPrefixRecord>);

    fn next(&mut self) -> Option<Self::Item> {
        // V4 is already done.
        if self.v4.is_none() {
            return self.v6.next().map(|res| {
                (
                    Prefix {
                        addr: Addr::V6(res.net),
                        len: res.len,
                    },
                    res.meta.as_ref(),
                )
            });
        }

        if let Some(res) = self.v4.as_mut().and_then(|v4| v4.next()) {
            return Some((
                Prefix {
                    addr: Addr::V4(res.net),
                    len: res.len,
                },
                res.meta.as_ref(),
            ));
        }
        self.v4 = None;
        self.next()
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

impl<'a> From<&'a str> for Rir {
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

impl<'a> fmt::Display for Rir {
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
                        ris_whois_rec
                            .origin_asns
                            .0
                            .push(update_ris_rec.origin_asns.0[0]);
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
    group_id: String,
    pub rir: Rir,
}

// Not really used right now, since the
// impl Display isn't used either. May make sense
// to redefine Asn to be an enum that can either
// be a u32 or a PRIVATE_ASN.
#[derive(Clone, Debug)]
pub struct AsnArray(pub Vec<Asn>);

impl fmt::Display for AsnArray {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let arr_str = self.0.iter().fold("".to_string(), |as_arr, asn| {
            let asn_str: &str = &asn.to_string();
            as_arr + "AS" + asn_str
        });
        write!(f, "{}", arr_str)
    }
}
#[derive(Clone, Debug)]
pub struct RisWhoisRecord {
    pub origin_asns: AsnArray,
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

    pub fn to_json_builder(self, builder: &mut JsonBuilder) {
        builder.member_array("sources", |builder| {
            for rir in [
                self.afrinic,
                self.apnic,
                self.arin,
                self.lacnic,
                self.ripencc,
                self.riswhois,
            ]
            .iter()
            .flatten()
            {
                builder.array_object(|builder| {
                    // RisWhois dataset has Rir::Unknown set
                    if let Rir::Unknown = rir.0 {
                        builder.member_str("type", "bgp");
                    } else {
                        builder.member_str("type", "rir-alloc");
                    }
                    builder.member_str("id", rir.0.to_json_id());
                    builder.member_raw("serial", rir.1);
                    builder.member_str("lastUpdated", rir.2.format("%+"));
                })
            }
        });
    }
}

//------------ Store ---------------------------------------------------------

pub struct Store {
    v4: TreeBitMap<InMemStorage<u32, ExtPrefixRecord>>,
    v6: TreeBitMap<InMemStorage<u128, ExtPrefixRecord>>,
    updated: DateTime<Utc>,
    pub timestamps: TimeStamps,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            v4: TreeBitMap::new(vec![4]),
            v6: TreeBitMap::new(vec![4]),
            updated: Utc::now(),
            timestamps: Default::default(),
        }
    }
}

impl Store {
    pub fn updated(&self) -> DateTime<Utc> {
        self.updated
    }

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
                    origin_asns: AsnArray(vec![asn]),
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
        self.updated = Utc::now();
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

            let group_id = match record.get(7) {
                Some(id) if !id.is_empty() => id.to_string(),
                None => continue,
                Some(_) => continue,
            };

            let meta = ExtPrefixRecord(
                Some(RirDelExtRecord {
                    group_id,
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
        self.updated = Utc::now();
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

    pub fn get_related_prefixes(
        &self,
        meta: &RirDelExtRecord,
    ) -> RecordSet<'_> {
        RecordSet {
            v4: Self::_get_related_prefixes(&self.v4, meta),
            v6: Self::_get_related_prefixes(&self.v6, meta),
        }
    }

    fn _get_related_prefixes<'b, T: AddressFamily>(
        tree: &'b TreeBitMap<InMemStorage<T, ExtPrefixRecord>>,
        meta: &RirDelExtRecord,
    ) -> Vec<&'b RotondaPrefix<T, ExtPrefixRecord>> {
        tree.store
            .prefixes
            .iter()
            .filter(|&rel_p| {
                if let Some(rel_p_meta) = rel_p.meta.as_ref() {
                    if let Some(rel_p_meta_rde) = rel_p_meta.0.as_ref() {
                        rel_p_meta_rde.group_id == meta.group_id
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .collect()
    }

    pub fn get_prefixes_for_bgp_asn(
        &self,
        asns: &[Asn],
        _search_options: &SearchByAsnOptions,
    ) -> AsnQueryResult<'_> {
        let prefixes_v4 = self
            .v4
            .store
            .prefixes
            .as_slice()
            .iter()
            .filter(|p| {
                if let Some(meta) = p.meta.as_ref() {
                    if let Some(asn_rec) = meta.1.as_ref() {
                        // search the vector of | search_asn X origin_asn |
                        asns.iter().any(|a1| {
                            asn_rec
                                .origin_asns
                                .0
                                .iter()
                                .any(|a2| a2.0 == a1.0)
                        })
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .collect::<Vec<&RotondaPrefix<_, _>>>();

        let prefixes_v6 = self
            .v6
            .store
            .prefixes
            .as_slice()
            .iter()
            .filter(|p| {
                if let Some(meta) = p.meta.as_ref() {
                    if let Some(asn_rec) = meta.1.as_ref() {
                        // search the vector of | search_asn X origin_asn |
                        asns.iter().any(|a1| {
                            asn_rec
                                .origin_asns
                                .0
                                .iter()
                                .any(|a2| a2.0 == a1.0)
                        })
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .collect::<Vec<&RotondaPrefix<_, _>>>();

        AsnQueryResult {
            // asns,
            prefixes: RecordSet {
                v4: prefixes_v4,
                v6: prefixes_v6,
            },
        }
    }

    pub fn output_stats(&self) {
        println!("IPv4\n----");
        Self::output_tree_stats(&self.v4);
        println!("\nIPv6\n----");
        Self::output_tree_stats(&self.v6);
    }

    fn output_tree_stats<AF: AddressFamily + PrimInt + fmt::Debug>(
        tree_bitmap: &TreeBitMap<InMemStorage<AF, ExtPrefixRecord>>,
    ) {
        let total_nodes = tree_bitmap.stats.iter().fold(0, |mut acc, c| {
            acc += c.created_nodes.iter().fold(0, |mut sum, l| {
                sum += l.count;
                sum
            });
            acc
        });
        println!("prefix vec size {}", tree_bitmap.store.prefixes.len());
        println!("finished building tree...");
        println!("{:?} nodes created", total_nodes);
        println!(
            "size of node: {} bytes",
            std::mem::size_of::<SizedStrideNode<AF, InMemNodeId>>()
        );
        println!(
            "memory used by nodes: {}kb",
            total_nodes
                * std::mem::size_of::<SizedStrideNode<AF, InMemNodeId>>()
                / 1024
        );
        println!(
            "size of prefix: {} bytes",
            std::mem::size_of::<RotondaPrefix<AF, ExtPrefixRecord>>()
        );
        println!(
            "memory used by prefixes: {}kb",
            tree_bitmap.store.prefixes.len()
                * std::mem::size_of::<RotondaPrefix<AF, ExtPrefixRecord>>()
                / 1024
        );
        println!("stride division  {:?}", tree_bitmap.strides);

        for s in &tree_bitmap.stats {
            println!("{:?}", s);
        }

        println!(
            "level\t[{}|{}] nodes occupied/max nodes percentage_max_nodes_occupied prefixes",
            Colour::Blue.paint("nodes"),
            Colour::Green.paint("prefixes")
        );
        let bars = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];
        let mut stride_bits = [0, 0];
        const SCALE: u32 = 5500;

        for stride in tree_bitmap.strides.iter().enumerate() {
            // let level = stride.0;
            stride_bits = [stride_bits[1] + 1, stride_bits[1] + stride.1];
            let nodes_num = tree_bitmap
                .stats
                .iter()
                .find(|s| s.stride_len == *stride.1)
                .unwrap()
                .created_nodes[stride.0]
                .count as u32;

            if nodes_num > 0 {
                let prefixes_num = tree_bitmap
                    .stats
                    .iter()
                    .find(|s| s.stride_len == *stride.1)
                    .unwrap()
                    .prefixes_num[stride.0]
                    .count as u32;

                let n = (nodes_num / SCALE) as usize;
                let max_pfx: u128 = u128::pow(2, stride_bits[1] as u32);

                print!("{}-{}\t", stride_bits[0], stride_bits[1]);

                for _ in 0..n {
                    print!("{}", Colour::Blue.paint("█"));
                }

                print!(
                    "{}",
                    Colour::Blue.paint(
                        bars[((nodes_num % SCALE) / (SCALE / 7)) as usize]
                    ) //  = scale / 7
                );

                print!(
                    " {}/{} {:.2}%",
                    nodes_num,
                    max_pfx,
                    (nodes_num as f64 / max_pfx as f64) * 100.0
                );
                print!("\n\t");

                let n = (prefixes_num / SCALE) as usize;
                for _ in 0..n {
                    print!("{}", Colour::Green.paint("█"));
                }

                print!(
                    "{}",
                    Colour::Green.paint(
                        bars[((nodes_num % SCALE) / (SCALE / 7)) as usize]
                    ) //  = scale / 7
                );

                println!(" {}", prefixes_num);
            }
        }
    }
}

//------------ JsonBuilder ---------------------------------------------------

/// A helper type for building a JSON-encoded string on the fly.
///
/// Note that the builder only supports strings without control characters.
pub struct JsonBuilder<'a> {
    target: &'a mut String,
    indent: usize,
    empty: bool,
}

impl JsonBuilder<'static> {
    pub fn build<F: FnOnce(&mut JsonBuilder)>(op: F) -> String {
        let mut target = String::new();
        JsonBuilder {
            target: &mut target,
            indent: 0,
            empty: true,
        }
        .array_object(op);
        target
    }
}

impl<'a> JsonBuilder<'a> {
    pub fn member_object<F: FnOnce(&mut JsonBuilder)>(
        &mut self,
        key: impl fmt::Display,
        op: F,
    ) {
        self.append_key(key);
        self.target.push_str("{\n");
        op(&mut JsonBuilder {
            target: self.target,
            indent: self.indent + 1,
            empty: true,
        });
        self.target.push('\n');
        self.append_indent();
        self.target.push('}');
    }

    pub fn member_array<F: FnOnce(&mut JsonBuilder)>(
        &mut self,
        key: impl fmt::Display,
        op: F,
    ) {
        self.append_key(key);
        self.target.push_str("[\n");
        op(&mut JsonBuilder {
            target: self.target,
            indent: self.indent + 1,
            empty: true,
        });
        self.target.push('\n');
        self.append_indent();
        self.target.push(']');
    }

    pub fn member_str(
        &mut self,
        key: impl fmt::Display,
        value: impl fmt::Display,
    ) {
        self.append_key(key);
        self.target.push('"');
        write!(
            JsonString {
                target: self.target
            },
            "{}",
            value
        )
        .unwrap();
        self.target.push('"');
    }

    pub fn member_raw(
        &mut self,
        key: impl fmt::Display,
        value: impl fmt::Display,
    ) {
        self.append_key(key);
        write!(
            JsonString {
                target: self.target
            },
            "{}",
            value
        )
        .unwrap();
    }

    pub fn array_object<F: FnOnce(&mut JsonBuilder)>(&mut self, op: F) {
        self.append_array_head();
        self.append_indent();
        self.target.push_str("{\n");
        op(&mut JsonBuilder {
            target: self.target,
            indent: self.indent + 1,
            empty: true,
        });
        self.target.push('\n');
        self.append_indent();
        self.target.push('}');
    }

    pub fn array_array<F: FnOnce(&mut JsonBuilder)>(&mut self, op: F) {
        self.append_array_head();
        self.append_indent();
        self.target.push_str("[\n");
        op(&mut JsonBuilder {
            target: self.target,
            indent: self.indent + 1,
            empty: true,
        });
        self.target.push('\n');
        self.append_indent();
        self.target.push(']');
    }

    pub fn array_str(&mut self, value: impl fmt::Display) {
        self.append_array_head();
        self.append_indent();
        self.target.push('"');
        write!(
            JsonString {
                target: self.target
            },
            "{}",
            value
        )
        .unwrap();
        self.target.push('"');
    }

    pub fn array_raw(&mut self, value: impl fmt::Display) {
        self.append_array_head();
        self.append_indent();
        write!(
            JsonString {
                target: self.target
            },
            "{}",
            value
        )
        .unwrap();
    }

    fn append_key(&mut self, key: impl fmt::Display) {
        if self.empty {
            self.empty = false
        } else {
            self.target.push_str(",\n");
        }
        self.append_indent();
        self.target.push('"');
        write!(
            JsonString {
                target: self.target
            },
            "{}",
            key
        )
        .unwrap();
        self.target.push('"');
        self.target.push_str(": ");
    }

    fn append_array_head(&mut self) {
        if self.empty {
            self.empty = false
        } else {
            self.target.push_str(",\n");
        }
    }

    fn append_indent(&mut self) {
        for _ in 0..self.indent {
            self.target.push_str("   ");
        }
    }
}

//------------ JsonString ----------------------------------------------------

struct JsonString<'a> {
    target: &'a mut String,
}

impl<'a> fmt::Write for JsonString<'a> {
    fn write_str(&mut self, mut s: &str) -> Result<(), fmt::Error> {
        while let Some(idx) = s.find(|ch| ch == '"' || ch == '\\') {
            self.target.push_str(&s[..idx]);
            self.target.push('\\');
            self.target.push(char::from(s.as_bytes()[idx]));
            s = &s[idx + 1..];
        }
        self.target.push_str(s);
        Ok(())
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
                    .origin_asns
                    .0
                    .iter()
                    .map(|asn| asn.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn load_riswhois_keeps_first_headerless_row() {
        let tmp = TestDir::new("ris-headerless");
        let csv_path =
            tmp.write("ris.csv", "8.8.8.0,24,15169\n1.1.1.0,24,13335\n");
        let mut store = Store::default();

        store.load_riswhois(&csv_path).unwrap();

        assert_eq!(
            lookup(&store, "8.8.8.8").prefix.unwrap().to_string(),
            "8.8.8.0/24"
        );
        assert_eq!(origin_asns(&lookup(&store, "8.8.8.8")), vec!["AS15169"]);
        assert_eq!(origin_asns(&lookup(&store, "1.1.1.1")), vec!["AS13335"]);
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
