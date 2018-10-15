#![feature(nll)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

//extern crate byteorder;
//extern crate csv;
//extern crate flate2;
//extern crate fnv;
//extern crate serde;
//extern crate serde_json;

//use std::borrow::Cow;
use std::fs::File;
//use std::io;
use std::io::{self, prelude::*, BufReader};

use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::MultiGzDecoder;

mod flags;
mod record;
mod version;

fn main() -> io::Result<()> {
    simple_logger::init_with_level(log::Level::Debug).expect("Couldn't init logger");

    let fh = File::open("/home/adam/t/0000000001df1b9b")?;
    let mut gread = BufReader::new(MultiGzDecoder::new(fh));
    //    let mut gread = BufReader::new(File::open("/home/adam/t/fse")?);

    let mut c = csv::Writer::from_path("records.csv")?;
    let j = File::create("records.json")?;

    let mut sbuf = Vec::with_capacity(1_000);

    loop {
        debug!("starting loop");
        let v = match version::Version::from_reader(&mut gread) {
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    debug!("eof");
                    break;
                } else {
                    return Err(e);
                }
            }

            Ok(Some(v)) => v,

            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unsupported type",
                ))
            }
        };

        gread.read_exact(&mut [0u8; 4])?;
        let p_len = gread.read_u32::<LittleEndian>()? as usize;

        info!("{:?} :: {}", v, p_len);

        let mut read = 12usize;

        loop {
            let rec = match v.parse_record(&mut gread, &mut sbuf)? {
                None => break,
                Some((s, rec)) => {
                    info!("Read {} bits", s);
                    read += s;
                    rec
                }
            };

            serde_json::ser::to_writer(&j, &rec)?;
            writeln!(&j);
            c.serialize(&rec)?;

            if read >= p_len {
                if read == p_len {
                    debug!("Wanted len");
                    break;
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Length of page records didn't match expected length",
                    ));
                }
            }
        }
    }

    Ok(())
}
