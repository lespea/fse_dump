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
extern crate csv;
extern crate env_logger;
extern crate flate2;
extern crate fnv;
extern crate num_cpus;
extern crate serde;
extern crate serde_json;
extern crate walkdir;

mod file_parser;
mod flags;
mod opts;
mod record;
mod uniques;
mod version;

use crossbeam::channel;
use env_logger::{Target, WriteStyle};

use log::LevelFilter;
use std::{
    collections::BTreeMap,
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
    let do_parallel = opts.parallel && opts.files.len() > 1;

    crossbeam::scope(move |scope| {
        let rec_send = if opts.csv.is_some() || opts.json.is_some() || opts.uniques.is_some() {
            let (send, recv) = channel::bounded::<record::Record>(5000);

            let csv_path = opts.csv.clone();
            let json_path = opts.json.clone();
            let uniq_path = opts.uniques.clone();

            scope.spawn(move || {
                let stdout = io::stdout();

                let mut c_writer: Option<csv::Writer<Box<io::Write>>> = if let Some(p) = csv_path {
                    if p.to_string_lossy() == "-" {
                        Some(csv::Writer::from_writer(Box::new(stdout.lock())))
                    } else {
                        Some(csv::Writer::from_writer(Box::new(
                            File::create(p.clone()).expect("Couldn't create the csv file"),
                        )))
                    }
                } else {
                    None
                };

                let u_writer: Option<csv::Writer<Box<io::Write>>> = if let Some(p) = uniq_path {
                    if p.to_string_lossy() == "-" {
                        Some(csv::Writer::from_writer(Box::new(stdout.lock())))
                    } else {
                        Some(csv::Writer::from_writer(Box::new(
                            File::create(p).expect("Couldn't create the uniques csv file"),
                        )))
                    }
                } else {
                    None
                };

                let mut j_writer: Option<Box<io::Write>> = if let Some(p) = json_path {
                    if p.to_string_lossy() == "-" {
                        Some(Box::new(stdout.lock()))
                    } else {
                        Some(Box::new(BufWriter::new(
                            File::create(p).expect("Couldn't create the json out file"),
                        )))
                    }
                } else {
                    None
                };

                let want_uniques = u_writer.is_some();
                let mut uniques = BTreeMap::new();

                for r in recv {
                    if let Some(ref mut c) = c_writer {
                        c.serialize(&r).expect("Error writing to the csv");
                    };

                    if let Some(ref mut j) = j_writer {
                        serde_json::ser::to_writer(j, &r).expect("Error writing the json");
                    };

                    if want_uniques {
                        uniques
                            .entry(r.path)
                            .or_insert_with(uniques::UniqueCounts::default)
                            .update(r.flag)
                    }
                }

                if !uniques.is_empty() {
                    if let Some(mut c) = u_writer {
                        for (path, entry) in uniques {
                            c.serialize(entry.into_unique_out(path))
                                .expect("Error writing the uniques");
                        }
                    }
                }
            });

            Some(send)
        } else {
            None
        };

        if do_parallel {
            let (path_send, path_recv) = channel::bounded::<std::path::PathBuf>(100);

            let single_csv = opts.csvs;
            let single_json = opts.jsons;

            for _ in 0..num_cpus::get() {
                let recv = path_recv.clone();
                let rec_send = rec_send.clone();

                scope.spawn(move || {
                    let mut buf = Vec::with_capacity(5000);

                    for f in recv {
                        let path = f.to_string_lossy().to_string();

                        let parser = file_parser::ParseOpts::for_path(
                            f,
                            &mut buf,
                            single_csv,
                            single_json,
                            rec_send.clone(),
                        );

                        match parser {
                            Ok(mut p) => match p.parse_file() {
                                Ok(_) => info!("Finished parsing {:?}", path),
                                Err(e) => error!("Couldn't parse '{:?}': {}", path, e),
                            },

                            Err(e) => error!("Couldn't construct a parser for '{:?}': {}", path, e),
                        }
                    }
                });
            }

            for f in opts.real_files() {
                path_send.send(f);
            }
            drop(path_send);
        } else {
            let mut buf = Vec::with_capacity(5000);
            for f in opts.real_files() {
                let path = f.to_string_lossy().to_string();
                let parser = file_parser::ParseOpts::for_path(
                    f,
                    &mut buf,
                    opts.csvs,
                    opts.jsons,
                    rec_send.clone(),
                );

                match parser {
                    Ok(mut p) => match p.parse_file() {
                        Ok(_) => info!("Finished parsing {}", path),
                        Err(e) => error!("Couldn't parse '{}': {}", path, e),
                    },

                    Err(e) => error!("Couldn't construct a parser for '{:?}': {}", path, e),
                };
            }
        }

        drop(rec_send);
    });

    Ok(())
}
