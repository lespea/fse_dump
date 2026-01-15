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

            if !filter.want(&rec) {
                debug!("Skipping {rec:?} due to the filters");
                continue;
            }

            bus.broadcast(Arc::new(rec));

            if read >= p_len {
                if read == p_len {
                    debug!("Wanted len");
                    break;
                } else {
                    return Err(eyre!("Length of page records didn't match expected length",));
                }
            }
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
}
