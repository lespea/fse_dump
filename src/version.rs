use std::io::{self, prelude::*};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};

use crate::{flags, record::Record};

const V1_BYTES: &[u8; 4] = b"1SLD";
const V2_BYTES: &[u8; 4] = b"2SLD";
const V3_BYTES: &[u8; 4] = b"3SLD";

#[derive(Debug, Eq, PartialEq)]
pub struct V1;

#[derive(Debug, Eq, PartialEq)]
pub struct V2;

#[derive(Debug, Eq, PartialEq)]
pub struct V3;

#[derive(Debug, Eq, PartialEq)]
pub enum Version {
    Ver1(V1),
    Ver2(V2),
    Ver3(V3),
}

trait RecordParser<I>
where
    I: BufRead,
{
    const IS_V3: bool;
    const IS_V2: bool;
    const IS_V1: bool;

    #[inline]
    fn parse_record(&self, reader: &mut I) -> io::Result<Option<(usize, Record)>> {
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

            if Self::IS_V3 {
                debug!("In V3");
                let node_id = reader.read_u64::<LittleEndian>()?;
                debug!("Found node id {}", node_id);
                Ok(Some((
                    // V3 contains an as-of-now unknown extra 4-bytes; skip them for now
                    rlen + 24,
                    Record {
                        path,
                        event_id,
                        flag,
                        flags,
                        node_id: Some(node_id),
                    },
                )))
            } else if Self::IS_V2 {
                debug!("In V2");
                let node_id = reader.read_u64::<LittleEndian>()?;
                debug!("Found node id {}", node_id);
                Ok(Some((
                    rlen + 20,
                    Record {
                        path,
                        event_id,
                        flag,
                        flags,
                        node_id: Some(node_id),
                    },
                )))
            } else if Self::IS_V1 {
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
            } else {
                unreachable!()
            }
        }
    }
}

impl<I> RecordParser<I> for V1
where
    I: BufRead,
{
    const IS_V1: bool = true;
    const IS_V2: bool = false;
    const IS_V3: bool = false;
}

impl<I> RecordParser<I> for V2
where
    I: BufRead,
{
    const IS_V1: bool = false;
    const IS_V2: bool = true;
    const IS_V3: bool = false;
}

impl<I> RecordParser<I> for V3
where
    I: BufRead,
{
    const IS_V1: bool = false;
    const IS_V2: bool = false;
    const IS_V3: bool = true;
}

impl Version {
    #[inline]
    pub fn from_reader<I>(reader: &mut I) -> io::Result<Option<Version>>
    where
        I: BufRead,
    {
        let mut b = [0u8; 4];
        reader.read_exact(&mut b)?;
        match &b {
            V1_BYTES => Ok(Some(Version::Ver1(V1))),
            V2_BYTES => Ok(Some(Version::Ver2(V2))),
            V3_BYTES => Ok(Some(Version::Ver3(V3))),
            _ => Ok(None),
        }
    }

    #[inline]
    pub fn parse_record<I>(&self, reader: &mut I) -> io::Result<Option<(usize, Record)>>
    where
        I: BufRead,
    {
        match self {
            Version::Ver1(v) => v.parse_record(reader),
            Version::Ver2(v) => v.parse_record(reader),
            Version::Ver3(v) => v.parse_record(reader),
        }
    }
}
