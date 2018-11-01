use flags;
use record::Record;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

use std::io::{self, prelude::*};

const V1_BYTES: &[u8; 4] = b"1SLD";
const V2_BYTES: &[u8; 4] = b"2SLD";

#[derive(Debug, PartialEq)]
pub struct V1;
#[derive(Debug, PartialEq)]
pub struct V2;

#[derive(Debug, PartialEq)]
pub enum Version {
    Ver1(V1),
    Ver2(V2),
}

trait RecordParser {
    const HAS_NODEID: bool;

    #[inline]
    fn parse_record(&self, reader: &mut BufRead) -> io::Result<Option<(usize, Record)>> {
        let mut sbuf = Vec::with_capacity(1000);
        debug!("Reading path");
        let rlen = reader.read_until(b'\0', &mut sbuf)?;
        if rlen == 0 || sbuf[rlen - 1] != b'\0' {
            info!("End of pages discovered :: {}", rlen);
            Ok(None)
        } else {
            debug!("Reading path done");
            let path = String::from_utf8_lossy(&sbuf[..rlen - 1]).into_owned();
            debug!("Found path {}", path);
            let event_id = reader.read_u64::<BigEndian>()?;
            debug!("Found event id {}", event_id);
            let flag = reader.read_u32::<BigEndian>()?;
            let flags = flags::parse_bits(flag);
            debug!("Found flags {}", flags);

            if Self::HAS_NODEID {
                debug!("In V2");
                let node_id = Some(reader.read_u64::<LittleEndian>()?);
                debug!("Found node id {}", node_id.unwrap());
                Ok(Some((
                    rlen + 20,
                    Record {
                        path,
                        event_id,
                        flag,
                        flags,
                        node_id,
                    },
                )))
            } else {
                debug!("In V1");
                Ok(Some((
                    rlen + 20,
                    Record {
                        path,
                        event_id,
                        flag,
                        flags,
                        node_id: None,
                    },
                )))
            }
        }
    }
}

impl RecordParser for V1 {
    const HAS_NODEID: bool = false;
}

impl RecordParser for V2 {
    const HAS_NODEID: bool = true;
}

impl Version {
    #[inline]
    pub fn from_reader(reader: &mut BufRead) -> io::Result<Option<Version>> {
        let mut b = [0u8; 4];
        reader.read_exact(&mut b)?;
        match &b {
            V1_BYTES => Ok(Some(Version::Ver1(V1))),
            V2_BYTES => Ok(Some(Version::Ver2(V2))),
            _ => Ok(None),
        }
    }

    #[inline]
    pub fn parse_record(&self, reader: &mut BufRead) -> io::Result<Option<(usize, Record)>> {
        match self {
            Version::Ver1(v) => v.parse_record(reader),
            Version::Ver2(v) => v.parse_record(reader),
        }
    }
}
