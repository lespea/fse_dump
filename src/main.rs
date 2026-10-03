#![warn(rust_2018_compatibility)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

#[macro_use]
mod fail;

use std::{
    collections::BTreeMap,
    convert::identity,
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
    sync::Arc,
    thread,
};

use bus::{Bus, BusReader};
use clap::CommandFactory;
use color_eyre::Result;
use csv::Writer;
use env_logger::{Target, WriteStyle};
use log::LevelFilter;
use opts::{Commands, Generate};

use crate::record::{BusMsg, Record};

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
        #[cfg(feature = "watch")]
        Commands::Watch(w) => watch(w),
    }
}

/// Writes records to CSV format from a bus receiver
///
/// Stops at the first write error, since nothing later can succeed either.
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - CSV writer to output data
/// * `_` - Unused pretty print flag (kept for API consistency)
/// * `flush_all` - Whether to flush after each record
fn csv_write<I>(recv: BusReader<BusMsg>, mut writer: Writer<I>, _: bool, flush_all: bool)
where
    I: Write,
{
    for rec in recv.into_iter().filter_map(BusMsg::into_record) {
        if let Err(err) = writer.serialize(rec) {
            fail!("Couldn't serialize csv: {err}");
            return;
        }
        if flush_all && let Err(err) = writer.flush() {
            fail!("Couldn't flush csv: {err}");
            return;
        }
    }
}

/// Writes records to JSON format from a bus receiver
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - Writer to output JSON data
/// * `pretty` - Whether to use pretty formatting (multi-line)
/// * `flush_all` - Whether to flush after each record
fn json_write<I>(recv: BusReader<BusMsg>, mut writer: I, pretty: bool, flush_all: bool)
where
    I: Write,
{
    if pretty {
        for rec in recv.into_iter().filter_map(BusMsg::into_record) {
            if let Err(err) = serde_json::to_writer_pretty(&mut writer, &rec) {
                fail!("Couldn't serialize json: {err}");
                return;
            }
            if let Err(err) = writeln!(writer) {
                fail!("Couldn't append json newline: {err}");
                return;
            }
            if flush_all && let Err(err) = writer.flush() {
                fail!("Couldn't flush json: {err}");
                return;
            }
        }
    } else {
        for rec in recv.into_iter().filter_map(BusMsg::into_record) {
            if let Err(err) = serde_json::to_writer(&mut writer, &rec) {
                fail!("Couldn't serialize json: {err}");
                return;
            }
            if let Err(err) = writeln!(writer) {
                fail!("Couldn't append json newline: {err}");
                return;
            }
            if flush_all && let Err(err) = writer.flush() {
                fail!("Couldn't flush json: {err}");
                return;
            }
        }
    }
}

/// Writes records to YAML format from a bus receiver
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - Writer to output YAML data
/// * `_` - Unused pretty print flag (kept for API consistency)
/// * `flush_all` - Whether to flush after each record
fn yaml_write<I>(recv: BusReader<BusMsg>, mut writer: I, _: bool, flush_all: bool)
where
    I: Write,
{
    for rec in recv.into_iter().filter_map(BusMsg::into_record) {
        if let Err(err) = writeln!(writer, "---") {
            fail!("Couldn't write yaml separator: {err}");
            return;
        }
        if let Err(err) = serde_yaml_ng::to_writer(&mut writer, &rec) {
            fail!("Couldn't serialize yaml: {err}");
            return;
        }
        if let Err(err) = writeln!(writer) {
            fail!("Couldn't append yaml newline: {err}");
            return;
        }
        if flush_all && let Err(err) = writer.flush() {
            fail!("Couldn't flush yaml: {err}");
            return;
        }
    }
}

/// Aggregates records by path and writes unique path counts with combined flags
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - CSV writer for unique path output
/// * `_` - Unused pretty print flag (kept for API consistency)
/// * `include_timestamps` - Whether to include timestamps in CSV output
fn write_uniqs<I>(recv: BusReader<BusMsg>, mut writer: Writer<I>, _: bool, include_timestamps: bool)
where
    I: Write,
{
    let mut u: BTreeMap<String, uniques::UniqueCounts> = BTreeMap::new();

    for rec in recv.into_iter().filter_map(BusMsg::into_record) {
        // Most paths repeat, so only pay for the key clone on the first sighting
        match u.get_mut(&rec.path) {
            Some(counts) => counts.update(rec.flag, rec.file_timestamp),
            None => {
                let mut counts = uniques::UniqueCounts::default();
                counts.update(rec.flag, rec.file_timestamp);
                u.insert(rec.path.clone(), counts);
            }
        }
    }

    if include_timestamps {
        // Use full serialization with timestamps
        for (path, v) in u {
            if let Err(err) = writer.serialize(v.into_unique_out(path)) {
                fail!("Error writing the uniques: {err}");
            }
        }
    } else {
        // Manually write CSV without timestamps
        // Write header
        #[cfg(feature = "alt_flags")]
        let header = vec!["path", "counts", "flags", "alt_flags"];
        #[cfg(not(feature = "alt_flags"))]
        let header = vec!["path", "counts", "flags"];

        if let Err(err) = writer.write_record(&header) {
            fail!("Error writing CSV header: {err}");
            return;
        }

        // Write data rows
        for (path, v) in u {
            let out = v.into_unique_out_no_timestamps(path);
            let counts_str = out.counts.to_string();
            #[cfg(feature = "alt_flags")]
            let record = vec![
                out.path.as_str(),
                counts_str.as_str(),
                out.flags,
                out.alt_flags,
            ];
            #[cfg(not(feature = "alt_flags"))]
            let record = vec![out.path.as_str(), counts_str.as_str(), out.flags];

            if let Err(err) = writer.write_record(&record) {
                fail!("Error writing unique record: {err}");
            }
        }
    }
}

/// Checks if the given path represents stdout (indicated by "-")
///
/// # Arguments
/// * `p` - Path to check
///
/// # Returns
/// `true` if the path is "-", `false` otherwise
fn path_stdout(p: &Path) -> bool {
    p.as_os_str() == "-"
}

#[inline]
fn icsv(rec: Arc<Record>, writer: &mut Writer<BufWriter<File>>) {
    if let Err(err) = writer.serialize(&rec) {
        fail!("Error writing csv rec: {err}")
    }
}

#[inline]
fn ijson(rec: Arc<Record>, writer: &mut BufWriter<File>) {
    if let Err(err) = serde_json::to_writer(&mut *writer, &rec) {
        fail!("Error writing json rec: {err}")
    }
    if let Err(err) = writeln!(writer) {
        fail!("Error writing json newline: {err}")
    }
}

#[inline]
fn iyaml(rec: Arc<Record>, writer: &mut BufWriter<File>) {
    if let Err(err) = writeln!(writer, "---") {
        fail!("Error writing yaml separator: {err}")
    }
    if let Err(err) = serde_yaml_ng::to_writer(&mut *writer, &rec) {
        fail!("Error writing yaml rec: {err}")
    }
    if let Err(err) = writeln!(writer) {
        fail!("Error writing yaml newline: {err}")
    }
}

macro_rules! fdump {
    ( $bus: ident, $scope: ident, $ftype: expr, $path:ident, $proc_f:ident, $c_opt: ident, $creater:expr, ) => {
        if let Some(p) = $path {
            let recv = $bus.add_rx();

            if path_stdout(&p) {
                $scope.spawn(move || {
                    $proc_f(recv, $creater($c_opt.make_stdout()), false, false);
                });
            } else {
                match File::create(&p) {
                    Err(err) => fail!(
                        "Couldn't create {} output file {}: {err}",
                        $ftype,
                        p.display()
                    ),
                    Ok(f) => {
                        $scope.spawn(move || {
                            if $c_opt.is_gz(&p) {
                                $proc_f(
                                    recv,
                                    $creater($c_opt.make_gzip(BufWriter::new(f))),
                                    false,
                                    false,
                                );
                            } else if $c_opt.is_zstd(&p) {
                                #[cfg(feature = "zstd")]
                                {
                                    $proc_f(recv, $creater($c_opt.make_zstd(f)), false, false);
                                }

                                #[cfg(not(feature = "zstd"))]
                                unreachable!("zstd feature not enabled");
                            } else {
                                $proc_f(recv, $creater(BufWriter::new(f)), false, false);
                            };
                        });
                    }
                }
            }
        };
    };
}

macro_rules! idump {
    ( $want: ident, $bus: ident, $fscope: ident, $ftype: expr, $f: ident, $make_out: expr, $ifun: expr, ) => {
        if $want {
            let mut out_path = $f.clone();
            out_path.as_mut_os_string().push(format!(".{}", $ftype));

            match File::create(&out_path) {
                Err(err) => fail!(
                    "Couldn't open a {} writer at {}: {err}",
                    $ftype,
                    out_path.display()
                ),
                Ok(w) => {
                    let recv = $bus.add_rx();

                    $fscope.spawn(move || {
                        let out = &mut $make_out(BufWriter::new(w));

                        // Stop as soon as the parser signals the end of this file
                        for msg in recv {
                            match msg {
                                BusMsg::Record(r) => $ifun(r, out),
                                BusMsg::EndOfFile => break,
                            }
                        }
                    });
                }
            };
        };
    };
}

#[inline]
fn new_bus() -> Bus<BusMsg> {
    Bus::new(4096)
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
        .parse_default_env()
        .write_style(WriteStyle::Always)
        .target(Target::Stderr)
        .init();

    color_eyre::install()?;

    opts.validate(std_counts)?;
    let file_paths = opts.real_files();
    if file_paths.is_empty() {
        return Err(color_eyre::eyre::eyre!(
            "No fsevents files found to parse (check the paths and the --days cutoff)"
        ));
    }

    info!("Starting");

    let opts::Dump {
        csvs: individual_csvs,
        jsons: individual_jsons,
        yamls: individual_yamls,
        csv: csv_path,
        json: json_path,
        yaml: yaml_path,
        uniques: uniq_path,
        unique_timestamps,
        ..
    } = opts;

    let rec_filter = opts.filter_opts.filter()?;

    let copts = opts.compress_opts;

    thread::scope(|scope| {
        let mut bus = new_bus();

        fdump!(
            bus,
            scope,
            "csv",
            csv_path,
            csv_write,
            copts,
            csv::Writer::from_writer,
        );

        // Handle uniques output with timestamp flag
        if let Some(p) = uniq_path {
            let recv = bus.add_rx();

            if path_stdout(&p) {
                scope.spawn(move || {
                    write_uniqs(
                        recv,
                        csv::Writer::from_writer(copts.make_stdout()),
                        false,
                        unique_timestamps,
                    );
                });
            } else {
                match File::create(&p) {
                    Err(err) => fail!(
                        "Couldn't create unique csv output file {}: {err}",
                        p.display()
                    ),
                    Ok(f) => {
                        scope.spawn(move || {
                            if copts.is_gz(&p) {
                                write_uniqs(
                                    recv,
                                    csv::Writer::from_writer(copts.make_gzip(BufWriter::new(f))),
                                    false,
                                    unique_timestamps,
                                );
                            } else if copts.is_zstd(&p) {
                                #[cfg(feature = "zstd")]
                                {
                                    write_uniqs(
                                        recv,
                                        csv::Writer::from_writer(copts.make_zstd(f)),
                                        false,
                                        unique_timestamps,
                                    );
                                }

                                #[cfg(not(feature = "zstd"))]
                                unreachable!("zstd feature not enabled");
                            } else {
                                write_uniqs(
                                    recv,
                                    csv::Writer::from_writer(BufWriter::new(f)),
                                    false,
                                    unique_timestamps,
                                );
                            };
                        });
                    }
                }
            }
        }

        fdump!(bus, scope, "json", json_path, json_write, copts, identity,);
        fdump!(bus, scope, "yaml", yaml_path, yaml_write, copts, identity,);

        for f in file_paths {
            thread::scope(|fscope| {
                idump!(
                    individual_csvs,
                    bus,
                    fscope,
                    "csv",
                    f,
                    Writer::from_writer,
                    icsv,
                );

                idump!(individual_jsons, bus, fscope, "json", f, identity, ijson,);
                idump!(individual_yamls, bus, fscope, "yaml", f, identity, iyaml,);

                match file_parser::parse_file(&f, &mut bus, &rec_filter) {
                    Ok(_) => info!("Finished parsing {}", f.display()),
                    Err(e) => fail!("Couldn't parse '{}': {}", f.display(), e),
                };

                // Lets the per-file writers above finish without waiting on a timeout
                bus.broadcast(BusMsg::EndOfFile);
            });
        }
    });

    fail::exit_result("dumping")
}

fn generate(g: Generate) -> Result<()> {
    let mut cmd = opts::Cli::command();
    let name = cmd.get_name().to_string();

    clap_complete::generate(g.shell, &mut cmd, name, &mut io::stdout().lock());
    Ok(())
}

#[cfg(feature = "watch")]
fn watch(opts: opts::Watch) -> Result<()> {
    use std::time::Duration;

    use crossbeam_channel::select;
    use notify_debouncer_full::{
        DebounceEventResult, FileIdMap, new_debouncer_opt, notify::RecursiveMode,
    };

    use crate::file_parser::parse_file;

    env_logger::Builder::new()
        .filter(None, LevelFilter::Info)
        .parse_default_env()
        .write_style(WriteStyle::Always)
        .target(Target::Stderr)
        .init();

    color_eyre::install()?;

    opts.compress_opts.validate()?;
    let rec_filter = opts.filter_opts.filter()?;

    let (send, recv) = crossbeam_channel::bounded(128);

    // Ctrl-C / SIGTERM stop the loop below so the output stream gets its trailer written
    let (stop_send, stop_recv) = crossbeam_channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        let _ = stop_send.try_send(());
    })?;

    let debounce_time = Duration::from_secs(2);

    // Forwards newly created fsevents logs (and only those) to the parsing loop
    let on_events = move |result: DebounceEventResult| match result {
        Ok(events) => events.iter().for_each(|event| {
            if event.kind.is_create() {
                for path in event.paths.iter() {
                    let is_log =
                        path.file_name().is_some_and(opts::is_fsevents_log_name) && path.is_file();
                    if !is_log {
                        debug!("Ignoring non-log file {}", path.display());
                        continue;
                    }
                    if let Err(err) = send.send_timeout(path.clone(), Duration::from_secs(1)) {
                        fail!("Error processing created file {}: {err}", path.display());
                    }
                }
            }
        }),
        Err(errors) => errors
            .iter()
            .for_each(|error| fail!("Watch error: {error:?}")),
    };

    // Keeps whichever watcher we built alive until the loop below is done
    let _watcher: Box<dyn std::any::Any> = if opts.poll {
        let mut debouncer = new_debouncer_opt::<_, notify::PollWatcher, FileIdMap>(
            debounce_time,
            None,
            on_events,
            FileIdMap::new(),
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        for path in opts.watch_dirs {
            info!("Watching {}", path.display());
            debouncer.watch(&path, RecursiveMode::Recursive)?;
        }

        Box::new(debouncer)
    } else {
        let mut debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher, FileIdMap>(
            debounce_time,
            None,
            on_events,
            FileIdMap::new(),
            notify::Config::default(),
        )?;

        for path in opts.watch_dirs {
            info!("Watching {}", path.display());
            debouncer.watch(&path, RecursiveMode::Recursive)?;
        }

        Box::new(debouncer)
    };

    let copts = opts.compress_opts;

    thread::scope(|fscope| {
        let mut bus = new_bus();

        let rec_recv = bus.add_rx();
        let writer = fscope.spawn(move || {
            let out = copts.make_stdout();

            match opts.format {
                opts::WatchFormat::Csv => {
                    csv_write(rec_recv, csv::Writer::from_writer(out), false, true)
                }
                opts::WatchFormat::Json => json_write(rec_recv, out, opts.pretty, true),
                opts::WatchFormat::Yaml => yaml_write(rec_recv, out, false, true),
            }
        });

        loop {
            select! {
                recv(recv) -> msg => match msg {
                    Ok(path) => {
                        if let Err(err) = parse_file(&path, &mut bus, &rec_filter) {
                            fail!("Error parsing {}: {err}", path.display());
                        }
                    }
                    Err(_) => break,
                },
                recv(stop_recv) -> _ => {
                    info!("Interrupted; finishing the output");
                    break;
                }
                default(Duration::from_millis(500)) => {
                    // Nothing to write to any more; keeping the watch alive would only hide it
                    if writer.is_finished() {
                        fail!("The output writer stopped unexpectedly");
                        break;
                    }
                }
            }
        }

        // Closing the bus is what lets the writer flush and finish its stream
        drop(bus);
    });

    fail::exit_result("watching")
}
