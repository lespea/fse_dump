use std::{
    fs::File,
    io::{self, prelude::*, BufReader, ErrorKind},
    path::PathBuf,
    sync::Arc,
};

use bus::Bus;
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::MultiGzDecoder;

use crate::{record::Record, version};

pub fn parse_file(in_file: PathBuf, bus: &mut Bus<Arc<Record>>) -> io::Result<()> {
    let mut reader = BufReader::new(MultiGzDecoder::new(File::open(in_file)?));

    loop {
        debug!("starting loop");
        let v = match version::Version::from_reader(&mut reader) {
            Err(e) => {
                if e.kind() == ErrorKind::UnexpectedEof {
                    debug!("eof");
                    break;
                }

                return Err(e);
            }

            Ok(Some(v)) => v,

            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Unsupported type",
                ));
            }
        };
        let parse_fun = v.get_parser();

        reader.read_exact(&mut [0u8; 4])?;
        let p_len = reader.read_u32::<LittleEndian>()? as usize;

        debug!("{:?} :: {}", v, p_len);

        let mut read = 12usize;

        loop {
            let rec = match parse_fun(&mut reader)? {
                None => break,
                Some((s, rec)) => {
                    debug!("Read {} bits", s);
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
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Length of page records didn't match expected length",
                    ));
                }
            }
        }
    }

    Ok(())
}
