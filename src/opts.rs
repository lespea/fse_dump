use std::{
    ffi::OsStr,
    io::{BufWriter, Write},
    path::PathBuf,
    time::SystemTime,
};

use clap::{Args, Parser, Subcommand, value_parser};
use clap_complete::Shell;
use color_eyre::{Result, eyre::eyre};
use jiff::{Span, Zoned};
use std::path::Path;

use crate::record::RecordFilter;

/// Utility to dump the fsevent files on OSX
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Dump fsevents file into the wanted output files/format
    Dump(Dump),

    /// Watch for new fse files, parse them, and write them to the desired output
    #[cfg(feature = "watch")]
    Watch(Watch),

    /// Outputs shell completions for the desired shell
    #[clap(aliases = &["gen"])]
    Generate(Generate),
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
    #[arg(short = 'o', long, default_value = "json")]
    pub format: WatchFormat,

    /// If the outupt should be "pretty" formatted (multi-line)
    #[arg(short = 'P', long)]
    pub pretty: bool,

    /// The dirs to watch
    #[arg(default_value = "/System/Volumes/Data/.fseventsd/")]
    pub watch_dirs: Vec<PathBuf>,

    /// Use polling (performance issues only use if the normal watcher doesn't work)
    #[arg(long)]
    pub poll: bool,

    /// The compression options
    #[clap(flatten)]
    pub compress_opts: CompressOpts,

    /// The filter options
    #[clap(flatten)]
    pub filter_opts: FilterOpts,
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
    ///
    /// If the path ends in `.gz` it will be gzip compressed
    #[arg(short, long)]
    pub csv: Option<PathBuf>,

    /// If we should dump the combined records into a single json.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    ///
    /// If the path ends in `.gz` it will be gzip compressed
    #[arg(short, long)]
    pub json: Option<PathBuf>,

    /// If we should dump the combined records into a single yaml.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    ///
    /// If the path ends in `.gz` it will be gzip compressed
    #[arg(short, long)]
    pub yaml: Option<PathBuf>,

    /// If we should dump the unique paths/operations found into a csv
    ///
    /// We'll combine all of the operations for each path so there is one entry per path
    ///
    /// If the path ends in `.gz` it will be gzip compressed
    #[arg(short, long)]
    pub uniques: Option<PathBuf>,

    /// How many days we should pull (based off the file mod time)
    #[arg(short = 'd', long = "days", default_value = "90")]
    pub pull_days: u32,

    /// The fs event files that should be parsed. If any arg is a directory then any file within
    /// that has a filename consisting solely of hex chars will be considered a file to parse
    #[arg(default_value = "/System/Volumes/Data/.fseventsd/")]
    pub files: Vec<PathBuf>,

    /// The compression options
    #[clap(flatten)]
    pub compress_opts: CompressOpts,

    /// The filter options
    #[clap(flatten)]
    pub filter_opts: FilterOpts,
}

#[derive(Debug, Args)]
pub struct FilterOpts {
    /// Only show entries that have a path matching this regex
    #[arg(short = 'p', long = "path-filter")]
    pub filter_paths: Option<String>,

    /// Only show entries if any of the flags are present in the operation
    #[arg(short = 'f', long = "any-flags", conflicts_with = "all_flags")]
    pub any_flags: Vec<String>,

    /// Only show entries if all of the flags are present in the operation
    #[arg(long = "all-flags", conflicts_with = "any_flags")]
    pub all_flags: Vec<String>,
}

impl FilterOpts {
    pub fn filter(&self) -> RecordFilter {
        RecordFilter::new(&self.filter_paths, &self.any_flags, &self.all_flags)
    }
}

#[derive(Clone, Copy, Debug, Args)]
pub struct CompressOpts {
    /// The level we should compress the gzip output as; 0-9
    #[arg(short = 'l', alias = "level", long, default_value = "7")]
    pub glevel: u32,

    /// The level we should compress the zstd output as; 0-20
    #[cfg(feature = "zstd")]
    #[arg(long, default_value = "10")]
    pub zlevel: u32,

    /// How many threads to use for zstd compression (0 disables it)
    #[cfg(feature = "zstd")]
    #[arg(long, default_value = "2")]
    pub zthreads: u16,

    /// Force the output file (or stdout) to be gzip
    #[arg(long)]
    pub gzip: bool,

    /// Force the output file (or stdout) to be zstd
    #[cfg(feature = "zstd")]
    #[arg(long, conflicts_with = "gzip")]
    pub zstd: bool,
}

impl CompressOpts {
    pub fn glvl(&self) -> flate2::Compression {
        flate2::Compression::new(self.glevel)
    }

    #[cfg(feature = "zstd")]
    pub fn zlvl(&self) -> i32 {
        self.zlevel as i32
    }

    pub fn validate(&self) -> Result<()> {
        if self.glevel > 9 {
            return Err(eyre!(
                "The gzip compression level must be between 0 and 9 (inclusive)",
            ));
        }

        #[cfg(feature = "zstd")]
        if self.zlevel > 20 {
            return Err(eyre!(
                "The zstd compression level must be between 0 and 20 (inclusive)",
            ));
        }

        Ok(())
    }

    pub fn is_gz(&self, path: &Path) -> bool {
        self.gzip
            || match path.extension() {
                None => false,
                Some(e) => e == "gz" || e == "gzip",
            }
    }

    #[cfg(feature = "zstd")]
    pub fn is_zstd(&self, path: &Path) -> bool {
        self.zstd
            || match path.extension() {
                None => false,
                Some(e) => e == "zstd" || e == "zst",
            }
    }

    #[cfg(not(feature = "zstd"))]
    pub const fn is_zstd(&self, _: &Path) -> bool {
        false
    }

    pub fn make_gzip<W>(&self, w: W) -> flate2::write::GzEncoder<W>
    where
        W: Write,
    {
        flate2::write::GzEncoder::new(w, self.glvl())
    }

    #[cfg(feature = "zstd")]
    pub fn make_zstd<'a, W>(&self, w: W) -> zstd::stream::AutoFinishEncoder<'a, W>
    where
        W: Write,
    {
        let mut z = zstd::stream::write::Encoder::new(w, self.zlvl()).unwrap();
        z.multithread(self.zthreads as u32).unwrap();
        z.auto_finish()
    }

    pub fn make_stdout(&self) -> BufWriter<Box<dyn Write>> {
        let out = std::io::stdout().lock();

        #[cfg(feature = "zstd")]
        let is_zstd = self.zstd;
        #[cfg(not(feature = "zstd"))]
        let is_zstd = false;

        BufWriter::with_capacity(
            512,
            if is_zstd {
                #[cfg(feature = "zstd")]
                {
                    Box::new(self.make_zstd(out))
                }

                #[cfg(not(feature = "zstd"))]
                unreachable!("zstd feature not enabled");
            } else if self.gzip {
                Box::new(self.make_gzip(out))
            } else {
                Box::new(out)
            },
        )
    }
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
        self.compress_opts.validate()?;

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
                Zoned::now()
                    .checked_sub(Span::new().days(self.pull_days))
                    .expect("invalid date offset")
                    .start_of_day()
                    .expect("invalid date offset")
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
                                    // Do the filename check first since it's fast and doesn't do
                                    // any metadata reads
                                    if Dump::want_filename(e.file_name()) {
                                        let want_file = if let Ok(m) = e.metadata() {
                                            if !m.is_dir() {
                                                // See if we care about filtering by time
                                                if let Some(cut_time) = cutoff {
                                                    // Get the file mod/create time
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
                                                        // Weird metadata issue assume we want it
                                                        true
                                                    }
                                                } else {
                                                    // No time filters always include
                                                    true
                                                }
                                            } else {
                                                // Skip dirs
                                                false
                                            }
                                        } else {
                                            // Couldn't get metadata so not sure what this is, try
                                            // getting it anyway (very unlikely to get here)
                                            true
                                        };

                                        if want_file {
                                            debug!("Found the fs events file {:?}", e.path());
                                            files.push(e.into_path());
                                        }
                                    }
                                }

                                Err(err) => {
                                    error!("Error iterating the files: {err}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_opts_glvl() {
        let opts = CompressOpts {
            glevel: 5,
            #[cfg(feature = "zstd")]
            zlevel: 10,
            #[cfg(feature = "zstd")]
            zthreads: 2,
            gzip: false,
            #[cfg(feature = "zstd")]
            zstd: false,
        };

        let compression = opts.glvl();
        assert_eq!(compression.level(), 5);
    }

    #[test]
    fn test_compress_opts_validate_valid_gzip() {
        let opts = CompressOpts {
            glevel: 9,
            #[cfg(feature = "zstd")]
            zlevel: 10,
            #[cfg(feature = "zstd")]
            zthreads: 2,
            gzip: false,
            #[cfg(feature = "zstd")]
            zstd: false,
        };

        assert!(opts.validate().is_ok());
    }

    #[test]
    fn test_compress_opts_validate_invalid_gzip() {
        let opts = CompressOpts {
            glevel: 10, // Invalid: max is 9
            #[cfg(feature = "zstd")]
            zlevel: 10,
            #[cfg(feature = "zstd")]
            zthreads: 2,
            gzip: false,
            #[cfg(feature = "zstd")]
            zstd: false,
        };

        assert!(opts.validate().is_err());
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compress_opts_validate_valid_zstd() {
        let opts = CompressOpts {
            glevel: 5,
            zlevel: 20,
            zthreads: 4,
            gzip: false,
            zstd: false,
        };

        assert!(opts.validate().is_ok());
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compress_opts_validate_invalid_zstd() {
        let opts = CompressOpts {
            glevel: 5,
            zlevel: 21, // Invalid: max is 20
            zthreads: 2,
            gzip: false,
            zstd: false,
        };

        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_compress_opts_is_gz_by_extension() {
        let opts = CompressOpts {
            glevel: 5,
            #[cfg(feature = "zstd")]
            zlevel: 10,
            #[cfg(feature = "zstd")]
            zthreads: 2,
            gzip: false,
            #[cfg(feature = "zstd")]
            zstd: false,
        };

        assert!(opts.is_gz(Path::new("output.gz")));
        assert!(opts.is_gz(Path::new("output.gzip")));
        assert!(!opts.is_gz(Path::new("output.txt")));
        assert!(!opts.is_gz(Path::new("output")));
    }

    #[test]
    fn test_compress_opts_is_gz_forced() {
        let opts = CompressOpts {
            glevel: 5,
            #[cfg(feature = "zstd")]
            zlevel: 10,
            #[cfg(feature = "zstd")]
            zthreads: 2,
            gzip: true, // Force gzip
            #[cfg(feature = "zstd")]
            zstd: false,
        };

        // Should return true even without .gz extension
        assert!(opts.is_gz(Path::new("output.txt")));
        assert!(opts.is_gz(Path::new("output")));
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compress_opts_is_zstd_by_extension() {
        let opts = CompressOpts {
            glevel: 5,
            zlevel: 10,
            zthreads: 2,
            gzip: false,
            zstd: false,
        };

        assert!(opts.is_zstd(Path::new("output.zstd")));
        assert!(opts.is_zstd(Path::new("output.zst")));
        assert!(!opts.is_zstd(Path::new("output.txt")));
        assert!(!opts.is_zstd(Path::new("output.gz")));
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compress_opts_is_zstd_forced() {
        let opts = CompressOpts {
            glevel: 5,
            zlevel: 10,
            zthreads: 2,
            gzip: false,
            zstd: true, // Force zstd
        };

        assert!(opts.is_zstd(Path::new("output.txt")));
        assert!(opts.is_zstd(Path::new("output")));
    }

    #[test]
    fn test_dump_stdout_counts_none() {
        let dump = Dump {
            csvs: false,
            jsons: false,
            yamls: false,
            csv: Some(PathBuf::from("output.csv")),
            json: Some(PathBuf::from("output.json")),
            yaml: None,
            uniques: None,
            pull_days: 90,
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 7,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        assert_eq!(dump.stdout_counts(), 0);
    }

    #[test]
    fn test_dump_stdout_counts_one() {
        let dump = Dump {
            csvs: false,
            jsons: false,
            yamls: false,
            csv: Some(PathBuf::from("-")), // stdout
            json: Some(PathBuf::from("output.json")),
            yaml: None,
            uniques: None,
            pull_days: 90,
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 7,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        assert_eq!(dump.stdout_counts(), 1);
    }

    #[test]
    fn test_dump_validate_multiple_stdout() {
        let dump = Dump {
            csvs: false,
            jsons: false,
            yamls: false,
            csv: Some(PathBuf::from("-")),
            json: Some(PathBuf::from("-")),
            yaml: None,
            uniques: None,
            pull_days: 90,
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 7,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        let count = dump.stdout_counts();
        assert!(
            dump.validate(count).is_err(),
            "Should fail with multiple stdout"
        );
    }

    #[test]
    fn test_dump_validate_no_outputs() {
        let dump = Dump {
            csvs: false,
            jsons: false,
            yamls: false,
            csv: None,
            json: None,
            yaml: None,
            uniques: None,
            pull_days: 90,
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 7,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        let count = dump.stdout_counts();
        assert!(dump.validate(count).is_err(), "Should fail with no outputs");
    }

    #[test]
    fn test_dump_validate_valid_config() {
        let dump = Dump {
            csvs: true,
            jsons: false,
            yamls: false,
            csv: None,
            json: None,
            yaml: None,
            uniques: None,
            pull_days: 90,
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 5,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        let count = dump.stdout_counts();
        assert!(dump.validate(count).is_ok());
    }

    #[test]
    fn test_dump_want_filename_hex_only() {
        assert!(Dump::want_filename(OsStr::new("0123456789abcdef")));
        assert!(Dump::want_filename(OsStr::new("ABCDEF")));
        assert!(Dump::want_filename(OsStr::new("0")));
        assert!(Dump::want_filename(OsStr::new("deadbeef")));
    }

    #[test]
    fn test_dump_want_filename_invalid() {
        assert!(!Dump::want_filename(OsStr::new("not_hex")));
        assert!(!Dump::want_filename(OsStr::new("file.txt")));
        assert!(!Dump::want_filename(OsStr::new("123-456")));
        assert!(!Dump::want_filename(OsStr::new("12g34")));
        // Note: empty string actually returns true because .all() on empty iterator is true
        // This is technically correct behavior - no non-hex chars in an empty string!
    }

    #[test]
    fn test_dump_cutoff_time_zero_days() {
        let dump = Dump {
            csvs: false,
            jsons: false,
            yamls: false,
            csv: None,
            json: None,
            yaml: None,
            uniques: None,
            pull_days: 0, // No time filter
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 7,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        assert!(dump.cutoff_time().is_none());
    }

    #[test]
    fn test_dump_cutoff_time_with_days() {
        let dump = Dump {
            csvs: false,
            jsons: false,
            yamls: false,
            csv: None,
            json: None,
            yaml: None,
            uniques: None,
            pull_days: 30,
            files: vec![],
            compress_opts: CompressOpts {
                glevel: 7,
                #[cfg(feature = "zstd")]
                zlevel: 10,
                #[cfg(feature = "zstd")]
                zthreads: 2,
                gzip: false,
                #[cfg(feature = "zstd")]
                zstd: false,
            },
            filter_opts: FilterOpts {
                filter_paths: None,
                any_flags: vec![],
                all_flags: vec![],
            },
        };

        let cutoff = dump.cutoff_time();
        assert!(cutoff.is_some());
    }

    #[test]
    fn test_filter_opts_filter() {
        let filter_opts = FilterOpts {
            filter_paths: Some(r"^/test/".to_string()),
            any_flags: vec!["Modified".to_string()],
            all_flags: vec![],
        };

        let filter = filter_opts.filter();
        assert!(filter.path_rex.is_some());
        assert!(filter.any_flag > 0);
        assert_eq!(filter.all_flag, 0);
    }

    #[test]
    fn test_filter_opts_default() {
        let filter_opts = FilterOpts {
            filter_paths: None,
            any_flags: vec![],
            all_flags: vec![],
        };

        let filter = filter_opts.filter();
        assert!(filter.path_rex.is_none());
        assert_eq!(filter.any_flag, 0);
        assert_eq!(filter.all_flag, 0);
    }

    #[test]
    fn test_stdout_path_helper() {
        assert!(stdout_path(&Some(PathBuf::from("-"))));
        assert!(!stdout_path(&Some(PathBuf::from("file.txt"))));
        assert!(!stdout_path(&None));
    }

    #[cfg(feature = "zstd")]
    #[test]
    fn test_compress_opts_zlvl() {
        let opts = CompressOpts {
            glevel: 7,
            zlevel: 15,
            zthreads: 4,
            gzip: false,
            zstd: false,
        };

        assert_eq!(opts.zlvl(), 15);
    }
}
