use std::path::PathBuf;
use structopt::StructOpt;

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

    /// If we should process things in parallel.  If outputting to a csv/json file no order is
    /// guaranteed.
    #[structopt(short = "p", long = "parallel")]
    pub parallel: bool,

    /// The fs event files that should be parsed. If any arg is a directory then any file within
    /// that has a filename consisting solely of hex chars will be considered a file to parse
    #[structopt(parse(from_os_str), raw(required = "true", min_values = "1"))]
    pub files: Vec<PathBuf>,
}

pub fn get_opts() -> Opts {
    Opts::from_args()
}
