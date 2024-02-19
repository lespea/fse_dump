#[cfg(feature = "hex")]
use serde_hex::{CompactCapPfx, SerHexOpt};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Record {
    pub path: String,
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
