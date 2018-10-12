#[macro_use]
extern crate enum_display_derive;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate option_set;
#[macro_use]
extern crate bitflags;

extern crate byteorder;
//extern crate csv;
extern crate flate2;
extern crate serde;
extern crate serde_json;

use std::borrow::Cow;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use flate2::read::MultiGzDecoder;

const V1: [u8; 4] = *b"1SLD";
const V2: [u8; 4] = *b"2SLD";

#[derive(Debug, Display, PartialEq)]
enum Version {
    V1,
    V2,
}

option_set! {
    struct RecordFlags: UpperCamel + u32 {
        const FOLDER_EVENT           = 0x00000001;
        const MOUNT                  = 0x00000002;
        const UNMOUNT                = 0x00000004;
        const END_OF_TRANSACTION     = 0x00000020;
        const LAST_HARD_LINK_REMOVED = 0x00000800;
        const HARD_LINK              = 0x00001000;
        const SYMBOLIC_LINK          = 0x00004000;
        const FILE_EVENT             = 0x00008000;
        const PERMISSION_CHANGE      = 0x00010000;
        const EXTENDED_ATTR_MODIFIED = 0x00020000;
        const EXTENDED_ATTR_REMOVED  = 0x00040000;
        const DOCUMENT_REVISIONING   = 0x00100000;
        const ITEM_CLONED            = 0x00400000;
        const CREATED                = 0x01000000;
        const REMOVED                = 0x02000000;
        const INODE_META_MOD         = 0x04000000;
        const RENAMED                = 0x08000000;
        const MODIFIED               = 0x10000000;
        const EXCHANGE               = 0x20000000;
        const FINDER_INFO_MOD        = 0x40000000;
        const FOLDER_CREATED         = 0x80000000;
    }
}

#[derive(Debug, Serialize)]
struct Record<'a> {
    path: Cow<'a, str>,
    event_id: u64,
    flags: RecordFlags,
    node_id: Option<u64>,
}

impl<'a> Record<'a> {
    fn from_bytes(
        reader: &mut BufRead,
        sbuf: &'a mut Vec<u8>,
        read: &mut usize,
        is_v2: bool,
    ) -> io::Result<Option<Record<'a>>> {
        sbuf.clear();
        let rlen = reader.read_until(b'\0', sbuf)?;
        if rlen == 0 || sbuf[rlen - 1] != b'\0' {
            println!("End of pages discovered :: {}", rlen);
            Ok(None)
        } else {
            let event_id = reader.read_u64::<BigEndian>()?;

            let mut flags = RecordFlags::default();
            flags.bits = reader.read_u32::<BigEndian>()?;

            Ok(Some(Record {
                path: String::from_utf8_lossy(&sbuf[..rlen - 1]),
                event_id,
                flags,
                node_id: if is_v2 {
                    *read += rlen + 20;
                    Some(reader.read_u64::<LittleEndian>()?)
                } else {
                    *read += rlen + 12;
                    None
                },
            }))
        }
    }
}

fn main() -> io::Result<()> {
    let fh = File::open("/home/adam/t/0000000001df1b9b")?;
    let mut gread = BufReader::new(MultiGzDecoder::new(fh));
    //    let mut gread = BufReader::new(File::open("/home/adam/t/fse")?);

    let mut header = [0u8; 4];
    //    let mut c = csv::Writer::from_path("records.csv")?;
    let j = File::create("records.json")?;
    let mut sbuf = Vec::with_capacity(1_000);

    loop {
        println!("starting loop");
        match gread.read_exact(&mut header) {
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    println!("eof");
                    break;
                } else {
                    return Err(e);
                }
            }

            _ => (),
        }

        let v = match header {
            V1 => Version::V1,
            V2 => Version::V2,
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unsupported type",
                ));
            }
        };
        let is_v2 = v == Version::V2;

        gread.read_exact(&mut [0u8; 4])?;
        let p_len = gread.read_u32::<LittleEndian>()? as usize;

        println!("{:?} ({}) :: {}", header, v, p_len);

        //        let t = gread.take(u64::from(p_len) - 12);
        //        let mut lr = BufReader::new(t);
        let mut read = 12usize;

        while let Some(r) = Record::from_bytes(&mut gread, &mut sbuf, &mut read, is_v2)? {
            //            c.serialize(r)?;
            serde_json::ser::to_writer(&j, &r)?;
            writeln!(&j);

            if read == p_len {
                println!("Wanted len");
                break;
            }
        }
    }

    Ok(())
}
