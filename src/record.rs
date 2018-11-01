use std::sync::Arc;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Record {
    pub path: String,
    pub event_id: u64,
    #[serde(skip_serializing)]
    pub flag: u32,
    pub flags: Arc<String>,
    pub node_id: Option<u64>,
}
