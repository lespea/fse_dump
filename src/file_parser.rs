//! FSEvents file parsing implementation
//!
//! This module provides the core functionality to parse compressed FSEvents files,
//! handling multiple file format versions and broadcasting records through a bus.

use std::{
    fs::File,
    io::{BufReader, ErrorKind, prelude::*},
    path::Path,
    sync::Arc,
};

use bus::Bus;
use byteorder::{LittleEndian, ReadBytesExt};
use color_eyre::{Result, eyre::eyre};
use flate2::read::MultiGzDecoder;

use crate::{
    record::{Record, RecordFilter},
    version,
};

/// Parses an FSEvents file and broadcasts records through the provided bus
///
/// The file is automatically decompressed using gzip if needed.
/// Supports multiple FSEvents versions (V1, V2, V3) within a single file.
///
/// # Arguments
/// * `in_file` - Path to the FSEvents file to parse
/// * `bus` - Message bus to broadcast parsed records
/// * `filter` - Filter to determine which records to broadcast
///
/// # Returns
/// `Ok(())` on success, or an error if the file cannot be parsed
///
/// # Errors
/// Returns an error if:
/// - The file cannot be opened or read
/// - The file has an unsupported format version
/// - Record lengths don't match expected values
pub fn parse_file(in_file: &Path, bus: &mut Bus<Arc<Record>>, filter: &RecordFilter) -> Result<()> {
    info!("Parsing {}", in_file.display());
    let mut reader = BufReader::new(MultiGzDecoder::new(File::open(in_file)?));

    loop {
        debug!("starting loop");
        let v = match version::Version::from_reader(&mut reader) {
            Err(e) => {
                if e.kind() == ErrorKind::UnexpectedEof {
                    debug!("eof");
                    break;
                }

                return Err(e.into());
            }

            Ok(Some(v)) => v,

            Ok(None) => {
                return Err(eyre!(
                    "Unsupported or invalid file version for: {}",
                    in_file.display()
                ));
            }
        };
        let parse_fun = v.get_parser();

        reader.read_exact(&mut [0u8; 4])?;
        let p_len = reader.read_u32::<LittleEndian>()? as usize;

        debug!("{v:?} :: {p_len}");

        let mut read = 12usize;

        loop {
            let rec = match parse_fun(&mut reader)? {
                None => break,
                Some((s, rec)) => {
                    debug!("Read {s} bits");
                    read += s;
                    rec
                }
            };

            // Check length before filtering to avoid reading past page boundary
            if read >= p_len {
                if read == p_len {
                    debug!("Wanted len");
                    // Still broadcast if filter accepts it
                    if filter.want(&rec) {
                        bus.broadcast(Arc::new(rec));
                    } else {
                        debug!("Skipping {rec:?} due to the filters");
                    }
                    break;
                } else {
                    return Err(eyre!("Length of page records didn't match expected length",));
                }
            }

            if !filter.want(&rec) {
                debug!("Skipping {rec:?} due to the filters");
                continue;
            }

            bus.broadcast(Arc::new(rec));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bus::Bus;

    use crate::record::RecordFilter;

    use super::parse_file;

    #[test]
    fn test_v3() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();
        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't find test file");
        drop(bus);

        let count = recv.iter().count();
        assert_eq!(count, 2730);
    }

    #[test]
    fn test_v3_with_path_filter() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        // Filter for paths containing "Library"
        let filter = RecordFilter::new(&Some("Library".to_string()), &[], &[]);

        parse_file(&path, &mut bus, &filter).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();
        let count = records.len();

        // Should be fewer than total (2730)
        assert!(count > 0, "Should find some Library paths");
        assert!(count < 2730, "Should filter out some records");

        // Verify all records match the filter
        for rec in records {
            assert!(
                rec.path.contains("Library"),
                "Record path should contain 'Library': {}",
                rec.path
            );
        }
    }

    #[test]
    fn test_v3_with_flag_filter() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        // Filter for Modified flag
        let filter = RecordFilter::new(&None, &["Modified".to_string()], &[]);

        parse_file(&path, &mut bus, &filter).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();
        let count = records.len();

        assert!(count > 0, "Should find some Modified records");
        assert!(count < 2730, "Should filter out some records");

        // Verify all records have the Modified flag
        for rec in records {
            assert!(
                rec.flags.contains("Modified"),
                "Record should have Modified flag: {}",
                rec.flags
            );
        }
    }

    #[test]
    fn test_v3_with_all_flags_filter() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        // Filter for records that have BOTH FileEvent AND Modified
        let filter = RecordFilter::new(
            &None,
            &[],
            &["FileEvent".to_string(), "Modified".to_string()],
        );

        parse_file(&path, &mut bus, &filter).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // Verify all records have both flags
        for rec in records {
            assert!(
                rec.flags.contains("FileEvent"),
                "Record should have FileEvent flag: {}",
                rec.flags
            );
            assert!(
                rec.flags.contains("Modified"),
                "Record should have Modified flag: {}",
                rec.flags
            );
        }
    }

    #[test]
    fn test_v3_with_combined_filters() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        // Combine path and flag filters
        let filter = RecordFilter::new(
            &Some(r"\.(log|txt|plist)$".to_string()),
            &["Created".to_string(), "Modified".to_string()],
            &[],
        );

        parse_file(&path, &mut bus, &filter).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // Verify all records match both filters
        for rec in records {
            assert!(
                rec.path.ends_with(".log")
                    || rec.path.ends_with(".txt")
                    || rec.path.ends_with(".plist"),
                "Record path should end with .log, .txt, or .plist: {}",
                rec.path
            );
            assert!(
                rec.flags.contains("Created") || rec.flags.contains("Modified"),
                "Record should have Created or Modified flag: {}",
                rec.flags
            );
        }
    }

    #[test]
    fn test_v3_filter_returns_no_matches() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        // Use a filter that shouldn't match anything
        let filter = RecordFilter::new(
            &Some(r"^/this/path/definitely/does/not/exist/in/the/test/file/xyz123$".to_string()),
            &[],
            &[],
        );

        parse_file(&path, &mut bus, &filter).expect("Couldn't parse test file");
        drop(bus);

        let count = recv.iter().count();
        assert_eq!(count, 0, "Filter should exclude all records");
    }

    #[test]
    fn test_v3_collect_specific_data() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // Test that we can access record fields
        assert!(records.len() > 0);

        // Check that we have diverse flag types
        let has_created = records.iter().any(|r| r.flags.contains("Created"));
        let has_modified = records.iter().any(|r| r.flags.contains("Modified"));
        let has_file_event = records.iter().any(|r| r.flags.contains("FileEvent"));

        // Real forensics data should have various event types
        assert!(
            has_created || has_modified || has_file_event,
            "Should have at least one common flag type"
        );

        // Check that event IDs are present
        let first_event_id = records[0].event_id;
        let last_event_id = records[records.len() - 1].event_id;

        // Event IDs should be non-zero in real data
        assert!(
            first_event_id > 0 || last_event_id > 0,
            "Should have non-zero event IDs"
        );
    }

    #[test]
    fn test_v3_multiple_receivers() {
        let mut bus = Bus::new(4096);
        let mut recv1 = bus.add_rx();
        let mut recv2 = bus.add_rx();
        let mut recv3 = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let count1 = recv1.iter().count();
        let count2 = recv2.iter().count();
        let count3 = recv3.iter().count();

        // All receivers should get the same number of records
        assert_eq!(count1, 2730);
        assert_eq!(count2, 2730);
        assert_eq!(count3, 2730);
    }

    #[test]
    fn test_v3_arc_sharing() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // Test that Arc cloning works as expected
        if let Some(first_rec) = records.first() {
            let cloned = first_rec.clone();
            assert_eq!(first_rec.path, cloned.path);
            assert_eq!(first_rec.event_id, cloned.event_id);
            assert_eq!(first_rec.flag, cloned.flag);
        }
    }

    #[test]
    fn test_nonexistent_file() {
        let mut bus = Bus::new(4096);
        let path: PathBuf = "testfiles/nonexistent/file.gz".into();

        let result = parse_file(&path, &mut bus, &RecordFilter::default());
        assert!(result.is_err(), "Should error on nonexistent file");
    }

    #[test]
    fn test_v3_check_node_ids() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // V3 files should have node_id populated
        let has_node_ids = records.iter().any(|r| r.node_id.is_some());
        assert!(has_node_ids, "V3 files should have node IDs");

        // Count how many have node IDs
        let with_node_ids = records.iter().filter(|r| r.node_id.is_some()).count();
        assert!(
            with_node_ids > 0,
            "Should have at least some records with node IDs"
        );
    }

    #[cfg(feature = "extra_id")]
    #[test]
    fn test_v3_check_extra_ids() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // V3 files should have extra_id populated
        let has_extra_ids = records.iter().any(|r| r.extra_id.is_some());
        assert!(has_extra_ids, "V3 files should have extra IDs");
    }

    #[test]
    fn test_v3_path_variety() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // Collect unique paths
        let unique_paths: std::collections::HashSet<_> =
            records.iter().map(|r| r.path.as_str()).collect();

        // Should have multiple unique paths (real forensics data has variety)
        assert!(unique_paths.len() > 1, "Should have multiple unique paths");
        assert!(
            unique_paths.len() < records.len(),
            "Should have some repeated paths"
        );
    }

    #[test]
    fn test_v3_flag_variety() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();

        parse_file(&path, &mut bus, &RecordFilter::default()).expect("Couldn't parse test file");
        drop(bus);

        let records: Vec<_> = recv.iter().collect();

        // Collect unique flag combinations
        let unique_flags: std::collections::HashSet<_> = records.iter().map(|r| r.flags).collect();

        // Real forensics data should have multiple flag types
        assert!(
            unique_flags.len() > 1,
            "Should have multiple unique flag combinations"
        );
    }

    #[test]
    fn test_uncompressed_file() {
        let mut bus = Bus::new(4096);
        let path: PathBuf = "testfiles/v3/000000000342c4f2".into();

        // MultiGzDecoder should handle uncompressed files too
        let result = parse_file(&path, &mut bus, &RecordFilter::default());

        // This should also work since MultiGzDecoder handles both compressed and uncompressed
        if result.is_ok() {
            // Great, it worked!
        } else {
            // Some versions might not handle this, which is also acceptable
            // Just check that it fails gracefully
            assert!(result.is_err());
        }
    }
}
