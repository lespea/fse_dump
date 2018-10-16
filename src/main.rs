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

use std::io;

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

    if do_parallel {
        use crossbeam::channel;

        let (send, recv) = channel::bounded::<std::path::PathBuf>(100);

        let single_csv = opts.csvs;
        let single_json = opts.jsons;

        crossbeam::scope(|scope| {
            for _ in 0..num_cpus::get() {
                let recv = recv.clone();

                scope.spawn(move || {
                    let mut buf = Vec::with_capacity(5000);

                    for f in recv {
                        let path = f.to_string_lossy().to_string();
                        let parser = file_parser::ParseOpts::for_path(
                            f,
                            &mut buf,
                            single_csv,
                            single_json,
                            None,
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
                send.send(f);
            }
            drop(send);
        })
    } else {
        let mut buf = Vec::with_capacity(5000);
        for f in opts.files.into_iter() {
            let p = f.to_string_lossy().to_string();
            let result =
                file_parser::ParseOpts::for_path(f, &mut buf, opts.csvs, opts.jsons, None)?
                    .parse_file();

            match result {
                Ok(_) => info!("Finished parsing {}", p),
                Err(e) => error!("Couldn't parse '{}': {}", p, e),
            };
        }
    }

    Ok(())
}
