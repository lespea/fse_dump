//! FSEvents file format version detection and record parsing
//!
//! This module handles the different versions of FSEvents file formats (V1, V2, V3)
//! and provides parsers to extract records from each version.

use std::io::{self, prelude::*};

use byteorder::{BigEndian, LittleEndian, NativeEndian, ReadBytesExt};

use crate::{flags, record::Record};

/// Magic bytes for FSEvents file format version 1
const V1_BYTES: &[u8; 4] = b"1SLD";
/// Magic bytes for FSEvents file format version 2
const V2_BYTES: &[u8; 4] = b"2SLD";
/// Magic bytes for FSEvents file format version 3
const V3_BYTES: &[u8; 4] = b"3SLD";

/// Marker type for FSEvents file format version 1
pub struct V1;
/// Marker type for FSEvents file format version 2
pub struct V2;
/// Marker type for FSEvents file format version 3
pub struct V3;

/// Represents the different FSEvents file format versions
#[derive(Debug)]
pub enum Version {
    /// FSEvents file format version 1
    Ver1,
    /// FSEvents file format version 2 (includes node ID)
    Ver2,
    /// FSEvents file format version 3 (includes node ID and extra ID)
    Ver3,
}

impl Version {
    /// Attempts to detect the file format version from a reader
    ///
    /// Reads the first 4 bytes to identify the version magic bytes.
    ///
    /// # Arguments
    /// * `reader` - Buffered reader positioned at the start of a version header
    ///
    /// # Returns
    /// - `Ok(Some(Version))` if a known version is detected
    /// - `Ok(None)` if the magic bytes don't match any known version
    /// - `Err` if there's an I/O error
    #[inline]
    pub fn from_reader<I>(reader: &mut I) -> io::Result<Option<Version>>
    where
        I: BufRead,
    {
        let mut b = [0u8; 4];
        reader.read_exact(&mut b)?;
        match &b {
            V1_BYTES => Ok(Some(Version::Ver1)),
            V2_BYTES => Ok(Some(Version::Ver2)),
            V3_BYTES => Ok(Some(Version::Ver3)),
            _ => Ok(None),
        }
    }

    /// Returns the appropriate parser function for this version
    ///
    /// # Returns
    /// A function pointer to the version-specific record parser
    #[inline]
    pub fn get_parser<I>(&self) -> fn(reader: &mut I) -> ParseRet
    where
        I: BufRead,
    {
        match self {
            Version::Ver1 => V1::parse_record,
            Version::Ver2 => V2::parse_record,
            Version::Ver3 => V3::parse_record,
        }
    }
}

impl<I> RecordParser<I> for V1
where
    I: BufRead,
{
    const HAS_NODEID: bool = false;
    const HAS_UNKNOWN_NUM: bool = false;
}

impl<I> RecordParser<I> for V2
where
    I: BufRead,
{
    const HAS_NODEID: bool = true;
    const HAS_UNKNOWN_NUM: bool = false;
}

impl<I> RecordParser<I> for V3
where
    I: BufRead,
{
    const HAS_NODEID: bool = true;
    const HAS_UNKNOWN_NUM: bool = true;
}

/// Result type for record parsing operations
///
/// Returns:
/// - `Ok(Some((bytes_read, record)))` on successful parse
/// - `Ok(None)` when end of records is reached
/// - `Err` on I/O or parsing error
pub type ParseRet = io::Result<Option<(usize, Record)>>;

trait RecordParser<I>
where
    I: BufRead,
{
    const HAS_NODEID: bool;
    const HAS_UNKNOWN_NUM: bool;

    fn parse_record(reader: &mut I) -> ParseRet {
        let mut sbuf = Vec::with_capacity(128);
        debug!("Reading path");
        let rlen = reader.read_until(b'\0', &mut sbuf)?;
        if rlen == 0 || sbuf[rlen - 1] != b'\0' {
            debug!("End of pages discovered :: {rlen}");
            Ok(None)
        } else {
            debug!("Reading path done");

            let path = String::from_utf8_lossy(&sbuf[..rlen - 1]).into_owned();
            debug!("Found path {path}");

            let event_id = reader.read_u64::<BigEndian>()?;
            debug!("Found event id {event_id}");

            let flag = reader.read_u32::<BigEndian>()?;
            let flags = flags::parse_bits(flag);
            debug!("Found flags {flags:?}");

            let mut tlen = rlen + 8 + 4; // u64 + u32

            let node_id = if Self::HAS_NODEID {
                tlen += 8;
                Some(reader.read_u64::<LittleEndian>()?)
            } else {
                None
            };

            // V3 contains an as-of-now unknown extra 4-bytes; skip them for now
            #[allow(unused_variables)]
            let extra_id = if Self::HAS_UNKNOWN_NUM {
                tlen += 4;
                Some(reader.read_u32::<NativeEndian>()?)
            } else {
                None
            };

            Ok(Some((
                tlen,
                Record {
                    path,
                    event_id,
                    flag,
                    flags: flags.norm,
                    #[cfg(feature = "alt_flags")]
                    alt_flags: flags.alt,
                    node_id,
                    #[cfg(feature = "extra_id")]
                    extra_id,
                    file_timestamp: None,
                },
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_version_from_reader_v1() {
        let mut cursor = Cursor::new(b"1SLD");
        let version = Version::from_reader(&mut cursor).expect("Should read version");
        assert!(matches!(version, Some(Version::Ver1)));
    }

    #[test]
    fn test_version_from_reader_v2() {
        let mut cursor = Cursor::new(b"2SLD");
        let version = Version::from_reader(&mut cursor).expect("Should read version");
        assert!(matches!(version, Some(Version::Ver2)));
    }

    #[test]
    fn test_version_from_reader_v3() {
        let mut cursor = Cursor::new(b"3SLD");
        let version = Version::from_reader(&mut cursor).expect("Should read version");
        assert!(matches!(version, Some(Version::Ver3)));
    }

    #[test]
    fn test_version_from_reader_unknown() {
        let mut cursor = Cursor::new(b"4SLD");
        let version = Version::from_reader(&mut cursor).expect("Should read bytes");
        assert!(version.is_none(), "Unknown version should return None");
    }

    #[test]
    fn test_version_from_reader_invalid() {
        let mut cursor = Cursor::new(b"XXXX");
        let version = Version::from_reader(&mut cursor).expect("Should read bytes");
        assert!(version.is_none(), "Invalid magic bytes should return None");
    }

    #[test]
    fn test_version_from_reader_empty() {
        let mut cursor = Cursor::new(b"");
        let result = Version::from_reader(&mut cursor);
        assert!(result.is_err(), "Empty buffer should return error");
    }

    #[test]
    fn test_version_from_reader_short() {
        let mut cursor = Cursor::new(b"1S");
        let result = Version::from_reader(&mut cursor);
        assert!(result.is_err(), "Short buffer should return error");
    }

    #[test]
    fn test_version_get_parser_v1() {
        let version = Version::Ver1;
        let parser = version.get_parser::<Cursor<Vec<u8>>>();

        // Just verify we can get a parser function
        // The actual parsing is tested in integration tests
        assert!(!std::ptr::eq(parser as *const (), std::ptr::null()));
    }

    #[test]
    fn test_version_get_parser_v2() {
        let version = Version::Ver2;
        let parser = version.get_parser::<Cursor<Vec<u8>>>();
        assert!(!std::ptr::eq(parser as *const (), std::ptr::null()));
    }

    #[test]
    fn test_version_get_parser_v3() {
        let version = Version::Ver3;
        let parser = version.get_parser::<Cursor<Vec<u8>>>();
        assert!(!std::ptr::eq(parser as *const (), std::ptr::null()));
    }

    #[test]
    fn test_version_constants() {
        assert_eq!(V1_BYTES, b"1SLD");
        assert_eq!(V2_BYTES, b"2SLD");
        assert_eq!(V3_BYTES, b"3SLD");
    }

    #[test]
    fn test_v1_parser_basic_record() {
        let mut data = Vec::new();
        data.extend_from_slice(b"/test/path\0"); // path with null terminator
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes()); // event_id
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes()); // flag (Modified)

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (size, record) = result.unwrap();
        assert_eq!(record.path, "/test/path");
        assert_eq!(record.event_id, 0x42);
        assert_eq!(record.flag, 0x1000_0000);
        assert!(record.flags.contains("Modified"));
        assert_eq!(record.node_id, None, "V1 should not have node_id");
        assert!(size > 0);
    }

    #[test]
    fn test_v2_parser_basic_record() {
        let mut data = Vec::new();
        data.extend_from_slice(b"/test/path\0");
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes()); // event_id
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes()); // flag
        data.extend_from_slice(&0x0000_0000_0000_1234u64.to_le_bytes()); // node_id (little endian)

        let mut cursor = Cursor::new(data);
        let result = V2::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (size, record) = result.unwrap();
        assert_eq!(record.path, "/test/path");
        assert_eq!(record.event_id, 0x42);
        assert_eq!(record.flag, 0x1000_0000);
        assert_eq!(record.node_id, Some(0x1234), "V2 should have node_id");
        assert!(size > 0);
    }

    #[test]
    fn test_v3_parser_basic_record() {
        let mut data = Vec::new();
        data.extend_from_slice(b"/test/path\0");
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes()); // event_id
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes()); // flag
        data.extend_from_slice(&0x0000_0000_0000_1234u64.to_le_bytes()); // node_id
        data.extend_from_slice(&0x5678u32.to_ne_bytes()); // extra_id (native endian)

        let mut cursor = Cursor::new(data);
        let result = V3::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (size, record) = result.unwrap();
        assert_eq!(record.path, "/test/path");
        assert_eq!(record.event_id, 0x42);
        assert_eq!(record.flag, 0x1000_0000);
        assert_eq!(record.node_id, Some(0x1234));
        #[cfg(feature = "extra_id")]
        assert_eq!(record.extra_id, Some(0x5678), "V3 should have extra_id");
        assert!(size > 0);
    }

    #[test]
    fn test_parser_empty_path() {
        let mut data = Vec::new();
        data.push(b'\0'); // Empty path
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (_, record) = result.unwrap();
        assert_eq!(record.path, "");
    }

    #[test]
    fn test_parser_long_path() {
        let long_path = "/very/long/path/".repeat(50);
        let mut data = Vec::new();
        data.extend_from_slice(long_path.as_bytes());
        data.push(b'\0');
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (_, record) = result.unwrap();
        assert_eq!(record.path, long_path);
    }

    #[test]
    fn test_parser_unicode_path() {
        let unicode_path = "/test/üñíçödé/😀";
        let mut data = Vec::new();
        data.extend_from_slice(unicode_path.as_bytes());
        data.push(b'\0');
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (_, record) = result.unwrap();
        assert_eq!(record.path, unicode_path);
    }

    #[test]
    fn test_parser_end_of_records() {
        let mut cursor = Cursor::new(Vec::new());
        let result = V1::parse_record(&mut cursor).expect("Should handle EOF");
        assert!(result.is_none(), "Empty buffer should return None");
    }

    #[test]
    fn test_parser_multiple_flags() {
        let flags: u32 = 0x1000_0000 | 0x0100_0000 | 0x0000_8000; // Modified | Created | FileEvent
        let mut data = Vec::new();
        data.extend_from_slice(b"/test\0");
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes());
        data.extend_from_slice(&flags.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (_, record) = result.unwrap();
        assert_eq!(record.flag, flags);
        assert!(record.flags.contains("Modified"));
        assert!(record.flags.contains("Created"));
        assert!(record.flags.contains("FileEvent"));
    }

    #[test]
    fn test_parser_zero_event_id() {
        let mut data = Vec::new();
        data.extend_from_slice(b"/test\0");
        data.extend_from_slice(&0u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (_, record) = result.unwrap();
        assert_eq!(record.event_id, 0);
    }

    #[test]
    fn test_parser_max_event_id() {
        let mut data = Vec::new();
        data.extend_from_slice(b"/test\0");
        data.extend_from_slice(&u64::MAX.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (_, record) = result.unwrap();
        assert_eq!(record.event_id, u64::MAX);
    }

    #[test]
    fn test_parser_calculates_size_v1() {
        let path = "/test/path";
        let mut data = Vec::new();
        data.extend_from_slice(path.as_bytes());
        data.push(b'\0');
        data.extend_from_slice(&0x0000_0000_0000_0042u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());

        let expected_size = path.len() + 1 + 8 + 4; // path + null + u64 + u32

        let mut cursor = Cursor::new(data);
        let result = V1::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (size, _) = result.unwrap();
        assert_eq!(size, expected_size);
    }

    #[test]
    fn test_parser_calculates_size_v2() {
        let path = "/test";
        let mut data = Vec::new();
        data.extend_from_slice(path.as_bytes());
        data.push(b'\0');
        data.extend_from_slice(&0x42u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());
        data.extend_from_slice(&0x1234u64.to_le_bytes());

        let expected_size = path.len() + 1 + 8 + 4 + 8; // path + null + u64 + u32 + u64

        let mut cursor = Cursor::new(data);
        let result = V2::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (size, _) = result.unwrap();
        assert_eq!(size, expected_size);
    }

    #[test]
    fn test_parser_calculates_size_v3() {
        let path = "/x";
        let mut data = Vec::new();
        data.extend_from_slice(path.as_bytes());
        data.push(b'\0');
        data.extend_from_slice(&0x42u64.to_be_bytes());
        data.extend_from_slice(&0x1000_0000u32.to_be_bytes());
        data.extend_from_slice(&0x1234u64.to_le_bytes());
        data.extend_from_slice(&0x5678u32.to_ne_bytes());

        let expected_size = path.len() + 1 + 8 + 4 + 8 + 4; // path + null + u64 + u32 + u64 + u32

        let mut cursor = Cursor::new(data);
        let result = V3::parse_record(&mut cursor).expect("Should parse");

        assert!(result.is_some());
        let (size, _) = result.unwrap();
        assert_eq!(size, expected_size);
    }

    #[test]
    fn test_version_debug_format() {
        let v1 = Version::Ver1;
        let v2 = Version::Ver2;
        let v3 = Version::Ver3;

        assert!(format!("{:?}", v1).contains("Ver1"));
        assert!(format!("{:?}", v2).contains("Ver2"));
        assert!(format!("{:?}", v3).contains("Ver3"));
    }
}
