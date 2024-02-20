use regex::bytes::Regex;
#[cfg(feature = "hex")]
use serde_hex::{CompactCapPfx, SerHex, SerHexOpt};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Record {
    pub path: String,
    #[cfg_attr(feature = "hex", serde(with = "SerHex::<CompactCapPfx>"))]
    pub event_id: u64,
    #[serde(skip_serializing)]
    pub flag: u32,
    pub flags: &'static str,
    pub alt_flags: &'static str,
    #[cfg_attr(feature = "hex", serde(with = "SerHexOpt::<CompactCapPfx>"))]
    pub node_id: Option<u64>,
    #[cfg(feature = "extra_id")]
    #[cfg_attr(feature = "hex", serde(with = "SerHexOpt::<CompactCapPfx>"))]
    pub extra_id: Option<u32>,
}

pub trait RecordFilter {
    fn filter(&self, rec: &Record) -> bool;
}

#[derive(Clone, Copy)]
pub struct NoRecordFilter;
impl RecordFilter for NoRecordFilter {
    #[inline]
    fn filter(&self, _: &Record) -> bool {
        true
    }
}

pub struct PathFilter {
    pub path_rex: Regex,
}

impl RecordFilter for PathFilter {
    #[inline]
    fn filter(&self, rec: &Record) -> bool {
        self.path_rex.is_match(rec.path.as_bytes())
    }
}
