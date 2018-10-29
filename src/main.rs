#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate structopt;

extern crate byteorder;
extern crate crossbeam;
extern crate crossbeam_channel as channel;
extern crate csv;
extern crate env_logger;
extern crate flate2;
extern crate fnv;
extern crate serde;
extern crate serde_json;
extern crate walkdir;

mod file_parser;
mod flags;
mod opts;
mod record;
mod uniques;
mod version;

use env_logger::{Target, WriteStyle};

use log::LevelFilter;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufWriter},
    thread,
};

fn main() -> io::Result<()> {
    let opts = opts::get_opts()?;
    let has_std = opts.validate()?;

    env_logger::Builder::new()
        .filter(
            None,
            if has_std {
                LevelFilter::Info
            } else {
                LevelFilter::Error
            },
        )
        .write_style(WriteStyle::Always)
        .target(Target::Stderr)
        .init();

    let mut buf = Vec::with_capacity(5000);
    let stdout = io::stdout();

    let mut c_writer: Option<csv::Writer<Box<io::Write>>> = if let Some(ref p) = opts.csv {
        if p.to_string_lossy() == "-" {
            Some(csv::Writer::from_writer(Box::new(io::stdout())))
        } else {
            Some(csv::Writer::from_writer(Box::new(
                File::create(p.clone()).expect("Couldn't create the csv file"),
            )))
        }
    } else {
        None
    };

    let mut j_writer: Option<Box<io::Write>> = if let Some(ref p) = opts.json {
        if p.to_string_lossy() == "-" {
            Some(Box::new(io::stdout()))
        } else {
            Some(Box::new(BufWriter::new(
                File::create(p).expect("Couldn't create the json out file"),
            )))
        }
    } else {
        None
    };

    let (send, thr) = if let Some(ref upath) = opts.uniques {
        let (send, recv) = channel::bounded(1000);
        let upath = upath.clone();

        let t = thread::spawn(move || {
            let mut c: csv::Writer<Box<io::Write>> = if upath.to_string_lossy() == "-" {
                csv::Writer::from_writer(Box::new(stdout.lock()))
            } else {
                csv::Writer::from_writer(Box::new(
                    File::create(upath).expect("Couldn't create the uniques csv file"),
                ))
            };

            let mut u = BTreeMap::new();

            for (path, flag) in recv {
                u.entry(path)
                    .or_insert_with(uniques::UniqueCounts::default)
                    .update(flag);
            }

            for (path, v) in u {
                c.serialize(v.into_unique_out(path))
                    .expect("Error writing the uniques");
            }
        });

        (Some(send), Some(t))
    } else {
        (None, None)
    };

    for f in opts.real_files() {
        let path = f.to_string_lossy().to_string();
        let parser = file_parser::ParseOpts::for_path(
            f,
            &mut buf,
            opts.csvs,
            opts.jsons,
            &mut c_writer,
            &mut j_writer,
            &send,
        );

        match parser {
            Ok(mut p) => match p.parse_file() {
                Ok(_) => info!("Finished parsing {}", path),
                Err(e) => error!("Couldn't parse '{}': {}", path, e),
            },

            Err(e) => error!("Couldn't construct a parser for '{:?}': {}", path, e),
        };
    }

    // Close the send channel
    if let Some(_s) = send {}

    if let Some(t) = thr {
        if let Err(e) = t.join() {
            error!("Couldn't join the unique thread: {:?}", e);
        }
    };

    Ok(())
}
