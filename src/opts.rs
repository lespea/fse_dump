use std::{ffi::OsStr, ops::Sub, path::PathBuf, time::SystemTime};

use clap::{value_parser, Args, Parser, Subcommand};
use clap_complete::Shell;
use color_eyre::{eyre::eyre, Result};
use time::OffsetDateTime;

/// Utility to dump the fsevent files on OSX
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Dump the known net defs
    Dump(Dump),

    /// Outputs shell completion for fish
    #[clap(aliases = &["gen"])]
    Generate(Generate),

    /// Watch for new fse files, parse them, and write them to the desired output
    #[cfg(feature = "watch")]
    Watch(Watch),
}

#[derive(Debug, Args)]
pub struct Generate {
    /// If every fse record file we find should be dumped to a csv "next" to it (filename + .csv)
    #[arg(value_parser = value_parser!(Shell))]
    pub shell: Shell,
}

#[cfg(feature = "watch")]
#[derive(Debug, Args)]
pub struct Watch {
    /// The format the parsed files should be output to
    #[arg(short, long, default_value = "json")]
    pub format: WatchFormat,

    /// If the outupt should be "pretty" formatted (multi-line)
    #[arg(short, long)]
    pub pretty: bool,

    /// Filter events based on the path
    #[arg(long)]
    pub filter: Option<String>,

    /// The dirs to watch
    #[arg(default_value = "/System/Volumes/Data/.fseventsd/")]
    pub watch_dirs: Vec<PathBuf>,

    /// Use polling (performance issues only use if the normal watcher doesn't work)
    #[arg(long)]
    pub poll: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[cfg(feature = "watch")]
pub enum WatchFormat {
    Csv,
    Json,
    Yaml,
}

#[derive(Debug, Args)]
pub struct Dump {
    /// If every fse record file we find should be dumped to a csv "next" to it (filename + .csv)
    #[arg(long = "csvs")]
    pub csvs: bool,

    /// If every fse record file we find should be dumped to a json "next" to it (filename + .json)
    #[arg(long = "jsons")]
    pub jsons: bool,

    /// If every fse record file we find should be dumped to a yaml "next" to it (filename + .yaml)
    #[arg(long = "yamls")]
    pub yamls: bool,

    /// If we should dump the combined records into a single csv.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    #[arg(short, long)]
    pub csv: Option<PathBuf>,

    /// If we should dump the combined records into a single json.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    #[arg(short, long)]
    pub json: Option<PathBuf>,

    /// If we should dump the combined records into a single yaml.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    #[arg(short, long)]
    pub yaml: Option<PathBuf>,

    /// If we should dump the unique paths/operations found into a csv
    ///
    /// We'll combine all of the operations for each path so there is one entry per path
    #[arg(short, long)]
    pub uniques: Option<PathBuf>,

    /// The level we should compress the output as; 0-9
    #[arg(short, long, default_value = "7")]
    pub level: u32,

    /// How many days we should pull (based off the file mod time)
    #[arg(short = 'd', long = "days", default_value = "90")]
    pub pull_days: u32,

    /// The fs event files that should be parsed. If any arg is a directory then any file within
    /// that has a filename consisting solely of hex chars will be considered a file to parse
    #[arg(default_value = "/System/Volumes/Data/.fseventsd/")]
    pub files: Vec<PathBuf>,
}

fn stdout_path(path: &Option<PathBuf>) -> bool {
    if let Some(p) = path {
        p.as_os_str() == "-"
    } else {
        false
    }
}

impl Dump {
    pub fn stdout_counts(&self) -> usize {
        let mut counts = 0;
        if stdout_path(&self.csv) {
            counts += 1
        };
        if stdout_path(&self.json) {
            counts += 1
        };
        if stdout_path(&self.uniques) {
            counts += 1
        };
        counts
    }

    pub fn validate(&self, counts: usize) -> Result<()> {
        if self.level > 9 {
            return Err(eyre!(
                "The compression level must be between 0 and 9 (inclusive)",
            ));
        }

        if counts > 1 {
            return Err(eyre!("Can't have more than one file printing to stdout!",));
        }

        if !(self.csvs
            || self.jsons
            || self.csv.is_some()
            || self.json.is_some()
            || self.uniques.is_some())
        {
            return Err(eyre!("You must specify at least one output type!",));
        }

        Ok(())
    }

    #[inline]
    fn want_filename(str: &OsStr) -> bool {
        str.to_string_lossy().chars().all(|c| c.is_ascii_hexdigit())
    }

    fn cutoff_time(&self) -> Option<SystemTime> {
        if self.pull_days > 0 {
            Some(
                OffsetDateTime::now_local()
                    .unwrap_or_else(|_| OffsetDateTime::now_utc())
                    .sub(time::Duration::days(self.pull_days as i64))
                    .replace_time(time::Time::MIDNIGHT)
                    .into(),
            )
        } else {
            None
        }
    }

    pub fn real_files(&self) -> Vec<PathBuf> {
        let cutoff = self.cutoff_time();

        let mut files = Vec::with_capacity(128);

        self.files.iter().for_each(|path| {
            match path.metadata() {
                Err(err) => error!("Error processing '{}': {err}", path.display()),
                Ok(info) => {
                    if info.is_dir() {
                        walkdir::WalkDir::new(path)
                            .max_depth(1)
                            .follow_links(true)
                            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
                            .into_iter()
                            .for_each(|e| match e {
                                Ok(e) => {
                                    if let Ok(m) = e.metadata() {
                                        if !m.is_dir()
                                            && Dump::want_filename(e.file_name())
                                            && if let Some(cut_time) = cutoff {
                                                if let Ok(mod_time) =
                                                    m.modified().or_else(|_| m.created())
                                                {
                                                    // Only process files that have a mod time greater than our
                                                    // cutoff time
                                                    if mod_time > cut_time {
                                                        true
                                                    } else {
                                                        debug!(
                                                            "Skipping {} due to time cutoff",
                                                            e.path().display()
                                                        );
                                                        false
                                                    }
                                                } else {
                                                    true
                                                }
                                            } else {
                                                true
                                            }
                                        {
                                            debug!("Found the fs events file {:?}", e.path());
                                            files.push(e.into_path());
                                        }
                                    }
                                }

                                Err(err) => {
                                    error!("Error iterating the files: {}", err);
                                }
                            });
                    } else if info.is_file() {
                        files.push(path.clone())
                    } else {
                        error!("Unknown file type for '{}': {info:?}", path.display())
                    }
                }
            }
        });

        files
    }
}

pub fn get_opts() -> Result<Cli> {
    Ok(Cli::parse())
}
