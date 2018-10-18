use std::{borrow::Cow, sync::Arc};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Record<'a> {
    pub path: Cow<'a, str>,
    pub event_id: u64,
    #[serde(skip_serializing)]
    pub flag: u32,
    pub flags: Arc<String>,
    pub node_id: Option<u64>,
}
