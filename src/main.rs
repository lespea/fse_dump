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

//use std::borrow::Cow;
//use std::fs::File;
//use std::io::{self, prelude::*, BufReader};
use std::io;

//use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
//use flate2::read::MultiGzDecoder;

mod record;
mod version;

fn main() -> io::Result<()> {
    //    let fh = File::open("/home/adam/t/0000000001df1b9b")?;
    //    let mut gread = BufReader::new(MultiGzDecoder::new(fh));
    //    //    let mut gread = BufReader::new(File::open("/home/adam/t/fse")?);
    //
    //    let mut header = [0u8; 4];
    //    //    let mut c = csv::Writer::from_path("records.csv")?;
    //    let j = File::create("records.json")?;
    //    let mut sbuf = Vec::with_capacity(1_000);
    //
    //    loop {
    //        println!("starting loop");
    //        match gread.read_exact(&mut header) {
    //            Err(e) => {
    //                if e.kind() == io::ErrorKind::UnexpectedEof {
    //                    println!("eof");
    //                    break;
    //                } else {
    //                    return Err(e);
    //                }
    //            }
    //
    //            _ => (),
    //        }
    //
    //        let v = match header {
    //            V1 => Version::V1,
    //            V2 => Version::V2,
    //            _ => {
    //                return Err(std::io::Error::new(
    //                    std::io::ErrorKind::InvalidData,
    //                    "Unsupported type",
    //                ));
    //            }
    //        };
    //        let is_v2 = v == Version::V2;
    //
    //        gread.read_exact(&mut [0u8; 4])?;
    //        let p_len = gread.read_u32::<LittleEndian>()? as usize;
    //
    //        println!("{:?} ({}) :: {}", header, v, p_len);
    //
    //        //        let t = gread.take(u64::from(p_len) - 12);
    //        //        let mut lr = BufReader::new(t);
    //        let mut read = 12usize;
    //
    //        while let Some(r) = Record::from_bytes(&mut gread, &mut sbuf, &mut read, is_v2)? {
    //            //            c.serialize(r)?;
    //            serde_json::ser::to_writer(&j, &r)?;
    //            writeln!(&j);
    //
    //            if read == p_len {
    //                println!("Wanted len");
    //                break;
    //            }
    //        }
    //    }

    Ok(())
}
