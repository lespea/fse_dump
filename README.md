# Overview
FSEvents files are written to disk by macOS APIs and contain historical records of file system
activity that occurred for a particular volume. They can be found on devices running macOS and
devices that were plugged in to a device running macOS. *fse_dump* can be used to parse FSEvents
files from the '/.fseventsd/' on a live system or FSEvents files extracted from an image.


![Github CI](https://github.com/lespea/fse_dump/actions/workflows/ci.yml/badge.svg)
![Github Release](https://github.com/lespea/fse_dump/actions/workflows/release.yml/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/fse_dump.svg)](https://crates.io/crates/fse_dump)

## Usage
```
USAGE:
    fse_dump [FLAGS] [OPTIONS] <files>...

FLAGS:
        --csvs       If every fse record file we find should be dumped to a csv "next" to it (filename + .csv)
    -h, --help       Prints help information
        --jsons      If every fse record file we find should be dumped to a json "next" to it (filename + .json)
    -V, --version    Prints version information

OPTIONS:
    -c, --csv <csv>           If we should dump the combined records into a single csv.
                              
                              The records will be dumped in the order that they're given on the command line (any dir
                              that is given is expanded to the record files within).
                              
                              If parallel is enabled than there is no guarantee of order (even within a single file)
    -j, --json <json>         If we should dump the combined records into a single json.
                              
                              The records will be dumped in the order that they're given on the command line (any dir
                              that is given is expanded to the record files within).
                              
                              If parallel is enabled than there is no guarantee of order (even within a single file)
    -u, --unique <uniques>    If we should dump the unique paths/operations found into a csv
                              
                              We'll combine all of the operations for each path so there is one entry per path

ARGS:
    <files>...    The fs event files that should be parsed. If any arg is a directory then any file within that has
                  a filename consisting solely of hex chars will be considered a file to parse
```

## References
* http://nicoleibrahim.com/apple-fsevents-forensics/
* https://github.com/dlcowen/FSEventsParser

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
