use std::io::{self, prelude::*};

use byteorder::{BigEndian, LittleEndian, NativeEndian, ReadBytesExt};

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
    Ver1,
    Ver2,
    Ver3,
}

pub type ParseRet = io::Result<Option<(usize, Record)>>;

trait RecordParser<I>
where
    I: BufRead,
{
    const HAS_NODEID: bool;
    const HAS_UNKNOWN_NUM: bool;

    #[inline]
    fn parse_record(reader: &mut I) -> ParseRet {
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

            let mut tlen = rlen + 8 + 4; // u64 + u32

            let node_id = if Self::HAS_NODEID {
                tlen += 8;
                Some(reader.read_u64::<LittleEndian>()?)
            } else {
                None
            };

            // V3 contains an as-of-now unknown extra 4-bytes; skip them for now
            if Self::HAS_UNKNOWN_NUM {
                tlen += 4;
                let _ = reader.read_u32::<NativeEndian>()?;
            }

            Ok(Some((
                tlen,
                Record {
                    path,
                    event_id,
                    flag,
                    flags,
                    node_id,
                },
            )))
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

impl Version {
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
