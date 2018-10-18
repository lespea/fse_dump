#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate structopt;

extern crate byteorder;
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
    collections::HashMap,
    fs::File,
    io::{self, BufWriter},
};

fn main() -> io::Result<()> {
    env_logger::Builder::new()
        .filter(None, LevelFilter::Info)
        .write_style(WriteStyle::Always)
        .target(Target::Stderr)
        .init();

    let opts = opts::get_opts()?;

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

    let mut uniques = if opts.uniques.is_some() {
        Some(HashMap::with_capacity(10_000))
    } else {
        None
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
            &mut uniques,
        );

        match parser {
            Ok(mut p) => match p.parse_file() {
                Ok(_) => info!("Finished parsing {}", path),
                Err(e) => error!("Couldn't parse '{}': {}", path, e),
            },

            Err(e) => error!("Couldn't construct a parser for '{:?}': {}", path, e),
        };
    }

    if let Some(u) = uniques {
        if !u.is_empty() {
            if let Some(p) = opts.uniques {
                let mut c: csv::Writer<Box<io::Write>> = if p.to_string_lossy() == "-" {
                    csv::Writer::from_writer(Box::new(stdout.lock()))
                } else {
                    csv::Writer::from_writer(Box::new(
                        File::create(p).expect("Couldn't create the uniques csv file"),
                    ))
                };

                let mut keys: Vec<&String> = u.keys().collect();
                keys.sort_unstable_by_key(|ref k| k.to_lowercase());

                for path in keys.into_iter() {
                    let mut v = &u[path];
                    let uo = v.to_unique_out(path.as_ref());
                    c.serialize(uo).expect("Error writing the uniques");
                }
            };
        }
    }

    Ok(())
}
