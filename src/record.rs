use regex::Regex;
#[cfg(feature = "hex")]
use serde_hex::{CompactCapPfx, SerHex, SerHexOpt};

use crate::flags;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Record {
    pub path: String,
    #[cfg_attr(feature = "hex", serde(with = "SerHex::<CompactCapPfx>"))]
    pub event_id: u64,
    #[serde(skip_serializing)]
    pub flag: u32,
    pub flags: &'static str,
    #[cfg(feature = "alt_flags")]
    pub alt_flags: &'static str,
    #[cfg_attr(feature = "hex", serde(with = "SerHexOpt::<CompactCapPfx>"))]
    pub node_id: Option<u64>,
    #[cfg(feature = "extra_id")]
    #[cfg_attr(feature = "hex", serde(with = "SerHexOpt::<CompactCapPfx>"))]
    pub extra_id: Option<u32>,
}

#[derive(Clone, Default)]
pub struct RecordFilter {
    pub path_rex: Option<Regex>,
    pub any_flag: u32,
    pub all_flag: u32,
}

impl RecordFilter {
    pub fn new(pat: &Option<String>, any_flags: &[String], all_flags: &[String]) -> Self {
        let mut any_flag = 0;
        let mut all_flag = 0;

        for flag in any_flags.iter() {
            any_flag |=
                flags::flag_id(flag).unwrap_or_else(|| panic!("Unknown any flag id: {flag}"));
        }

        for flag in all_flags.iter() {
            all_flag |=
                flags::flag_id(flag).unwrap_or_else(|| panic!("Unknown all flag id: {flag}"));
        }

        Self {
            path_rex: pat
                .as_ref()
                .map(|pat| Regex::new(pat).expect("Invalid pattern")),
            any_flag,
            all_flag,
        }
    }

    #[inline]
    pub fn want(&self, rec: &Record) -> bool {
        self.match_flags(rec, self.any_flag, true)
            && self.match_flags(rec, self.all_flag, false)
            && self.match_path(rec)
    }

    #[inline]
    fn match_path(&self, rec: &Record) -> bool {
        self.path_rex
            .as_ref()
            .map(|rex| rex.is_match(&rec.path))
            .unwrap_or(true)
    }

    #[inline]
    fn match_flags(&self, rec: &Record, flags: u32, any: bool) -> bool {
        if flags > 0 {
            let diff = flags & rec.flag;
            if any { diff > 0 } else { diff == flags }
        } else {
            true
        }
    }
}
