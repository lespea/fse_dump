//! File system event record structures and filtering
//!
//! This module defines the `Record` structure representing individual FSEvents
//! and the `RecordFilter` for selectively processing records based on path patterns
//! and flag criteria.

use regex::Regex;
#[cfg(feature = "hex")]
use serde_hex::{CompactCapPfx, SerHex, SerHexOpt};

use jiff::Timestamp;

use crate::flags;

/// Represents a file system event record from macOS fseventsd
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
    #[serde(skip_serializing)]
    pub file_timestamp: Option<Timestamp>,
}

/// Filter for selecting which records to process based on path patterns and flags
#[derive(Clone, Default)]
pub struct RecordFilter {
    pub path_rex: Option<Regex>,
    pub any_flag: u32,
    pub all_flag: u32,
}

impl RecordFilter {
    /// Creates a new record filter from command-line options
    ///
    /// # Arguments
    /// * `pat` - Optional regex pattern to match against file paths
    /// * `any_flags` - List of flag names where at least one must be present
    /// * `all_flags` - List of flag names where all must be present
    ///
    /// # Panics
    /// Panics if an unknown flag name is provided
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

    /// Determines if a record should be included based on the filter criteria
    ///
    /// # Arguments
    /// * `rec` - The record to test
    ///
    /// # Returns
    /// `true` if the record matches all filter criteria, `false` otherwise
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags;

    /// Helper function to create a test record
    fn make_record(path: &str, flag_bits: u32) -> Record {
        let flag_strs = flags::parse_bits(flag_bits);
        Record {
            path: path.to_string(),
            event_id: 12345,
            flag: flag_bits,
            flags: flag_strs.norm,
            #[cfg(feature = "alt_flags")]
            alt_flags: flag_strs.alt,
            node_id: Some(67890),
            #[cfg(feature = "extra_id")]
            extra_id: Some(42),
            file_timestamp: None,
        }
    }

    #[test]
    fn test_record_default() {
        let rec = Record::default();
        assert_eq!(rec.path, "");
        assert_eq!(rec.event_id, 0);
        assert_eq!(rec.flag, 0);
        assert_eq!(rec.node_id, None);
    }

    #[test]
    fn test_filter_default_accepts_all() {
        let filter = RecordFilter::default();
        let rec = make_record("/test/path", 0x1000_0000);
        assert!(
            filter.want(&rec),
            "Default filter should accept all records"
        );
    }

    #[test]
    fn test_filter_path_regex_match() {
        let filter = RecordFilter::new(&Some(r"^/usr/local/.*".to_string()), &[], &[]);

        let rec1 = make_record("/usr/local/bin/test", 0x1000_0000);
        let rec2 = make_record("/usr/bin/test", 0x1000_0000);
        let rec3 = make_record("/usr/local/lib/foo", 0x1000_0000);

        assert!(filter.want(&rec1), "Should match /usr/local/bin/test");
        assert!(!filter.want(&rec2), "Should not match /usr/bin/test");
        assert!(filter.want(&rec3), "Should match /usr/local/lib/foo");
    }

    #[test]
    fn test_filter_path_regex_case_sensitive() {
        let filter = RecordFilter::new(&Some(r"Test".to_string()), &[], &[]);

        let rec1 = make_record("/path/Test/file", 0x1000_0000);
        let rec2 = make_record("/path/test/file", 0x1000_0000);

        assert!(filter.want(&rec1), "Should match Test");
        assert!(
            !filter.want(&rec2),
            "Should not match test (case sensitive)"
        );
    }

    #[test]
    fn test_filter_any_flags_single() {
        let filter = RecordFilter::new(&None, &["Modified".to_string()], &[]);

        let rec1 = make_record("/test", 0x1000_0000); // Modified
        let rec2 = make_record("/test", 0x0800_0000); // Renamed
        let rec3 = make_record("/test", 0x1800_0000); // Modified | Renamed

        assert!(filter.want(&rec1), "Should match record with Modified flag");
        assert!(
            !filter.want(&rec2),
            "Should not match record without Modified flag"
        );
        assert!(
            filter.want(&rec3),
            "Should match record with Modified flag (even with others)"
        );
    }

    #[test]
    fn test_filter_any_flags_multiple() {
        let filter =
            RecordFilter::new(&None, &["Modified".to_string(), "Created".to_string()], &[]);

        let rec1 = make_record("/test", 0x1000_0000); // Modified
        let rec2 = make_record("/test", 0x0100_0000); // Created
        let rec3 = make_record("/test", 0x0800_0000); // Renamed (neither)
        let rec4 = make_record("/test", 0x1100_0000); // Modified | Created

        assert!(filter.want(&rec1), "Should match Modified");
        assert!(filter.want(&rec2), "Should match Created");
        assert!(!filter.want(&rec3), "Should not match Renamed");
        assert!(filter.want(&rec4), "Should match when both flags present");
    }

    #[test]
    fn test_filter_all_flags_single() {
        let filter = RecordFilter::new(&None, &[], &["Modified".to_string()]);

        let rec1 = make_record("/test", 0x1000_0000); // Modified
        let rec2 = make_record("/test", 0x0800_0000); // Renamed
        let rec3 = make_record("/test", 0x1800_0000); // Modified | Renamed

        assert!(filter.want(&rec1), "Should match record with Modified flag");
        assert!(
            !filter.want(&rec2),
            "Should not match record without Modified flag"
        );
        assert!(
            filter.want(&rec3),
            "Should match record with Modified flag (with others)"
        );
    }

    #[test]
    fn test_filter_all_flags_multiple() {
        let filter = RecordFilter::new(
            &None,
            &[],
            &["Modified".to_string(), "FileEvent".to_string()],
        );

        let rec1 = make_record("/test", 0x1000_0000); // Modified only
        let rec2 = make_record("/test", 0x0000_8000); // FileEvent only
        let rec3 = make_record("/test", 0x1000_8000); // Modified | FileEvent
        let rec4 = make_record("/test", 0x1800_8000); // Modified | FileEvent | Renamed

        assert!(!filter.want(&rec1), "Should not match Modified only");
        assert!(!filter.want(&rec2), "Should not match FileEvent only");
        assert!(
            filter.want(&rec3),
            "Should match both Modified and FileEvent"
        );
        assert!(
            filter.want(&rec4),
            "Should match when both required flags present (even with others)"
        );
    }

    #[test]
    fn test_filter_combined_path_and_flags() {
        let filter =
            RecordFilter::new(&Some(r"\.txt$".to_string()), &["Modified".to_string()], &[]);

        let rec1 = make_record("/test/file.txt", 0x1000_0000); // Matches both
        let rec2 = make_record("/test/file.txt", 0x0800_0000); // Matches path only
        let rec3 = make_record("/test/file.log", 0x1000_0000); // Matches flag only
        let rec4 = make_record("/test/file.log", 0x0800_0000); // Matches neither

        assert!(filter.want(&rec1), "Should match both path and flag");
        assert!(!filter.want(&rec2), "Should not match path without flag");
        assert!(!filter.want(&rec3), "Should not match flag without path");
        assert!(!filter.want(&rec4), "Should not match neither");
    }

    #[test]
    fn test_filter_complex_combined() {
        let filter = RecordFilter::new(
            &Some(r"^/Users/[^/]+/Documents/".to_string()),
            &["Created".to_string(), "Removed".to_string()],
            &["FileEvent".to_string()],
        );

        // Has FileEvent and Created, matches path
        let rec1 = make_record("/Users/john/Documents/test.txt", 0x0100_8000);
        // Has FileEvent and Removed, matches path
        let rec2 = make_record("/Users/jane/Documents/file.pdf", 0x0200_8000);
        // Has FileEvent but no Created/Removed, matches path
        let rec3 = make_record("/Users/bob/Documents/data.csv", 0x0000_8000);
        // Has FileEvent and Created, wrong path
        let rec4 = make_record("/Users/alice/Downloads/test.txt", 0x0100_8000);
        // Right path, Created, but no FileEvent
        let rec5 = make_record("/Users/charlie/Documents/note.md", 0x0100_0000);

        assert!(
            filter.want(&rec1),
            "Should match: path + FileEvent + Created"
        );
        assert!(
            filter.want(&rec2),
            "Should match: path + FileEvent + Removed"
        );
        assert!(
            !filter.want(&rec3),
            "Should not match: missing any_flags (Created or Removed)"
        );
        assert!(!filter.want(&rec4), "Should not match: wrong path");
        assert!(
            !filter.want(&rec5),
            "Should not match: missing all_flags (FileEvent)"
        );
    }

    #[test]
    fn test_filter_empty_path_filter() {
        let filter = RecordFilter::new(&Some("".to_string()), &[], &[]);

        let rec = make_record("/any/path", 0x1000_0000);
        assert!(filter.want(&rec), "Empty regex should match all paths");
    }

    #[test]
    fn test_filter_zero_flag() {
        let filter = RecordFilter::new(&None, &[], &[]);

        let rec = make_record("/test", 0x0000_0000);
        assert!(filter.want(&rec), "Should match record with no flags set");
    }

    #[test]
    #[should_panic(expected = "Unknown any flag id")]
    fn test_filter_invalid_any_flag() {
        RecordFilter::new(&None, &["InvalidFlagName".to_string()], &[]);
    }

    #[test]
    #[should_panic(expected = "Unknown all flag id")]
    fn test_filter_invalid_all_flag() {
        RecordFilter::new(&None, &[], &["AnotherInvalidFlag".to_string()]);
    }

    #[test]
    #[should_panic(expected = "Invalid pattern")]
    fn test_filter_invalid_regex() {
        RecordFilter::new(&Some("[invalid(".to_string()), &[], &[]);
    }

    #[test]
    fn test_match_flags_any_with_zero() {
        let filter = RecordFilter {
            path_rex: None,
            any_flag: 0,
            all_flag: 0,
        };

        let rec = make_record("/test", 0x1000_0000);
        assert!(
            filter.match_flags(&rec, 0, true),
            "Zero flags with 'any' should always match"
        );
    }

    #[test]
    fn test_match_flags_all_with_zero() {
        let filter = RecordFilter {
            path_rex: None,
            any_flag: 0,
            all_flag: 0,
        };

        let rec = make_record("/test", 0x1000_0000);
        assert!(
            filter.match_flags(&rec, 0, false),
            "Zero flags with 'all' should always match"
        );
    }

    #[test]
    fn test_record_serialization() {
        let rec = make_record("/test/path.txt", 0x1000_0000);
        let json = serde_json::to_string(&rec).expect("Should serialize to JSON");
        assert!(json.contains("/test/path.txt"));
        assert!(json.contains("Modified"));
    }

    #[test]
    fn test_multiple_flags_in_record() {
        // Created | Modified | FileEvent
        let flag_bits = 0x0100_0000 | 0x1000_0000 | 0x0000_8000;
        let rec = make_record("/test", flag_bits);

        assert!(rec.flags.contains("Created"));
        assert!(rec.flags.contains("Modified"));
        assert!(rec.flags.contains("FileEvent"));
    }
}
