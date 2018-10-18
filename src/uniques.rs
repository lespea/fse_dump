use flags as f;

use std::sync::Arc;

#[derive(Debug, Default, PartialEq)]
pub struct UniqueCounts {
    counts: u64,
    flags: u32,
}

impl UniqueCounts {
    #[inline]
    pub fn update(&mut self, flag: u32) {
        self.counts += 1;
        self.flags |= flag;
    }

    #[inline]
    pub fn into_unique_out(self, path: String) -> UniqueOut {
        UniqueOut {
            path,
            counts: self.counts,
            flags: f::parse_bits(self.flags),
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct UniqueOut {
    path: String,
    counts: u64,
    flags: Arc<String>,
}
