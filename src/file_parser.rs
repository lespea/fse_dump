use version;

use byteorder::{LittleEndian, ReadBytesExt};
use csv;
use flate2::read::MultiGzDecoder;

use std::borrow::BorrowMut;
use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter, ErrorKind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub enum CsvRecordWriter {
    Simple(Box<csv::Writer<File>>),
    Threaded(Arc<Mutex<csv::Writer<File>>>),
}

pub enum JsonRecordWriter {
    Simple(BufWriter<File>),
    Threaded(Arc<Mutex<BufWriter<File>>>),
}

pub struct ParseOpts<'a> {
    reader: BufReader<MultiGzDecoder<File>>,
    buf: &'a mut Vec<u8>,

    csv_out: Option<csv::Writer<File>>,
    json_out: Option<BufWriter<File>>,

    global_csv: &'a mut Option<CsvRecordWriter>,
    global_json: &'a mut Option<JsonRecordWriter>,
}

impl<'a> ParseOpts<'a> {
    pub fn for_path(
        in_file: PathBuf,
        buf: &'a mut Vec<u8>,
        single_csv: bool,
        single_json: bool,
        global_csv: &'a mut Option<CsvRecordWriter>,
        global_json: &'a mut Option<JsonRecordWriter>,
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
                let ext = ext.map_or_else(|| ".csv".to_string(), |e| format!("{}.csv", e));
                csv_out = Some(csv::Writer::from_path(in_file.with_extension(ext))?);
            }

            if single_json {
                let ext = ext.map_or_else(|| ".csv".to_string(), |e| format!("{}.csv", e));
                json_out = Some(BufWriter::new(File::create(in_file.with_extension(ext))?));
            }
        }

        let reader = BufReader::new(MultiGzDecoder::new(File::open(in_file)?));

        Ok(ParseOpts {
            reader,
            buf,

            csv_out,
            json_out,

            global_csv,
            global_json,
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

                if let Some(ref mut c) = self.global_csv {
                    match c {
                        CsvRecordWriter::Simple(c) => c.serialize(&rec)?,

                        CsvRecordWriter::Threaded(c) => c
                            .lock()
                            .expect("Couldn't lock the combined csv writer?")
                            .serialize(&rec)?,
                    }
                }

                if let Some(ref mut c) = self.global_json {
                    match c {
                        JsonRecordWriter::Simple(j) => {
                            serde_json::ser::to_writer(j.get_mut(), &rec)?;
                            writeln!(j.get_mut());
                        }

                        JsonRecordWriter::Threaded(j) => {
                            let mut j = j.lock().expect("Couldn't lock the combined csv writer?");
                            let j = j.borrow_mut();
                            serde_json::ser::to_writer(j.get_mut(), &rec)?;
                            writeln!(j.get_mut())?;
                        }
                    }
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
