extern crate byteorder;
extern crate serde;
extern crate serde_json;

use crate::record::Record;

use std::fmt::Display;
use std::io::{self, prelude::*};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

const V1: &[u8; 4] = b"1SLD";
const V2: &[u8; 4] = b"2SLD";

#[derive(Debug, Display, PartialEq)]
enum Version {
    V1,
    V2,
}

trait RecordParser<'a> {
    #[inline]
    fn has_nodeid(&self) -> bool;

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
            rec.set_flags(reader.read_u32::<BigEndian>()?);

            if self.has_nodeid() {
                rec.node_id = Some(reader.read_u64::<LittleEndian>()?);
                Ok(Some(rlen + 20))
            } else {
                Ok(Some(rlen + 12))
            }
        }
    }
}
