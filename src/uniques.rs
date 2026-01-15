//! Unique path aggregation and counting
//!
//! This module provides structures to aggregate FSEvents records by path,
//! combining their flags and counting occurrences.

use crate::flags as f;

/// Aggregates counts and flags for a unique path
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UniqueCounts {
    counts: u64,
    flags: u32,
}

impl UniqueCounts {
    /// Updates the count and combines the flag bits
    ///
    /// # Arguments
    /// * `flag` - The flag bits to OR with existing flags
    #[inline]
    pub fn update(&mut self, flag: u32) {
        self.counts += 1;
        self.flags |= flag;
    }

    /// Converts internal counts to output format with parsed flag strings
    ///
    /// # Arguments
    /// * `path` - The file path associated with these counts
    ///
    /// # Returns
    /// A `UniqueOut` ready for serialization
    #[inline]
    pub fn into_unique_out(self, path: String) -> UniqueOut {
        let flags = f::parse_bits(self.flags);
        UniqueOut {
            path,
            counts: self.counts,
            flags: flags.norm,
            #[cfg(feature = "alt_flags")]
            alt_flags: flags.alt,
        }
    }
}

/// Output structure for unique path aggregation results
#[derive(Debug, Serialize)]
pub struct UniqueOut {
    path: String,
    counts: u64,
    flags: &'static str,
    #[cfg(feature = "alt_flags")]
    alt_flags: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_counts_default() {
        let uc = UniqueCounts::default();
        assert_eq!(uc.counts, 0);
        assert_eq!(uc.flags, 0);
    }

    #[test]
    fn test_unique_counts_single_update() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000); // Modified

        assert_eq!(uc.counts, 1);
        assert_eq!(uc.flags, 0x1000_0000);
    }

    #[test]
    fn test_unique_counts_multiple_updates_same_flag() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000); // Modified
        uc.update(0x1000_0000); // Modified
        uc.update(0x1000_0000); // Modified

        assert_eq!(uc.counts, 3, "Count should increment for each update");
        assert_eq!(uc.flags, 0x1000_0000, "Flags should remain the same");
    }

    #[test]
    fn test_unique_counts_multiple_updates_different_flags() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000); // Modified
        uc.update(0x0800_0000); // Renamed
        uc.update(0x0100_0000); // Created

        assert_eq!(uc.counts, 3, "Count should be 3");
        assert_eq!(
            uc.flags,
            0x1000_0000 | 0x0800_0000 | 0x0100_0000,
            "Flags should be ORed together"
        );
    }

    #[test]
    fn test_unique_counts_flag_accumulation() {
        let mut uc = UniqueCounts::default();

        // Update with different flags, simulating different operations on same path
        uc.update(0x0100_0000); // Created
        assert_eq!(uc.flags, 0x0100_0000);

        uc.update(0x1000_0000); // Modified
        assert_eq!(uc.flags, 0x0100_0000 | 0x1000_0000);

        uc.update(0x0800_0000); // Renamed
        assert_eq!(uc.flags, 0x0100_0000 | 0x1000_0000 | 0x0800_0000);

        assert_eq!(uc.counts, 3);
    }

    #[test]
    fn test_unique_counts_idempotent_flag_or() {
        let mut uc = UniqueCounts::default();

        uc.update(0x1000_0000); // Modified
        uc.update(0x1000_0000); // Modified again

        // Flag should only be set once (OR is idempotent)
        assert_eq!(uc.flags, 0x1000_0000);
        assert_eq!(uc.counts, 2, "But count should still be 2");
    }

    #[test]
    fn test_into_unique_out() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000); // Modified
        uc.update(0x0100_0000); // Created

        let out = uc.into_unique_out("/test/path.txt".to_string());

        assert_eq!(out.path, "/test/path.txt");
        assert_eq!(out.counts, 2);
        assert!(out.flags.contains("Modified"));
        assert!(out.flags.contains("Created"));
    }

    #[test]
    fn test_into_unique_out_zero_counts() {
        let uc = UniqueCounts::default();
        let out = uc.into_unique_out("/empty/path".to_string());

        assert_eq!(out.path, "/empty/path");
        assert_eq!(out.counts, 0);
        assert_eq!(out.flags, "", "Zero flags should produce empty string");
    }

    #[test]
    fn test_into_unique_out_single_flag() {
        let mut uc = UniqueCounts::default();
        uc.update(0x0200_0000); // Removed

        let out = uc.into_unique_out("/deleted/file".to_string());

        assert_eq!(out.counts, 1);
        assert_eq!(out.flags, "Removed");
    }

    #[test]
    fn test_into_unique_out_all_flags() {
        let mut uc = UniqueCounts::default();

        // Add all possible flags
        uc.update(0x0000_0001); // FolderEvent
        uc.update(0x0000_0002); // Mount
        uc.update(0x0000_0004); // Unmount
        uc.update(0x0000_0020); // EndOfTransaction
        uc.update(0x0000_0800); // LastHardLinkRemoved
        uc.update(0x0000_1000); // HardLink
        uc.update(0x0000_4000); // SymbolicLink
        uc.update(0x0000_8000); // FileEvent
        uc.update(0x0001_0000); // PermissionChange
        uc.update(0x0002_0000); // ExtendedAttrModified
        uc.update(0x0004_0000); // ExtendedAttrRemoved
        uc.update(0x0010_0000); // DocumentRevisioning
        uc.update(0x0040_0000); // ItemCloned
        uc.update(0x0100_0000); // Created
        uc.update(0x0200_0000); // Removed
        uc.update(0x0400_0000); // InodeMetaMod
        uc.update(0x0800_0000); // Renamed
        uc.update(0x1000_0000); // Modified
        uc.update(0x2000_0000); // Exchange
        uc.update(0x4000_0000); // FinderInfoMod
        uc.update(0x8000_0000); // FolderCreated

        let out = uc.into_unique_out("/complex/path".to_string());

        assert_eq!(out.counts, 21);
        // Verify some key flags are present
        assert!(out.flags.contains("Created"));
        assert!(out.flags.contains("Modified"));
        assert!(out.flags.contains("Removed"));
        assert!(out.flags.contains("FileEvent"));
    }

    #[test]
    fn test_unique_out_serialization() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000); // Modified
        uc.update(0x1000_0000); // Modified again

        let out = uc.into_unique_out("/test.txt".to_string());
        let json = serde_json::to_string(&out).expect("Should serialize to JSON");

        assert!(json.contains("/test.txt"));
        assert!(json.contains("Modified"));
        assert!(json.contains("\"counts\":2"));
    }

    #[test]
    fn test_unique_counts_equality() {
        let uc1 = UniqueCounts {
            counts: 5,
            flags: 0x1000_0000,
        };
        let uc2 = UniqueCounts {
            counts: 5,
            flags: 0x1000_0000,
        };
        let uc3 = UniqueCounts {
            counts: 3,
            flags: 0x1000_0000,
        };

        assert_eq!(uc1, uc2, "Same counts and flags should be equal");
        assert_ne!(uc1, uc3, "Different counts should not be equal");
    }

    #[test]
    fn test_unique_counts_large_count() {
        let mut uc = UniqueCounts::default();

        // Simulate a frequently accessed file
        for _ in 0..10000 {
            uc.update(0x1000_0000); // Modified
        }

        assert_eq!(uc.counts, 10000);
        assert_eq!(uc.flags, 0x1000_0000);
    }

    #[test]
    fn test_unique_counts_update_with_zero() {
        let mut uc = UniqueCounts::default();
        uc.update(0); // No flags

        assert_eq!(uc.counts, 1, "Count should increment even with zero flags");
        assert_eq!(uc.flags, 0, "Flags should remain zero");
    }

    #[test]
    fn test_path_special_characters() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000);

        let paths = vec![
            "/path with spaces/file.txt",
            "/path/with/üñíçödé/file.txt",
            "/path/with/emoji/😀.txt",
            "/path/with/quotes/\"file\".txt",
        ];

        for path in paths {
            let out = uc.clone().into_unique_out(path.to_string());
            assert_eq!(out.path, path);
        }
    }

    #[test]
    fn test_unique_counts_debug_format() {
        let uc = UniqueCounts {
            counts: 42,
            flags: 0x1000_0000,
        };
        let debug_str = format!("{:?}", uc);

        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("268435456") || debug_str.contains("1000_0000"));
    }

    #[cfg(feature = "alt_flags")]
    #[test]
    fn test_unique_out_with_alt_flags() {
        let mut uc = UniqueCounts::default();
        uc.update(0x0000_0001); // Different meaning in alt_flags

        let out = uc.into_unique_out("/test".to_string());

        // Should have both norm and alt_flags populated
        assert!(!out.flags.is_empty());
        assert!(!out.alt_flags.is_empty());
    }
}
