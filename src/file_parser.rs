use record::Record;
use version;

use bus::Bus;
use byteorder::{LittleEndian, ReadBytesExt};
use csv;
use flate2::read::MultiGzDecoder;
use serde_json;

use std::{
    fs::File,
    io::{self, prelude::*, BufReader, BufWriter, ErrorKind},
    path::PathBuf,
    sync::Arc,
};

pub struct ParseOpts<'a> {
    reader: BufReader<MultiGzDecoder<File>>,

    csv_out: Option<csv::Writer<File>>,
    json_out: Option<BufWriter<File>>,

    bus: Option<&'a mut Bus<Arc<Record>>>,
}

impl<'a> ParseOpts<'a> {
    pub fn for_path(
        in_file: PathBuf,
        single_csv: bool,
        single_json: bool,
        bus: Option<&'a mut Bus<Arc<Record>>>,
    ) -> io::Result<ParseOpts<'a>> {
        let mut csv_out = None;
        let mut json_out = None;

        if single_csv || single_json {
            if in_file.file_name().is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("The input file doesn't have a filename? '{:?}'", in_file),
                ));
            }

            let ext = in_file.extension().and_then(|e| e.to_str());

            if single_csv {
                let ext = ext.map_or_else(|| "csv".to_string(), |e| format!("{}.csv", e));
                csv_out = Some(csv::Writer::from_path(in_file.with_extension(ext))?);
            }

            if single_json {
                let ext = ext.map_or_else(|| "csv".to_string(), |e| format!("{}.csv", e));
                json_out = Some(BufWriter::new(File::create(in_file.with_extension(ext))?));
            }
        }

        let reader = BufReader::new(MultiGzDecoder::new(File::open(in_file)?));

        Ok(ParseOpts {
            reader,

            csv_out,
            json_out,

            bus,
        })
    }

    pub fn parse_file(&mut self) -> io::Result<()> {
        loop {
            debug!("starting loop");
            let v = match version::Version::from_reader(&mut self.reader) {
                Err(e) => {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        debug!("eof");
                        break;
                    } else {
                        return Err(e);
                    }
                }

                Ok(Some(v)) => v,

                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unsupported type",
                    ))
                }
            };

            self.reader.read_exact(&mut [0u8; 4])?;
            let p_len = self.reader.read_u32::<LittleEndian>()? as usize;

            debug!("{:?} :: {}", v, p_len);

            let mut read = 12usize;

            loop {
                let rec = match v.parse_record(&mut self.reader)? {
                    None => break,
                    Some((s, rec)) => {
                        debug!("Read {} bits", s);
                        read += s;
                        rec
                    }
                };

                if let Some(ref mut c) = self.csv_out {
                    c.serialize(&rec)?;
                }

                if let Some(ref mut c) = self.json_out {
                    serde_json::to_writer(c, &rec)?;
                }

                if let Some(ref mut b) = self.bus {
                    b.broadcast(Arc::new(rec));
                }

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
}
