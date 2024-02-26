# Overview

FSEvents files are written to disk by macOS APIs and contain historical records of file system
activity that occurred for a particular volume. They can be found on devices running macOS and
devices that were plugged in to a device running macOS. _fse_dump_ can be used to parse FSEvents
files from the '/.fseventsd/' on a live system or FSEvents files extracted from an image.

![Github CI](https://github.com/lespea/fse_dump/actions/workflows/ci.yml/badge.svg)
![Github Release](https://github.com/lespea/fse_dump/actions/workflows/release.yml/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/fse_dump.svg)](https://crates.io/crates/fse_dump)

## Usage

```
Usage: fse_dump <COMMAND>

Commands:
  dump      Dump the known net defs
  generate  Outputs shell completion for fish
  watch     Watch for new fse files, parse them, and write them to the desired output
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

#### Dump

```
Usage: fse_dump dump [OPTIONS] [FILES]...

Arguments:
  [FILES]...
          The fs event files that should be parsed. If any arg is a directory then any file within that has a filename consisting solely of hex chars will be considered a file to parse
          
          [default: /System/Volumes/Data/.fseventsd/]

Options:
      --csvs
          If every fse record file we find should be dumped to a csv "next" to it (filename + .csv)

      --jsons
          If every fse record file we find should be dumped to a json "next" to it (filename + .json)

      --yamls
          If every fse record file we find should be dumped to a yaml "next" to it (filename + .yaml)

  -c, --csv <CSV>
          If we should dump the combined records into a single csv.
          
          The records will be dumped in the order that they're given on the command line (any dir that is given is expanded to the record files within).
          
          If parallel is enabled than there is no guarantee of order (even within a single file)

  -j, --json <JSON>
          If we should dump the combined records into a single json.
          
          The records will be dumped in the order that they're given on the command line (any dir that is given is expanded to the record files within).
          
          If parallel is enabled than there is no guarantee of order (even within a single file)

  -y, --yaml <YAML>
          If we should dump the combined records into a single yaml.
          
          The records will be dumped in the order that they're given on the command line (any dir that is given is expanded to the record files within).
          
          If parallel is enabled than there is no guarantee of order (even within a single file)

  -u, --uniques <UNIQUES>
          If we should dump the unique paths/operations found into a csv
          
          We'll combine all of the operations for each path so there is one entry per path

  -l, --level <LEVEL>
          The level we should compress the output as; 0-9
          
          [default: 7]

  -d, --days <PULL_DAYS>
          How many days we should pull (based off the file mod time)
          
          [default: 90]

  -h, --help
          Print help (see a summary with '-h')
```

#### Watch

```
Usage: fse_dump watch [OPTIONS] [WATCH_DIRS]...

Arguments:
  [WATCH_DIRS]...  The dirs to watch [default: /System/Volumes/Data/.fseventsd/]

Options:
  -f, --format <FORMAT>  The format the parsed files should be output to [default: json] [possible values: csv, json, yaml]
  -p, --pretty           If the outupt should be "pretty" formatted (multi-line)
      --filter <FILTER>  Filter events based on the path
      --poll             Use polling (performance issues only use if the normal watcher doesn't work)
  -h, --help             Print help
```

#### Gen

```
Usage: fse_dump generate <SHELL>

Arguments:
  <SHELL>  If every fse record file we find should be dumped to a csv "next" to it (filename + .csv) [possible values: bash, elvish, fish, powershell, zsh]

Options:
  -h, --help  Print help
```

## References

- http://nicoleibrahim.com/apple-fsevents-forensics/
- https://github.com/dlcowen/FSEventsParser

## License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
