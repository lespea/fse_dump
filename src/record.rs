use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Default, Serialize)]
pub struct Record<'a> {
    pub path: Cow<'a, str>,
    pub event_id: u64,
    pub flags: Arc<String>,
    pub node_id: Option<u64>,
}
