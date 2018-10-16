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
extern crate crossbeam;
extern crate csv;
extern crate flate2;
extern crate fnv;
extern crate num_cpus;
extern crate serde;
extern crate serde_json;
extern crate simple_logger;

mod file_parser;
mod flags;
mod opts;
mod record;
mod version;

use crossbeam::channel;

use std::{
    fs::File,
    io::{self, BufWriter},
};

fn main() -> io::Result<()> {
    simple_logger::init_with_level(log::Level::Info).expect("Couldn't init logger");

    let opts = opts::get_opts();
    if !(opts.csvs || opts.jsons || opts.csv.is_some() || opts.json.is_some()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "You must specify at least one output type!",
        ));
    }

    let do_parallel = opts.parallel && opts.files.len() > 1;

    let c_writer = if let Some(ref p) = opts.csv {
        Some(csv::Writer::from_path(p)?)
    } else {
        None
    };

    let j_writer = if let Some(ref p) = opts.json {
        Some(BufWriter::new(File::create(p)?))
    } else {
        None
    };

    crossbeam::scope(move |scope| {
        let rec_send = if c_writer.is_some() || j_writer.is_some() {
            let (send, recv) = channel::bounded::<record::Record>(5000);

            scope.spawn(move || match (c_writer, j_writer) {
                (Some(mut c), Some(mut j)) => {
                    for r in recv {
                        c.serialize(&r).expect("Error writing to the csv");
                        serde_json::ser::to_writer(&mut j, &r).expect("Error writing the json");
                    }
                }

                (Some(mut c), None) => {
                    for r in recv {
                        c.serialize(&r).expect("Error writing to the csv");
                    }
                }

                (None, Some(mut j)) => {
                    for r in recv {
                        serde_json::ser::to_writer(&mut j, &r).expect("Error writing the json");
                    }
                }

                _ => (),
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

            for f in opts.files.into_iter() {
                path_send.send(f);
            }
            drop(path_send);
        } else {
            let mut buf = Vec::with_capacity(5000);
            for f in opts.files.into_iter() {
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
    });

    Ok(())
}
