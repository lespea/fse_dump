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
    io::{self, BufWriter},
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

use crate::{
    finish::Finish,
    record::{BusMsg, Record},
};

mod file_parser;
mod finish;
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

/// The records a writer should handle
///
/// A per-file writer stops at the end of the file it was started for; a shared writer runs
/// until the bus is closed.
fn records(recv: BusReader<BusMsg>, one_file: bool) -> impl Iterator<Item = Arc<Record>> {
    recv.into_iter()
        .take_while(move |msg| !(one_file && matches!(msg, BusMsg::EndOfFile)))
        .filter_map(BusMsg::into_record)
}

/// Reports how a writer ended and closes its output
///
/// Writing stops at the first error, since nothing after it can succeed either. The sink is
/// still closed so a compressed stream is left as well formed as the sink allows, but a second
/// failure there is not counted again.
fn end_output(what: &str, wrote: Result<()>, close: impl FnOnce() -> io::Result<()>) {
    match wrote {
        Ok(()) => {
            if let Err(err) = close() {
                fail!("Couldn't finish the {what} output: {err}");
            }
        }
        Err(err) => {
            fail!("Couldn't write {what}: {err}");
            if let Err(err) = close() {
                debug!("Couldn't finish the {what} output after a write failure: {err}");
            }
        }
    }
}

/// Flushes a csv writer and finishes the sink underneath it
fn close_csv<W: Finish>(writer: Writer<W>) -> io::Result<()> {
    writer
        .into_inner()
        .map_err(|err| err.into_error())?
        .finish()
}

/// Writes records to CSV format from a bus receiver
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - CSV writer to output data
/// * `_` - Unused pretty print flag (kept for API consistency)
/// * `flush_all` - Whether to flush after each record
/// * `one_file` - Whether to stop at the end of the current input file
fn csv_write<W>(
    recv: BusReader<BusMsg>,
    mut writer: Writer<W>,
    _: bool,
    flush_all: bool,
    one_file: bool,
) where
    W: Finish,
{
    let wrote = (|| {
        for rec in records(recv, one_file) {
            writer.serialize(&rec)?;
            if flush_all {
                writer.flush()?;
            }
        }
        Ok(())
    })();

    end_output("csv", wrote, || close_csv(writer));
}

/// Writes records to JSON format from a bus receiver
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - Writer to output JSON data
/// * `pretty` - Whether to use pretty formatting (multi-line)
/// * `flush_all` - Whether to flush after each record
/// * `one_file` - Whether to stop at the end of the current input file
fn json_write<W>(
    recv: BusReader<BusMsg>,
    mut writer: W,
    pretty: bool,
    flush_all: bool,
    one_file: bool,
) where
    W: Finish,
{
    let wrote = (|| {
        for rec in records(recv, one_file) {
            if pretty {
                serde_json::to_writer_pretty(&mut writer, &rec)?;
            } else {
                serde_json::to_writer(&mut writer, &rec)?;
            }
            writeln!(writer)?;
            if flush_all {
                writer.flush()?;
            }
        }
        Ok(())
    })();

    end_output("json", wrote, || writer.finish());
}

/// Writes records to YAML format from a bus receiver
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - Writer to output YAML data
/// * `_` - Unused pretty print flag (kept for API consistency)
/// * `flush_all` - Whether to flush after each record
/// * `one_file` - Whether to stop at the end of the current input file
fn yaml_write<W>(recv: BusReader<BusMsg>, mut writer: W, _: bool, flush_all: bool, one_file: bool)
where
    W: Finish,
{
    let wrote = (|| {
        for rec in records(recv, one_file) {
            writeln!(writer, "---")?;
            serde_yaml_ng::to_writer(&mut writer, &rec)?;
            writeln!(writer)?;
            if flush_all {
                writer.flush()?;
            }
        }
        Ok(())
    })();

    end_output("yaml", wrote, || writer.finish());
}

/// Writes unique path records with aggregated counts and flags
///
/// # Arguments
/// * `recv` - Bus reader receiving record updates
/// * `writer` - CSV writer for unique path output
/// * `_` - Unused pretty print flag (kept for API consistency)
/// * `include_timestamps` - Whether to include timestamps in CSV output
fn write_uniqs<W>(recv: BusReader<BusMsg>, mut writer: Writer<W>, _: bool, include_timestamps: bool)
where
    W: Finish,
{
    let mut u: BTreeMap<String, uniques::UniqueCounts> = BTreeMap::new();

    for rec in records(recv, false) {
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

    let wrote = (|| {
        if include_timestamps {
            // Use full serialization with timestamps
            for (path, v) in u {
                writer.serialize(v.into_unique_out(path))?;
            }
        } else {
            // Manually write CSV without timestamp fields
            #[cfg(feature = "alt_flags")]
            let header = vec!["path", "counts", "flags", "alt_flags"];
            #[cfg(not(feature = "alt_flags"))]
            let header = vec!["path", "counts", "flags"];

            writer.write_record(&header)?;

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

                writer.write_record(&record)?;
            }
        }
        Ok(())
    })();

    end_output("uniques", wrote, || close_csv(writer));
}

#[inline]
fn path_stdout(p: &Path) -> bool {
    p.as_os_str() == "-"
}

macro_rules! fdump {
    ( $bus: ident, $scope: ident, $ftype: expr, $path:ident, $proc_f:ident, $c_opt: ident, $creater:expr, ) => {
        if let Some(p) = $path {
            let recv = $bus.add_rx();

            if path_stdout(&p) {
                $scope.spawn(move || {
                    $proc_f(recv, $creater($c_opt.make_stdout()), false, false, false);
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
                                    false,
                                );
                            } else if $c_opt.is_zstd(&p) {
                                #[cfg(feature = "zstd")]
                                {
                                    $proc_f(
                                        recv,
                                        $creater($c_opt.make_zstd(f)),
                                        false,
                                        false,
                                        false,
                                    );
                                }

                                #[cfg(not(feature = "zstd"))]
                                unreachable!("zstd feature not enabled");
                            } else {
                                $proc_f(recv, $creater(BufWriter::new(f)), false, false, false);
                            };
                        });
                    }
                }
            }
        };
    };
}

macro_rules! idump {
    ( $want: ident, $bus: ident, $fscope: ident, $ftype: expr, $f: ident, $creater: expr, $proc_f: ident, ) => {
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
                        // Stops as soon as the parser signals the end of this file
                        $proc_f(recv, $creater(BufWriter::new(w)), false, false, true);
                    });
                }
            }
        }
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
            // Logs go to stderr either way; when stdout carries the data, keep progress
            // chatter out of the terminal but still show what was skipped or went wrong
            if std_counts == 1 {
                LevelFilter::Warn
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
                    csv_write,
                );
                idump!(
                    individual_jsons,
                    bus,
                    fscope,
                    "json",
                    f,
                    identity,
                    json_write,
                );
                idump!(
                    individual_yamls,
                    bus,
                    fscope,
                    "yaml",
                    f,
                    identity,
                    yaml_write,
                );

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
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

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

    // Ctrl-C / SIGTERM stop the loop below so the output stream gets its trailer written; a
    // second signal means that shutdown is stuck or too slow, so it exits on the spot
    let (stop_send, stop_recv) = crossbeam_channel::bounded::<()>(1);
    let interrupted = AtomicBool::new(false);
    ctrlc::set_handler(move || {
        if interrupted.swap(true, Ordering::SeqCst) {
            error!("Interrupted again; exiting without finishing the output");
            std::process::exit(130);
        }
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
                    csv_write(rec_recv, csv::Writer::from_writer(out), false, true, false)
                }
                opts::WatchFormat::Json => json_write(rec_recv, out, opts.pretty, true, false),
                opts::WatchFormat::Yaml => yaml_write(rec_recv, out, false, true, false),
            }
        });

        loop {
            // While the bus is open the writer only stops when its output failed, which it has
            // already reported; keeping the watch alive would just hide that
            if writer.is_finished() {
                warn!("The output writer stopped; ending the watch");
                break;
            }

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
                // Wakes up now and then so the writer is re-checked while nothing arrives
                default(Duration::from_millis(500)) => {}
            }
        }

        // Closing the bus is what lets the writer flush and finish its stream
        drop(bus);
    });

    fail::exit_result("watching")
}
