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

use crate::{record::Record, version};

pub fn parse_file(in_file: &Path, bus: &mut Bus<Arc<Record>>) -> Result<()> {
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

            _ => {
                return Err(eyre!("Unsupported type",));
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

    use super::parse_file;

    #[test]
    fn test_v3() {
        let mut bus = Bus::new(4096);
        let mut recv = bus.add_rx();

        let path: PathBuf = "testfiles/v3/test_1.gz".into();
        parse_file(&path, &mut bus).expect("Couldn't find test file");
        drop(bus);

        let count = recv.iter().count();
        assert_eq!(count, 2730);
    }
}
