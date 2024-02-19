#![warn(rust_2018_compatibility)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![deny(warnings)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
    sync::{mpsc::RecvTimeoutError, Arc},
    thread,
    time::Duration,
};

use bus::{Bus, BusReader};
use clap::CommandFactory;
use color_eyre::Result;
use csv::Writer;
use env_logger::{Target, WriteStyle};
use log::LevelFilter;
use opts::{Commands, Generate};

use crate::record::Record;

mod file_parser;
mod flags;
mod opts;
mod record;
mod uniques;
mod version;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> Result<()> {
    match opts::get_opts()?.command {
        Commands::Dump(d) => dump(d),
        Commands::Generate(g) => generate(g),
    }
}

fn is_gz(path: &Path) -> bool {
    match path.extension() {
        None => false,
        Some(e) => e == "gz" || e == "gzip",
    }
}

fn csv_write<I>(recv: BusReader<Arc<Record>>, mut writer: Writer<I>)
where
    I: Write,
{
    for rec in recv {
        writer.serialize(rec).expect("Couldn't write to global csv");
    }
}

fn json_write<I>(recv: BusReader<Arc<Record>>, mut writer: I)
where
    I: Write,
{
    for rec in recv {
        serde_json::to_writer(&mut writer, &rec).expect("Couldn't write to global json");
        writeln!(writer).expect("Couldn't write to global json");
    }
}

fn yaml_write<I>(recv: BusReader<Arc<Record>>, mut writer: I)
where
    I: Write,
{
    for rec in recv {
        serde_yaml::to_writer(&mut writer, &rec).expect("Couldn't write to global yaml");
        writeln!(writer).expect("Couldn't write to global yaml");
    }
}

fn write_uniqs<I>(recv: BusReader<Arc<Record>>, mut writer: Writer<I>)
where
    I: Write,
{
    let mut u = BTreeMap::new();

    for rec in recv {
        u.entry(rec.path.clone())
            .or_insert_with(uniques::UniqueCounts::default)
            .update(rec.flag);
    }

    for (path, v) in u {
        writer
            .serialize(v.into_unique_out(path))
            .expect("Error writing the uniques");
    }
}

fn path_stdout(p: &Path) -> bool {
    p.as_os_str() == "-"
}

fn dump(opts: opts::Dump) -> Result<()> {
    let std_counts = opts.stdout_counts();
    env_logger::Builder::new()
        .filter(
            None,
            if std_counts == 1 {
                LevelFilter::Error
            } else {
                LevelFilter::Info
            },
        )
        .write_style(WriteStyle::Always)
        .target(Target::Stderr)
        .init();

    color_eyre::install()?;

    opts.validate(std_counts)?;
    let file_paths = opts.real_files();

    info!("Starting");

    let opts::Dump {
        csvs: individual_csvs,
        jsons: individual_jsons,
        yamls: individual_yamls,
        csv: csv_path,
        json: json_path,
        yaml: yaml_path,
        uniques: uniq_path,
        ..
    } = opts;

    let g_lvl = flate2::Compression::new(opts.level);

    crossbeam::scope(|scope| {
        let mut bus: Bus<Arc<Record>> = Bus::new(1000);

        if let Some(p) = csv_path {
            let recv = bus.add_rx();

            scope.spawn(|_| {
                if path_stdout(&p) {
                    csv_write(recv, csv::Writer::from_writer(io::stdout().lock()));
                } else if is_gz(&p) {
                    csv_write(
                        recv,
                        csv::Writer::from_writer(flate2::write::GzEncoder::new(
                            BufWriter::new(File::create(p).expect("Couldn't create the csv file")),
                            g_lvl,
                        )),
                    );
                } else {
                    csv_write(
                        recv,
                        csv::Writer::from_writer(BufWriter::new(
                            File::create(p).expect("Couldn't create the csv file"),
                        )),
                    );
                };
            });
        };

        if let Some(p) = json_path {
            let recv = bus.add_rx();

            scope.spawn(|_| {
                if path_stdout(&p) {
                    json_write(recv, io::stdout().lock())
                } else if is_gz(&p) {
                    json_write(
                        recv,
                        flate2::write::GzEncoder::new(
                            BufWriter::new(File::create(p).expect("Couldn't create the json file")),
                            g_lvl,
                        ),
                    );
                } else {
                    json_write(
                        recv,
                        BufWriter::new(File::create(p).expect("Couldn't create the json file")),
                    );
                };
            });
        };

        if let Some(p) = yaml_path {
            let recv = bus.add_rx();

            scope.spawn(|_| {
                if path_stdout(&p) {
                    yaml_write(recv, io::stdout().lock())
                } else if is_gz(&p) {
                    yaml_write(
                        recv,
                        flate2::write::GzEncoder::new(
                            BufWriter::new(File::create(p).expect("Couldn't create the yaml file")),
                            g_lvl,
                        ),
                    );
                } else {
                    json_write(
                        recv,
                        BufWriter::new(File::create(p).expect("Couldn't create the yaml file")),
                    );
                };
            });
        };

        if let Some(p) = uniq_path {
            let recv = bus.add_rx();

            scope.spawn(|_| {
                if path_stdout(&p) {
                    write_uniqs(recv, csv::Writer::from_writer(io::stdout().lock()));
                } else if is_gz(&p) {
                    write_uniqs(
                        recv,
                        csv::Writer::from_writer(flate2::write::GzEncoder::new(
                            BufWriter::new(
                                File::create(p).expect("Couldn't create the uniques csv file"),
                            ),
                            g_lvl,
                        )),
                    );
                } else {
                    write_uniqs(
                        recv,
                        csv::Writer::from_writer(BufWriter::new(
                            File::create(p).expect("Couldn't create the uniques csv file"),
                        )),
                    );
                };
            });
        }

        let wait_dur = Duration::from_millis(1);

        for f in file_paths {
            let running = Arc::new(std::sync::RwLock::new(true));

            crossbeam::scope(|fscope| {
                if individual_csvs {
                    let f = f.clone();
                    let mut recv = bus.add_rx();
                    let running = running.clone();

                    fscope.spawn(move |_| {
                        let ext = f
                            .extension()
                            .map_or_else(|| "csv".to_string(), |e| format!("{e:?}.csv"));

                        let mut csv_out = csv::Writer::from_path(f.with_extension(ext))
                            .expect("Couldn't open a csv writer");

                        'RUNNING: loop {
                            match recv.recv_timeout(wait_dur) {
                                Ok(r) => { csv_out.serialize(r) }
                                    .expect("Couldn't write an entry into a csv"),
                                Err(e) => match e {
                                    RecvTimeoutError::Timeout => {
                                        let r = running.read().unwrap();
                                        if !*r {
                                            break 'RUNNING;
                                        }
                                    }
                                    _ => return,
                                },
                            }
                        }
                    });
                };

                if individual_jsons {
                    let f = f.clone();
                    let mut recv = bus.add_rx();
                    let running = running.clone();

                    fscope.spawn(move |_| {
                        let ext = f
                            .extension()
                            .map_or_else(|| "json".to_string(), |e| format!("{e:?}.json"));

                        let mut json_out = BufWriter::new(
                            File::create(f.with_extension(ext))
                                .expect("Couldn't open a csv writer"),
                        );
                        'RUNNING: loop {
                            match recv.recv_timeout(wait_dur) {
                                Ok(r) => { serde_json::to_writer(&mut json_out, &r) }
                                    .expect("Couldn't write an entry into a csv"),
                                Err(e) => match e {
                                    RecvTimeoutError::Timeout => {
                                        let r = running.read().unwrap();
                                        if !*r {
                                            break 'RUNNING;
                                        }
                                        thread::yield_now();
                                    }
                                    _ => return,
                                },
                            }
                        }
                    });
                };

                if individual_yamls {
                    let f = f.clone();
                    let mut recv = bus.add_rx();
                    let running = running.clone();

                    fscope.spawn(move |_| {
                        let ext = f
                            .extension()
                            .map_or_else(|| "yaml".to_string(), |e| format!("{e:?}.yaml"));

                        let mut yaml_out = BufWriter::new(
                            File::create(f.with_extension(ext))
                                .expect("Couldn't open a csv writer"),
                        );
                        'RUNNING: loop {
                            match recv.recv_timeout(wait_dur) {
                                Ok(r) => { serde_yaml::to_writer(&mut yaml_out, &r) }
                                    .expect("Couldn't write an entry into a csv"),
                                Err(e) => match e {
                                    RecvTimeoutError::Timeout => {
                                        let r = running.read().unwrap();
                                        if !*r {
                                            break 'RUNNING;
                                        }
                                        thread::yield_now();
                                    }
                                    _ => return,
                                },
                            }
                        }
                    });
                };

                let path = f.to_string_lossy().into_owned();

                match file_parser::parse_file(f, &mut bus) {
                    Ok(_) => info!("Finished parsing {}", path),
                    Err(e) => error!("Couldn't parse '{}': {}", path, e),
                };

                {
                    let mut r = running.write().unwrap();
                    *r = false;
                }
            })
            .expect("Couldn't close all the threads");
        }
    })
    .expect("Couldn't close all the threads");

    Ok(())
}

fn generate(gen: Generate) -> Result<()> {
    let mut cmd = opts::Cli::command();
    let name = cmd.get_name().to_string();

    clap_complete::generate(gen.shell, &mut cmd, name, &mut io::stdout().lock());
    Ok(())
}
