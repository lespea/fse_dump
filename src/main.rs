#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate structopt;

extern crate bus;
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

use record::Record;

use bus::Bus;
use env_logger::{Target, WriteStyle};

use log::LevelFilter;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufWriter, Write},
    path::PathBuf,
    sync::Arc,
};

fn is_gz(path: &PathBuf) -> bool {
    match path.extension() {
        None => false,
        Some(e) => e == "gz" || e == "gzip",
    }
}

fn main() -> io::Result<()> {
    let opts = opts::get_opts()?;
    let has_std = opts.validate()?;
    let file_paths: Vec<PathBuf> = opts.real_files().collect();

    let opts::Opts {
        csvs: individual_csvs,
        jsons: individual_jsons,
        csv: csv_path,
        json: json_path,
        uniques: uniq_path,
        ..
    } = opts;

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

    let g_lvl = flate2::Compression::fast();

    crossbeam::scope(|scope| {
        let mut bus: Bus<Arc<Record>> = Bus::new(1000);
        let mut has_bus = false;

        if let Some(p) = csv_path {
            has_bus = true;
            let recv = bus.add_rx();

            scope.spawn(|| {
                let sout = io::stdout();
                let mut writer: csv::Writer<Box<Write>> = if p.to_string_lossy() == "-" {
                    csv::Writer::from_writer(Box::new(sout.lock()))
                } else if is_gz(&p) {
                    csv::Writer::from_writer(Box::new(flate2::write::GzEncoder::new(
                        BufWriter::new(File::create(p).expect("Couldn't create the csv file")),
                        g_lvl,
                    )))
                } else {
                    csv::Writer::from_writer(Box::new(BufWriter::new(
                        File::create(p).expect("Couldn't create the csv file"),
                    )))
                };

                for rec in recv {
                    writer.serialize(rec).expect("Couldn't write to global csv");
                }
            });
        };

        if let Some(p) = json_path {
            has_bus = true;
            let recv = bus.add_rx();

            scope.spawn(|| {
                let sout = io::stdout();
                let mut writer: Box<Write> = if p.to_string_lossy() == "-" {
                    Box::new(sout.lock())
                } else if is_gz(&p) {
                    Box::new(flate2::write::GzEncoder::new(
                        BufWriter::new(File::create(p).expect("Couldn't create the csv file")),
                        g_lvl,
                    ))
                } else {
                    Box::new(BufWriter::new(
                        File::create(p).expect("Couldn't create the csv file"),
                    ))
                };

                for rec in recv {
                    serde_json::to_writer(&mut writer, &rec)
                        .expect("Couldn't write to global json");
                }
            });
        };

        if let Some(p) = uniq_path {
            has_bus = true;
            let recv = bus.add_rx();

            scope.spawn(|| {
                let sout = io::stdout();
                let mut c: csv::Writer<Box<io::Write>> = if p.to_string_lossy() == "-" {
                    csv::Writer::from_writer(Box::new(sout.lock()))
                } else if is_gz(&p) {
                    csv::Writer::from_writer(Box::new(flate2::write::GzEncoder::new(
                        BufWriter::new(
                            File::create(p).expect("Couldn't create the uniques csv file"),
                        ),
                        g_lvl,
                    )))
                } else {
                    csv::Writer::from_writer(Box::new(BufWriter::new(
                        File::create(p).expect("Couldn't create the uniques csv file"),
                    )))
                };

                let mut u = BTreeMap::new();

                for rec in recv {
                    u.entry(rec.path.clone())
                        .or_insert_with(uniques::UniqueCounts::default)
                        .update(rec.flag);
                }

                for (path, v) in u {
                    c.serialize(v.into_unique_out(path))
                        .expect("Error writing the uniques");
                }
            });
        }

        for f in file_paths {
            let path = f.to_string_lossy().into_owned();
            let parser = file_parser::ParseOpts::for_path(
                f,
                individual_csvs,
                individual_jsons,
                if has_bus { Some(&mut bus) } else { None },
            );

            match parser {
                Ok(mut p) => match p.parse_file() {
                    Ok(_) => info!("Finished parsing {}", path),
                    Err(e) => error!("Couldn't parse '{}': {}", path, e),
                },

                Err(e) => error!("Couldn't construct a parser for '{:?}': {}", path, e),
            };
        }
    });

    Ok(())
}
