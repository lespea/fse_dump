#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate structopt;

#[cfg(test)]
extern crate env_logger;

extern crate byteorder;
extern crate csv;
extern crate flate2;
extern crate fnv;
extern crate serde;
extern crate serde_json;
extern crate simple_logger;

mod file_parser;
mod flags;
mod opts;
mod record;
mod version;

use std::fs::File;
use std::io;
use std::sync::{Arc, Mutex};

fn main() -> io::Result<()> {
    simple_logger::init_with_level(log::Level::Info).expect("Couldn't init logger");

    let opts = opts::get_opts();
    if !(opts.csvs || opts.jsons || opts.csv.is_some() || opts.json.is_some()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "You must specify at least one output type!",
        ));
    }

    let mut global_csv = if let Some(p) = opts.csv {
        if opts.parallel && opts.files.len() > 1 {
            Some(file_parser::CsvRecordWriter::Threaded(Arc::new(
                Mutex::new(csv::Writer::from_path(p)?),
            )))
        } else {
            Some(file_parser::CsvRecordWriter::Simple(
                csv::Writer::from_path(p)?,
            ))
        }
    } else {
        None
    };

    let mut global_json = if let Some(p) = opts.json {
        if opts.parallel && opts.files.len() > 1 {
            Some(file_parser::JsonRecordWriter::Threaded(Arc::new(
                Mutex::new(io::BufWriter::new(File::create(p)?)),
            )))
        } else {
            Some(file_parser::JsonRecordWriter::Simple(io::BufWriter::new(
                File::create(p)?,
            )))
        }
    } else {
        None
    };

    let mut buf = Vec::with_capacity(5000);
    for f in opts.files.into_iter() {
        let p = f.to_string_lossy().to_string();
        match file_parser::ParseOpts::for_path(
            f,
            &mut buf,
            opts.csvs.clone(),
            opts.jsons.clone(),
            &mut global_csv,
            &mut global_json,
        )?
        .parse_file()
        {
            Ok(_) => info!("Finished parsing {}", p),
            Err(e) => error!("Couldn't parse '{}': {}", p, e),
        };
    }

    Ok(())
}
