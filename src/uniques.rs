use crate::flags as f;

#[derive(Debug, Default, Eq, PartialEq)]
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
        let flags = f::parse_bits(self.flags);
        UniqueOut {
            path,
            counts: self.counts,
            flags: flags.norm,
            alt_flags: flags.alt,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UniqueOut {
    path: String,
    counts: u64,
    flags: &'static str,
    alt_flags: &'static str,
}
