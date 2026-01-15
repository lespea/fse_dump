# fse_dump

FSEvents files are written to disk by macOS APIs and contain historical records
of file system activity that occurred for a particular volume. They can be
found on devices running macOS and devices that were plugged in to a device
running macOS. **fse_dump** can be used to parse FSEvents files from the
`/System/Volumes/Data/.fseventsd/` on a live system or FSEvents files
extracted from an image.

![Github CI](https://github.com/lespea/fse_dump/actions/workflows/ci.yml/badge.svg)
![Github Release](https://github.com/lespea/fse_dump/actions/workflows/release.yml/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/fse_dump.svg)](https://crates.io/crates/fse_dump)

## Features

- Parse FSEvents files from macOS (versions 1, 2, and 3)
- Export to multiple formats: CSV, JSON, YAML
- Filter events by path (regex) and flags
- Compress output with gzip or zstd
- Watch mode for real-time parsing of new FSEvents files
- Generate unique path/operation summaries
- Fast parallel processing with memory-efficient design

## Installation

### From crates.io

```bash
cargo install fse_dump
```

### From source

```bash
git clone https://github.com/lespea/fse_dump
cd fse_dump
cargo build --release
```

### With optional features

```bash
# Build with zstd compression support
cargo install fse_dump --features zstd

# Build with watch mode (requires notify)
cargo install fse_dump --features watch

# Build with all features
cargo install fse_dump --all-features
```

## Quick Start

### Parse FSEvents to JSON

```bash
# Parse default FSEvents directory to JSON
fse_dump dump --json output.json

# Parse with compression
fse_dump dump --json output.json.gz

# Parse specific files
fse_dump dump --json output.json /path/to/fsevent/file1 /path/to/file2
```

### Filter Events

```bash
# Filter by path (regex)
fse_dump dump --json output.json -p ".*\.pdf$"

# Filter by any of the specified flags
fse_dump dump --json output.json -f Created Modified

# Filter requiring all specified flags
fse_dump dump --json output.json --all-flags FileEvent Modified

# Combine filters
fse_dump dump --json output.json \
  -p "/Users/.*" \
  -f Created Removed
```

### Watch Mode

```bash
# Watch for new FSEvents files and output as JSON
fse_dump watch

# Watch with filters
fse_dump watch -o json -P \
  -p ".*\.docx?$" \
  -f Modified
```

## Usage

```
Usage: fse_dump <COMMAND>

Commands:
  dump      Dump fsevents file into the wanted output files/format
  watch     Watch for new fse files, parse them, and write them to the desired output
  generate  Outputs shell completions for the desired shell
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Dump Command

The `dump` command parses FSEvents files and outputs them in various formats.

```bash
fse_dump dump [OPTIONS] [FILES]...
```

#### Arguments

- `[FILES]...` - The FSEvents files to parse (default: `/System/Volumes/Data/.fseventsd/`)
  - Can be individual files or directories
  - Directories are scanned for files with hex-only filenames
  - Files are sorted by name before processing

#### Output Format Options

**Individual File Output** (creates output file next to each input file):

- `--csvs` - Create `.csv` file for each FSEvents file
- `--jsons` - Create `.json` file for each FSEvents file  
- `--yamls` - Create `.yaml` file for each FSEvents file

**Combined Output** (all records in one file):

- `-c, --csv <FILE>` - Write all records to a single CSV file
- `-j, --json <FILE>` - Write all records to a single JSON file
- `-y, --yaml <FILE>` - Write all records to a single YAML file
- `-u, --uniques <FILE>` - Write unique paths with combined operations to CSV

Use `-` as the filename to write to stdout:

```bash
fse_dump dump --json - | jq .
```

#### Compression Options

**Automatic Compression** (based on file extension):

```bash
# Gzip compression (automatic)
fse_dump dump --json output.json.gz

# Zstd compression (automatic, requires zstd feature)
fse_dump dump --json output.json.zst
```

**Force Compression**:

- `--gzip` - Force gzip compression (even for stdout)
- `--zstd` - Force zstd compression (requires zstd feature)
- `-l, --glevel <0-9>` - Gzip compression level (default: 7)
- `--zlevel <0-20>` - Zstd compression level (default: 10)
- `--zthreads <N>` - Zstd threads (default: 2, 0 to disable)

#### Time Filtering

- `-d, --days <N>` - Only process files modified in the last N days (default: 90)
  - Set to 0 to process all files regardless of age
  - Based on file modification/creation time

#### Event Filtering

Filter which events are included in the output:

**Path Filtering**:

- `-p, --path-filter <REGEX>` - Only include events matching the regex pattern

```bash
# Only PDF files
fse_dump dump --json output.json -p "\.pdf$"

# Only files in /Users directory
fse_dump dump --json output.json --path-filter "^/Users/"

# Multiple patterns (use regex alternation)
fse_dump dump --json output.json -p "\.(pdf|docx?|xlsx?)$"
```

**Flag Filtering**:

- `-f, --any-flags <FLAG>...` - Include events with ANY of these flags
- `--all-flags <FLAG>...` - Include events with ALL of these flags

These options are mutually exclusive.

**Available Flags**:

| Flag | Description |
|------|-------------|
| `FolderEvent` | Event occurred on a folder |
| `Mount` | Volume was mounted |
| `Unmount` | Volume was unmounted |
| `EndOfTransaction` | End of a transaction |
| `LastHardLinkRemoved` | Last hard link to file removed |
| `HardLink` | Hard link created |
| `SymbolicLink` | Symbolic link created |
| `FileEvent` | Event occurred on a file |
| `PermissionChange` | Permissions were changed |
| `ExtendedAttrModified` | Extended attributes modified |
| `ExtendedAttrRemoved` | Extended attributes removed |
| `DocumentRevisioning` | Document versioning event |
| `ItemCloned` | Item was cloned |
| `Created` | File/folder was created |
| `Removed` | File/folder was removed |
| `InodeMetaMod` | Inode metadata modified |
| `Renamed` | File/folder was renamed |
| `Modified` | File/folder was modified |
| `Exchange` | Files exchanged |
| `FinderInfoMod` | Finder info modified |
| `FolderCreated` | Folder was created |

**Flag names are case-insensitive.**

**Examples**:

```bash
# Find all file creation or removal events
fse_dump dump --json output.json -f Created Removed

# Find all modified files (not folders)
fse_dump dump --json output.json --all-flags FileEvent Modified

# Find files created in the Documents folder
fse_dump dump --json output.json \
  -p "/Documents/" \
  -f Created

# Find permission changes on system files
fse_dump dump --json output.json \
  -p "^/(System|Library)/" \
  -f PermissionChange
```

#### Complete Examples

```bash
# Parse last 30 days to compressed JSON
fse_dump dump --days 30 --json events.json.gz

# Create CSV and JSON for each FSEvents file
fse_dump dump --csvs --jsons

# Export unique paths to CSV
fse_dump dump --uniques unique_paths.csv

# Parse specific file to stdout with filters
fse_dump dump --json - \
  -p "/Users/alice/" \
  -f Modified Created \
  /path/to/fsevent/file

# Multiple outputs with compression
fse_dump dump \
  --json all_events.json.gz \
  --csv all_events.csv.gz \
  --uniques unique_paths.csv \
  --days 7
```

### Watch Command

The `watch` command monitors directories for new FSEvents files and parses them in real-time.

```bash
fse_dump watch [OPTIONS] [WATCH_DIRS]...
```

#### Arguments

- `[WATCH_DIRS]...` - Directories to watch (default: `/System/Volumes/Data/.fseventsd/`)

#### Options

- `-o, --format <FORMAT>` - Output format: `csv`, `json`, or `yaml` (default: `json`)
- `-P, --pretty` - Pretty-print JSON output (multi-line formatting)
- `--poll` - Use polling instead of native file system events (slower but more compatible)

**Compression options** (same as dump command):
- `--gzip`, `--zstd`, `-l, --glevel`, `--zlevel`, `--zthreads`

**Filtering options** (same as dump command):
- `-p, --path-filter <REGEX>`
- `-f, --any-flags <FLAG>...`
- `--all-flags <FLAG>...`

#### Examples

```bash
# Watch default directory and output JSON to stdout
fse_dump watch

# Watch with pretty-printed JSON
fse_dump watch -o json -P

# Watch and filter for document changes
fse_dump watch \
  -p "\.(doc|pdf|txt)$" \
  -f Modified Created

# Watch custom directory with CSV output
fse_dump watch -o csv /custom/fsevents/path

# Watch with compression (pipe to file)
fse_dump watch --gzip > events.json.gz
```

### Generate Command

Generate shell completion scripts for various shells.

```bash
fse_dump generate <SHELL>
```

#### Supported Shells

- `bash`
- `elvish`
- `fish`
- `powershell`
- `zsh`

#### Examples

```bash
# Generate completions for bash
fse_dump generate bash > ~/.local/share/bash-completion/completions/fse_dump

# Generate completions for zsh
fse_dump generate zsh > ~/.zsh/completions/_fse_dump

# Generate completions for fish
fse_dump generate fish > ~/.config/fish/completions/fse_dump.fish
```

## Output Format

### Record Fields

Each FSEvents record contains the following fields:

```json
{
  "path": "/Users/alice/Documents/file.txt",
  "event_id": "0x12ab34cd",
  "flags": "FileEvent | Modified",
  "node_id": "0x56ef78",
  "extra_id": "0x9abc"
}
```

- `path` - Full path to the file/folder
- `event_id` - Unique event identifier (hex format if built with `hex` feature)
- `flags` - Human-readable flag names separated by ` | `
- `alt_flags` - Alternative flag interpretation (if built with `alt_flags` feature)
- `node_id` - Inode number (v2 and v3 only, hex format if built with `hex` feature)
- `extra_id` - Additional ID (v3 only, requires `extra_id` feature)

### Unique Output Format

The `--uniques` option produces aggregated records:

```csv
path,counts,flags
/Users/alice/file.txt,5,"FileEvent | Modified | Created"
/Users/alice/Documents,3,"FolderEvent | Modified"
```

- `path` - The file/folder path
- `counts` - Number of events for this path
- `flags` - Combined flags (bitwise OR of all events)

## Advanced Usage

### Filtering Complex Scenarios

**Find all deletions in user directories**:
```bash
fse_dump dump --json deletions.json \
  -p "^/Users/" \
  -f Removed
```

**Find renamed files (with old and new names)**:
```bash
fse_dump dump --json renames.json \
  -f Renamed
```

**Monitor system configuration changes**:
```bash
fse_dump dump --json system_changes.json \
  -p "^/(System|Library|etc)/" \
  -f Modified PermissionChange
```

**Find cloned/copied files**:
```bash
fse_dump dump --json clones.json \
  -f ItemCloned
```

### Combining with Other Tools

**jq for JSON processing**:
```bash
# Extract just the paths
fse_dump dump --json - | jq -r '.path'

# Find events for a specific user
fse_dump dump --json - | jq 'select(.path | startswith("/Users/alice"))'

# Count events by flag
fse_dump dump --json - | jq -r '.flags' | sort | uniq -c
```

**grep/awk for quick filtering**:
```bash
# Find all PDF operations
fse_dump dump --json - | grep '\.pdf"'

# CSV processing with awk
fse_dump dump --csv - | awk -F',' '$3 ~ /Created/ {print $1}'
```

### Performance Tips

1. **Use compression** for large outputs to save disk space
2. **Use `--days`** to limit processing to recent files
3. **Apply filters early** with `--path-filter` and `--any-flags` to reduce output size
4. **Use CSV** for the most compact output format
5. **Use `--uniques`** when you only need summary statistics

### Forensics Use Cases

**Timeline analysis**:
```bash
# Export everything from last 7 days
fse_dump dump --days 7 --json timeline.json.gz
```

**Malware detection** (find suspicious file operations):
```bash
# Find new executables
fse_dump dump --json suspicious.json \
  -p "\.(app|exe|sh|command|pkg|dmg)$" \
  -f Created

# Find hidden files
fse_dump dump --json hidden.json \
  -p "/\.[^/]+$" \
  -f Created Modified
```

**Data exfiltration** (find removable media):
```bash
# Monitor mounts/unmounts
fse_dump dump --json removable.json \
  -f Mount Unmount
```

**User activity**:
```bash
# Monitor specific user's home directory
fse_dump dump --json user_activity.json \
  -p "^/Users/targetuser/" \
  --days 30
```

## Building from Source

### Features

Optional features can be enabled during build:

- `zstd` - Enable zstd compression support
- `watch` - Enable watch mode for real-time monitoring
- `hex` - Output numeric IDs in hexadecimal format
- `alt_flags` - Include alternative flag interpretations
- `extra_id` - Include extra_id field from v3 files

```bash
# Build with specific features
cargo build --release --features "zstd,watch"

# Build with all features
cargo build --release --all-features
```

### Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- dump --json output.json

# Check code
cargo clippy
cargo fmt
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
