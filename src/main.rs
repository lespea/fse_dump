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
    convert::identity,
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc,
    },
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
use record::RecordFilter;

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

static NO_FILTER: record::NoRecordFilter = record::NoRecordFilter {};

fn main() -> Result<()> {
    match opts::get_opts()?.command {
        Commands::Dump(d) => dump(d),
        Commands::Generate(g) => generate(g),
        #[cfg(feature = "watch")]
        Commands::Watch(w) => watch(w),
    }
}

fn is_gz(path: &Path) -> bool {
    match path.extension() {
        None => false,
        Some(e) => e == "gz" || e == "gzip",
    }
}

fn csv_write<I, F>(recv: BusReader<Arc<Record>>, mut writer: Writer<I>, filter: F, _: bool)
where
    I: Write,
    F: RecordFilter,
{
    for rec in recv {
        if filter.filter(&rec) {
            if let Err(err) = writer.serialize(rec) {
                error!("Couldn't serialize csv: {err}");
            }
        }
    }
}

fn json_write<I, F>(recv: BusReader<Arc<Record>>, mut writer: I, filter: F, pretty: bool)
where
    I: Write,
    F: RecordFilter,
{
    if pretty {
        for rec in recv {
            if filter.filter(&rec) {
                if let Err(err) = serde_json::to_writer(&mut writer, &rec) {
                    error!("Couldn't serialize json: {err}");
                }
                if let Err(err) = writeln!(writer) {
                    error!("Couldn't append newline: {err}");
                }
            }
        }
    } else {
        for rec in recv {
            if filter.filter(&rec) {
                if let Err(err) = serde_json::to_writer_pretty(&mut writer, &rec) {
                    error!("Couldn't serialize json: {err}");
                }
                if let Err(err) = writeln!(writer) {
                    error!("Couldn't append newline: {err}");
                }
            }
        }
    }
}

fn yaml_write<I, F>(recv: BusReader<Arc<Record>>, mut writer: I, filter: F, _: bool)
where
    I: Write,
    F: RecordFilter,
{
    for rec in recv {
        if filter.filter(&rec) {
            if let Err(err) = serde_yaml::to_writer(&mut writer, &rec) {
                error!("Couldn't serialize yaml: {err}");
            }
            if let Err(err) = writeln!(writer) {
                error!("Couldn't append newline: {err}");
            }
        }
    }
}

fn write_uniqs<I, F>(recv: BusReader<Arc<Record>>, mut writer: Writer<I>, filter: F, _: bool)
where
    I: Write,
    F: RecordFilter,
{
    let mut u = BTreeMap::new();

    for rec in recv {
        if filter.filter(&rec) {
            u.entry(rec.path.clone())
                .or_insert_with(uniques::UniqueCounts::default)
                .update(rec.flag);
        }
    }

    for (path, v) in u {
        if let Err(err) = writer.serialize(v.into_unique_out(path)) {
            error!("Error writing the uniques: {err}");
        }
    }
}

fn path_stdout(p: &Path) -> bool {
    p.as_os_str() == "-"
}

#[inline]
fn icsv(rec: Arc<Record>, writer: &mut Writer<BufWriter<File>>) {
    if let Err(err) = writer.serialize(&rec) {
        error!("Error writing json rec: {err}")
    }
}

#[inline]
fn ijson(rec: Arc<Record>, writer: &mut BufWriter<File>) {
    if let Err(err) = serde_json::to_writer(writer, &rec) {
        error!("Error writing json rec: {err}")
    }
}

#[inline]
fn iyaml(rec: Arc<Record>, writer: &mut BufWriter<File>) {
    if let Err(err) = serde_yaml::to_writer(writer, &rec) {
        error!("Error writing json rec: {err}")
    }
}

macro_rules! fdump {
    ( $bus: ident, $scope: ident, $ftype: expr, $path:ident, $proc_f:ident, $g_lvl: ident, $creater:expr, ) => {
        if let Some(p) = $path {
            let recv = $bus.add_rx();

            if path_stdout(&p) {
                $scope.spawn(move |_| {
                    $proc_f(recv, $creater(io::stdout().lock()), NO_FILTER, false);
                });
            } else {
                match File::create(&p) {
                    Err(err) => error!(
                        "Couldn't create {} output file {}: {err}",
                        $ftype,
                        p.display()
                    ),
                    Ok(f) => {
                        $scope.spawn(move |_| {
                            if is_gz(&p) {
                                $proc_f(
                                    recv,
                                    $creater(flate2::write::GzEncoder::new(
                                        BufWriter::new(f),
                                        $g_lvl,
                                    )),
                                    NO_FILTER,
                                    false,
                                );
                            } else {
                                $proc_f(recv, $creater(BufWriter::new(f)), NO_FILTER, false);
                            };
                        });
                    }
                }
            }
        };
    };
}

macro_rules! idump {
    ( $want: ident, $bus: ident, $fscope: ident, $running: ident, $ftype: expr, $f: ident, $make_out: expr, $ifun: expr, ) => {
        if $want {
            let mut out_path = $f.clone();
            out_path.as_mut_os_string().push(format!(".{}", $ftype));

            match File::create(&out_path) {
                Err(err) => error!(
                    "Couldn't open a {} writer at {}: {err}",
                    $ftype,
                    out_path.display()
                ),
                Ok(w) => {
                    let mut recv = $bus.add_rx();
                    let running = $running.clone();

                    $fscope.spawn(move |_| {
                        let out = &mut $make_out(BufWriter::new(w));

                        'RUNNING: loop {
                            match recv.recv_timeout(Duration::from_millis(50)) {
                                Ok(r) => $ifun(r, out),
                                Err(e) => match e {
                                    RecvTimeoutError::Timeout => {
                                        if !running.load(Ordering::Acquire) {
                                            break 'RUNNING;
                                        }
                                        thread::yield_now();
                                    }
                                    _ => return,
                                },
                            }
                        }
                    });
                }
            };
        };
    };
}

#[inline]
fn new_bus() -> Bus<Arc<Record>> {
    Bus::new(512)
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
        let mut bus = new_bus();

        fdump!(
            bus,
            scope,
            "csv",
            csv_path,
            csv_write,
            g_lvl,
            csv::Writer::from_writer,
        );

        fdump!(
            bus,
            scope,
            "unique csv",
            uniq_path,
            write_uniqs,
            g_lvl,
            csv::Writer::from_writer,
        );

        fdump!(bus, scope, "json", json_path, json_write, g_lvl, identity,);
        fdump!(bus, scope, "yaml", yaml_path, yaml_write, g_lvl, identity,);

        for f in file_paths {
            let running = Arc::new(AtomicBool::new(true));

            crossbeam::scope(|fscope| {
                idump!(
                    individual_csvs,
                    bus,
                    fscope,
                    running,
                    "csv",
                    f,
                    Writer::from_writer,
                    icsv,
                );

                idump!(
                    individual_jsons,
                    bus,
                    fscope,
                    running,
                    "json",
                    f,
                    identity,
                    ijson,
                );

                idump!(
                    individual_yamls,
                    bus,
                    fscope,
                    running,
                    "yaml",
                    f,
                    identity,
                    iyaml,
                );

                match file_parser::parse_file(&f, &mut bus) {
                    Ok(_) => info!("Finished parsing {}", f.display()),
                    Err(e) => error!("Couldn't parse '{}': {}", f.display(), e),
                };

                running.store(false, Ordering::Release);
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

#[cfg(feature = "watch")]
fn watch(opts: opts::Watch) -> Result<()> {
    use std::mem;

    use notify_debouncer_full::{
        new_debouncer_opt,
        notify::{RecursiveMode, Watcher},
        DebounceEventResult, FileIdMap,
    };
    use regex::bytes::Regex;

    use crate::{file_parser::parse_file, record::PathFilter};

    env_logger::Builder::new()
        .filter(None, LevelFilter::Info)
        .write_style(WriteStyle::Always)
        .target(Target::Stderr)
        .init();

    color_eyre::install()?;

    let path_rex = opts
        .filter
        .map(|re| Regex::new(&re).expect("Bad filter regex"));

    let (send, recv) = crossbeam_channel::bounded(128);

    let debounce_time = Duration::from_secs(2);

    if opts.poll {
        let mut debouncer = new_debouncer_opt::<_, notify::PollWatcher, FileIdMap>(
            debounce_time,
            None,
            move |result: DebounceEventResult| match result {
                Ok(events) => events.iter().for_each(|event| {
                    if event.kind.is_create() {
                        for path in event.paths.iter() {
                            if path.exists() {
                                if let Err(err) =
                                    send.send_timeout(path.clone(), Duration::from_secs(1))
                                {
                                    error!(
                                        "Error processing created file {}: {err}",
                                        path.display()
                                    );
                                }
                            }
                        }
                    }
                }),
                Err(errors) => errors
                    .iter()
                    .for_each(|error| error!("Watch error: {error:?}")),
            },
            FileIdMap::new(),
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        for path in opts.watch_dirs {
            info!("Watching {}", path.display());
            debouncer.watcher().watch(&path, RecursiveMode::Recursive)?;
            debouncer.cache().add_root(&path, RecursiveMode::Recursive);
        }

        mem::forget(debouncer);
    } else {
        let mut debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher, FileIdMap>(
            debounce_time,
            None,
            move |result: DebounceEventResult| match result {
                Ok(events) => events.iter().for_each(|event| {
                    if event.kind.is_create() {
                        for path in event.paths.iter() {
                            if path.exists() {
                                if let Err(err) =
                                    send.send_timeout(path.clone(), Duration::from_secs(1))
                                {
                                    error!(
                                        "Error processing created file {}: {err}",
                                        path.display()
                                    );
                                }
                            }
                        }
                    }
                }),
                Err(errors) => errors
                    .iter()
                    .for_each(|error| error!("Watch error: {error:?}")),
            },
            FileIdMap::new(),
            notify::Config::default(),
        )?;

        for path in opts.watch_dirs {
            info!("Watching {}", path.display());
            debouncer.watcher().watch(&path, RecursiveMode::Recursive)?;
            debouncer.cache().add_root(&path, RecursiveMode::Recursive);
        }

        mem::forget(debouncer);
    };

    crossbeam::scope(|fscope| {
        let mut bus = new_bus();

        let rec_recv = bus.add_rx();
        fscope.spawn(move |_| {
            let out = io::stdout().lock();
            if let Some(path_rex) = path_rex {
                let filt = PathFilter { path_rex };
                match opts.format {
                    opts::WatchFormat::Csv => {
                        csv_write(rec_recv, csv::Writer::from_writer(out), filt, false)
                    }
                    opts::WatchFormat::Json => json_write(rec_recv, out, filt, opts.pretty),
                    opts::WatchFormat::Yaml => yaml_write(rec_recv, out, filt, false),
                }
            } else {
                match opts.format {
                    opts::WatchFormat::Csv => {
                        csv_write(rec_recv, csv::Writer::from_writer(out), NO_FILTER, false)
                    }
                    opts::WatchFormat::Json => json_write(rec_recv, out, NO_FILTER, opts.pretty),
                    opts::WatchFormat::Yaml => yaml_write(rec_recv, out, NO_FILTER, false),
                }
            }
        });

        for path in recv {
            if let Err(err) = parse_file(&path, &mut bus) {
                error!("Error parsing {}: {err}", path.display());
            }
        }
    })
    .unwrap();

    Ok(())
}
