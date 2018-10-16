use record;
use version;

use byteorder::{LittleEndian, ReadBytesExt};
use crossbeam::channel::Sender;
use flate2::read::MultiGzDecoder;

use std::{
    fs::File,
    io::{self, prelude::*, BufReader, BufWriter, ErrorKind},
    path::PathBuf,
};

pub struct ParseOpts<'a> {
    reader: BufReader<MultiGzDecoder<File>>,
    buf: &'a mut Vec<u8>,

    csv_out: Option<csv::Writer<File>>,
    json_out: Option<BufWriter<File>>,

    channel: Option<Sender<record::Record>>,
}

impl<'a> ParseOpts<'a> {
    pub fn for_path(
        in_file: PathBuf,
        buf: &'a mut Vec<u8>,
        single_csv: bool,
        single_json: bool,
        channel: Option<Sender<record::Record>>,
    ) -> io::Result<ParseOpts<'a>> {
        let mut csv_out = None;
        let mut json_out = None;

        if single_csv || single_json {
            if in_file.file_name().is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
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
            buf,

            csv_out,
            json_out,

            channel,
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
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Unsupported type",
                    ))
                }
            };

            self.reader.read_exact(&mut [0u8; 4])?;
            let p_len = self.reader.read_u32::<LittleEndian>()? as usize;

            info!("{:?} :: {}", v, p_len);

            let mut read = 12usize;

            loop {
                let rec = match v.parse_record(&mut self.reader, self.buf)? {
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

                if let Some(ref mut j) = self.json_out {
                    serde_json::ser::to_writer(j.get_mut(), &rec)?;
                    writeln!(j.get_mut())?;
                }

                if let Some(ref chan) = self.channel {
                    chan.send(rec)
                }

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
}
