use structopt::StructOpt;
use walkdir;

use io::{self, ErrorKind};
use std::path::PathBuf;

#[derive(Debug, StructOpt)]
#[structopt()]
pub struct Opts {
    /// If every fse record file we find should be dumped to a csv "next" to it (filename + .csv)
    #[structopt(long = "csvs")]
    pub csvs: bool,

    /// If every fse record file we find should be dumped to a json "next" to it (filename + .json)
    #[structopt(long = "jsons")]
    pub jsons: bool,

    /// If we should dump the combined records into a single csv.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    #[structopt(short = "c", long = "csv", parse(from_os_str))]
    pub csv: Option<PathBuf>,

    /// If we should dump the combined records into a single json.
    ///
    /// The records will be dumped in the order that they're given on the command line (any dir
    /// that is given is expanded to the record files within).
    ///
    /// If parallel is enabled than there is no guarantee of order (even within a single file)
    #[structopt(short = "j", long = "json", parse(from_os_str))]
    pub json: Option<PathBuf>,

    /// If we should dump the unique paths/operations found into a csv
    ///
    /// We'll combine all of the operations for each path so there is one entry per path
    #[structopt(short = "u", long = "unique", parse(from_os_str))]
    pub uniques: Option<PathBuf>,

    /// If we should process things in parallel.  If outputting to a csv/json file no order is
    /// guaranteed.
    #[structopt(short = "p", long = "parallel")]
    pub parallel: bool,

    /// The fs event files that should be parsed. If any arg is a directory then any file within
    /// that has a filename consisting solely of hex chars will be considered a file to parse
    #[structopt(parse(from_os_str), raw(required = "true", min_values = "1"))]
    pub files: Vec<PathBuf>,
}

fn stdout_path(path: &Option<PathBuf>) -> bool {
    if let Some(p) = path {
        p.to_string_lossy() == "-"
    } else {
        false
    }
}

impl Opts {
    fn validate(&self) -> io::Result<()> {
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

        if counts > 1 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Can't have more than one file printing to stdout!",
            ));
        }

        if !(self.csvs
            || self.jsons
            || self.csv.is_some()
            || self.json.is_some()
            || self.uniques.is_some())
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "You must specify at least one output type!",
            ));
        }

        Ok(())
    }

    pub fn real_files(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.files.iter().flat_map(|path| {
            walkdir::WalkDir::new(path)
                .max_depth(1)
                .follow_links(true)
                .sort_by(|a, b| a.file_name().cmp(b.file_name()))
                .into_iter()
                .filter_map(|e| match e {
                    Ok(e) =>
                    if let Ok(m) = e.metadata() {
                         if !m.is_dir() && e.file_name()
                                .to_string_lossy()
                                .chars()
                                .all(|c| match c {
                                    'a'..='f' | 'A'..='F' | '0'..='9' => true,
                                    _ => false,
                                }) {
                             info!("Found the fs events file {:?}", e.path());
                                Some(e.into_path())
                            } else {
                                None
                            }
                    } else {
                        None
                    },

                    Err(err) => {
                        error!("Error iterating the files: {}", err);
                        None
                    }
                })
        })
    }
}

pub fn get_opts() -> io::Result<Opts> {
    let o = Opts::from_args();
    o.validate()?;
    Ok(o)
}
