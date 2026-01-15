//! Unique path aggregation and counting
//!
//! This module provides structures to aggregate FSEvents records by path,
//! combining their flags and counting occurrences.

use jiff::Timestamp;

use crate::flags as f;

/// Aggregates counts and flags for a unique path
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UniqueCounts {
    counts: u64,
    flags: u32,
    earliest_timestamp: Option<Timestamp>,
    latest_timestamp: Option<Timestamp>,
}

impl UniqueCounts {
    /// Updates the count and combines the flag bits
    ///
    /// # Arguments
    /// * `flag` - The flag bits to OR with existing flags
    /// * `file_timestamp` - The file modification timestamp to track earliest/latest
    #[inline]
    pub fn update(&mut self, flag: u32, file_timestamp: Option<Timestamp>) {
        self.counts += 1;
        self.flags |= flag;

        if let Some(ts) = file_timestamp {
            match (self.earliest_timestamp, self.latest_timestamp) {
                (None, None) => {
                    // First update - initialize both
                    self.earliest_timestamp = Some(ts);
                    self.latest_timestamp = Some(ts);
                }
                (Some(earliest), Some(latest)) => {
                    // Track min and max timestamps
                    if ts < earliest {
                        self.earliest_timestamp = Some(ts);
                    }
                    if ts > latest {
                        self.latest_timestamp = Some(ts);
                    }
                }
                _ => {
                    // This shouldn't happen, but handle it gracefully
                    self.earliest_timestamp = Some(ts);
                    self.latest_timestamp = Some(ts);
                }
            }
        }
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
            earliest_timestamp: self.earliest_timestamp,
            latest_timestamp: self.latest_timestamp,
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
    #[serde(serialize_with = "serialize_optional_timestamp")]
    earliest_timestamp: Option<Timestamp>,
    #[serde(serialize_with = "serialize_optional_timestamp")]
    latest_timestamp: Option<Timestamp>,
}

/// Custom serializer for Option<Timestamp> to produce ISO 8601 format
fn serialize_optional_timestamp<S>(
    timestamp: &Option<Timestamp>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match timestamp {
        Some(ts) => serializer.serialize_str(&ts.to_string()),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test timestamps from seconds since epoch
    fn ts(secs: i64) -> Option<Timestamp> {
        Timestamp::from_second(secs).ok()
    }

    #[test]
    fn test_unique_counts_default() {
        let uc = UniqueCounts::default();
        assert_eq!(uc.counts, 0);
        assert_eq!(uc.flags, 0);
        assert_eq!(uc.earliest_timestamp, None);
        assert_eq!(uc.latest_timestamp, None);
    }

    #[test]
    fn test_unique_counts_single_update() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000, ts(100)); // Modified

        assert_eq!(uc.counts, 1);
        assert_eq!(uc.flags, 0x1000_0000);
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
        assert_eq!(uc.earliest_timestamp, uc.latest_timestamp);
    }

    #[test]
    fn test_unique_counts_multiple_updates_same_flag() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000, ts(100)); // Modified
        uc.update(0x1000_0000, ts(200)); // Modified
        uc.update(0x1000_0000, ts(150)); // Modified

        assert_eq!(uc.counts, 3, "Count should increment for each update");
        assert_eq!(uc.flags, 0x1000_0000, "Flags should remain the same");
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
        assert!(
            uc.earliest_timestamp < uc.latest_timestamp,
            "Should track earliest < latest"
        );
    }

    #[test]
    fn test_unique_counts_multiple_updates_different_flags() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000, ts(100)); // Modified
        uc.update(0x0800_0000, ts(200)); // Renamed
        uc.update(0x0100_0000, ts(150)); // Created

        assert_eq!(uc.counts, 3, "Count should be 3");
        assert_eq!(
            uc.flags,
            0x1000_0000 | 0x0800_0000 | 0x0100_0000,
            "Flags should be ORed together"
        );
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
    }

    #[test]
    fn test_unique_counts_flag_accumulation() {
        let mut uc = UniqueCounts::default();

        // Update with different flags, simulating different operations on same path
        uc.update(0x0100_0000, ts(1000)); // Created
        assert_eq!(uc.flags, 0x0100_0000);

        uc.update(0x1000_0000, ts(2000)); // Modified
        assert_eq!(uc.flags, 0x0100_0000 | 0x1000_0000);

        uc.update(0x0800_0000, ts(1500)); // Renamed
        assert_eq!(uc.flags, 0x0100_0000 | 0x1000_0000 | 0x0800_0000);

        assert_eq!(uc.counts, 3);
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
    }

    #[test]
    fn test_unique_counts_idempotent_flag_or() {
        let mut uc = UniqueCounts::default();

        uc.update(0x1000_0000, ts(100)); // Modified
        uc.update(0x1000_0000, ts(200)); // Modified again

        // Flag should only be set once (OR is idempotent)
        assert_eq!(uc.flags, 0x1000_0000);
        assert_eq!(uc.counts, 2, "But count should still be 2");
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
    }

    #[test]
    fn test_into_unique_out() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000, ts(1000)); // Modified
        uc.update(0x0100_0000, ts(2000)); // Created

        let out = uc.into_unique_out("/test/path.txt".to_string());

        assert_eq!(out.path, "/test/path.txt");
        assert_eq!(out.counts, 2);
        assert!(out.flags.contains("Modified"));
        assert!(out.flags.contains("Created"));
        assert!(out.earliest_timestamp.is_some());
        assert!(out.latest_timestamp.is_some());
    }

    #[test]
    fn test_into_unique_out_zero_counts() {
        let uc = UniqueCounts::default();
        let out = uc.into_unique_out("/empty/path".to_string());

        assert_eq!(out.path, "/empty/path");
        assert_eq!(out.counts, 0);
        assert_eq!(out.flags, "", "Zero flags should produce empty string");
        assert_eq!(out.earliest_timestamp, None);
        assert_eq!(out.latest_timestamp, None);
    }

    #[test]
    fn test_into_unique_out_single_flag() {
        let mut uc = UniqueCounts::default();
        uc.update(0x0200_0000, ts(500)); // Removed

        let out = uc.into_unique_out("/deleted/file".to_string());

        assert_eq!(out.counts, 1);
        assert_eq!(out.flags, "Removed");
        assert!(out.earliest_timestamp.is_some());
        assert!(out.latest_timestamp.is_some());
    }

    #[test]
    fn test_into_unique_out_all_flags() {
        let mut uc = UniqueCounts::default();

        // Add all possible flags
        uc.update(0x0000_0001, ts(1)); // FolderEvent
        uc.update(0x0000_0002, ts(2)); // Mount
        uc.update(0x0000_0004, ts(3)); // Unmount
        uc.update(0x0000_0020, ts(4)); // EndOfTransaction
        uc.update(0x0000_0800, ts(5)); // LastHardLinkRemoved
        uc.update(0x0000_1000, ts(6)); // HardLink
        uc.update(0x0000_4000, ts(7)); // SymbolicLink
        uc.update(0x0000_8000, ts(8)); // FileEvent
        uc.update(0x0001_0000, ts(9)); // PermissionChange
        uc.update(0x0002_0000, ts(10)); // ExtendedAttrModified
        uc.update(0x0004_0000, ts(11)); // ExtendedAttrRemoved
        uc.update(0x0010_0000, ts(12)); // DocumentRevisioning
        uc.update(0x0040_0000, ts(13)); // ItemCloned
        uc.update(0x0100_0000, ts(14)); // Created
        uc.update(0x0200_0000, ts(15)); // Removed
        uc.update(0x0400_0000, ts(16)); // InodeMetaMod
        uc.update(0x0800_0000, ts(17)); // Renamed
        uc.update(0x1000_0000, ts(18)); // Modified
        uc.update(0x2000_0000, ts(19)); // Exchange
        uc.update(0x4000_0000, ts(20)); // FinderInfoMod
        uc.update(0x8000_0000, ts(21)); // FolderCreated

        let out = uc.into_unique_out("/complex/path".to_string());

        assert_eq!(out.counts, 21);
        // Verify some key flags are present
        assert!(out.flags.contains("Created"));
        assert!(out.flags.contains("Modified"));
        assert!(out.flags.contains("Removed"));
        assert!(out.flags.contains("FileEvent"));
        assert!(out.earliest_timestamp.is_some());
        assert!(out.latest_timestamp.is_some());
    }

    #[test]
    fn test_unique_out_serialization() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000, ts(100)); // Modified
        uc.update(0x1000_0000, ts(200)); // Modified again

        let out = uc.into_unique_out("/test.txt".to_string());
        let json = serde_json::to_string(&out).expect("Should serialize to JSON");

        assert!(json.contains("/test.txt"));
        assert!(json.contains("Modified"));
        assert!(json.contains("\"counts\":2"));
        // Timestamps will be ISO 8601 format, just check they exist
        assert!(json.contains("earliest_timestamp"));
        assert!(json.contains("latest_timestamp"));
    }

    #[test]
    fn test_unique_counts_equality() {
        let uc1 = UniqueCounts {
            counts: 5,
            flags: 0x1000_0000,
            earliest_timestamp: ts(100),
            latest_timestamp: ts(200),
        };
        let uc2 = UniqueCounts {
            counts: 5,
            flags: 0x1000_0000,
            earliest_timestamp: ts(100),
            latest_timestamp: ts(200),
        };
        let uc3 = UniqueCounts {
            counts: 3,
            flags: 0x1000_0000,
            earliest_timestamp: ts(100),
            latest_timestamp: ts(200),
        };

        assert_eq!(uc1, uc2, "Same counts and flags should be equal");
        assert_ne!(uc1, uc3, "Different counts should not be equal");
    }

    #[test]
    fn test_unique_counts_large_count() {
        let mut uc = UniqueCounts::default();

        // Simulate a frequently accessed file
        for i in 0..10000 {
            uc.update(0x1000_0000, ts(i)); // Modified
        }

        assert_eq!(uc.counts, 10000);
        assert_eq!(uc.flags, 0x1000_0000);
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
    }

    #[test]
    fn test_unique_counts_update_with_zero() {
        let mut uc = UniqueCounts::default();
        uc.update(0, ts(42)); // No flags

        assert_eq!(uc.counts, 1, "Count should increment even with zero flags");
        assert_eq!(uc.flags, 0, "Flags should remain zero");
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
    }

    #[test]
    fn test_path_special_characters() {
        let mut uc = UniqueCounts::default();
        uc.update(0x1000_0000, ts(999));

        let paths = vec![
            "/path with spaces/file.txt",
            "/path/with/üñíçödé/file.txt",
            "/path/with/emoji/😀.txt",
            "/path/with/quotes/\"file\".txt",
        ];

        for path in paths {
            let out = uc.into_unique_out(path.to_string());
            assert_eq!(out.path, path);
            assert!(out.earliest_timestamp.is_some());
            assert!(out.latest_timestamp.is_some());
        }
    }

    #[test]
    fn test_unique_counts_debug_format() {
        let uc = UniqueCounts {
            counts: 42,
            flags: 0x1000_0000,
            earliest_timestamp: ts(100),
            latest_timestamp: ts(200),
        };
        let debug_str = format!("{:?}", uc);

        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("268435456") || debug_str.contains("1000_0000"));
    }

    #[cfg(feature = "alt_flags")]
    #[test]
    fn test_unique_out_with_alt_flags() {
        let mut uc = UniqueCounts::default();
        uc.update(0x0000_0001, ts(1)); // Different meaning in alt_flags

        let out = uc.into_unique_out("/test".to_string());

        // Should have both norm and alt_flags populated
        assert!(!out.flags.is_empty());
        assert!(!out.alt_flags.is_empty());
        assert!(out.earliest_timestamp.is_some());
        assert!(out.latest_timestamp.is_some());
    }

    #[test]
    fn test_timestamp_tracking_out_of_order() {
        let mut uc = UniqueCounts::default();

        // Updates arrive out of order
        uc.update(0x1000_0000, ts(500));
        uc.update(0x1000_0000, ts(100)); // Earlier timestamp
        uc.update(0x1000_0000, ts(1000)); // Later timestamp
        uc.update(0x1000_0000, ts(300));

        assert_eq!(uc.counts, 4);
        assert!(
            uc.earliest_timestamp < uc.latest_timestamp,
            "Should find minimum timestamp"
        );
    }

    #[test]
    fn test_timestamp_tracking_reverse_order() {
        let mut uc = UniqueCounts::default();

        // Updates arrive in reverse chronological order
        uc.update(0x1000_0000, ts(1000));
        uc.update(0x1000_0000, ts(900));
        uc.update(0x1000_0000, ts(800));
        uc.update(0x1000_0000, ts(700));

        assert_eq!(uc.counts, 4);
        assert!(uc.earliest_timestamp.is_some());
        assert!(uc.latest_timestamp.is_some());
        assert!(uc.earliest_timestamp < uc.latest_timestamp);
    }

    #[test]
    fn test_timestamp_with_same_values() {
        let mut uc = UniqueCounts::default();

        // All updates have the same timestamp (from same file)
        uc.update(0x1000_0000, ts(500));
        uc.update(0x0800_0000, ts(500));
        uc.update(0x0100_0000, ts(500));

        assert_eq!(uc.counts, 3);
        assert_eq!(uc.earliest_timestamp, uc.latest_timestamp);
    }
}
