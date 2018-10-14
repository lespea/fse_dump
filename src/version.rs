extern crate byteorder;
extern crate serde;
extern crate serde_json;

use crate::{flags, record::Record};

use std::io::{self, prelude::*};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

const V1_BYTES: &[u8; 4] = b"1SLD";
const V2_BYTES: &[u8; 4] = b"2SLD";

#[derive(Debug, PartialEq)]
struct V1;
#[derive(Debug, PartialEq)]
struct V2;

#[derive(Debug, PartialEq)]
enum Version {
    Ver1(V1),
    Ver2(V2),
}

trait RecordParser<'a> {
    const HAS_NODEID: bool;

    fn parse_record(
        &self,
        reader: &mut BufRead,
        sbuf: &'a mut Vec<u8>,
        rec: &'a mut Record<'a>,
    ) -> io::Result<Option<usize>> {
        sbuf.clear();
        let rlen = reader.read_until(b'\0', sbuf)?;
        if rlen == 0 || sbuf[rlen - 1] != b'\0' {
            println!("End of pages discovered :: {}", rlen);
            Ok(None)
        } else {
            rec.path = String::from_utf8_lossy(&sbuf[..rlen - 1]);
            rec.event_id = reader.read_u64::<BigEndian>()?;
            rec.flags = flags::parse_bits(reader.read_u32::<BigEndian>()?);

            if Self::HAS_NODEID {
                rec.node_id = Some(reader.read_u64::<LittleEndian>()?);
                Ok(Some(rlen + 20))
            } else {
                Ok(Some(rlen + 12))
            }
        }
    }
}

impl<'a> RecordParser<'a> for V1 {
    const HAS_NODEID: bool = false;
}

impl<'a> RecordParser<'a> for V2 {
    const HAS_NODEID: bool = true;
}

impl Version {
    pub fn from_reader(reader: &mut BufRead) -> io::Result<Option<Version>> {
        let mut b = [0u8; 4];
        reader.read_exact(&mut b)?;
        match &b {
            V1_BYTES => Ok(Some(Version::Ver1(V1))),
            V2_BYTES => Ok(Some(Version::Ver2(V2))),
            _ => Ok(None),
        }
    }

    pub fn parse_record<'a>(
        &self,
        reader: &mut BufRead,
        sbuf: &'a mut Vec<u8>,
        rec: &'a mut Record<'a>,
    ) -> io::Result<Option<usize>> {
        match self {
            Version::Ver1(v) => v.parse_record(reader, sbuf, rec),
            Version::Ver2(v) => v.parse_record(reader, sbuf, rec),
        }
    }
}
