//! End-to-end checks of the command line surface

use std::{
    fs,
    io::{Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, SystemTime},
};

const FIXTURE: &str = "testfiles/v3/000000000342c4f2";
const FIXTURE_RECORDS: usize = 2730;

fn fse_dump(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fse_dump"))
        .args(args)
        .output()
        .expect("failed to run fse_dump")
}

fn stdout_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A fresh scratch directory per test, removed again when the test is done
struct Scratch(PathBuf);

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("fse_dump-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn set_a_year_old(path: &Path) {
    let a_year_ago = SystemTime::now() - Duration::from_secs(365 * 24 * 60 * 60);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(a_year_ago)
        .unwrap();
}

/// A gzipped v2 page holding `paths`, cut `chop` bytes short of its declared length
fn truncated_v2_log(dir: &Path, paths: &[&str], chop: usize) -> PathBuf {
    let mut body = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        body.extend_from_slice(p.as_bytes());
        body.push(0);
        body.extend_from_slice(&(i as u64 + 1).to_le_bytes()); // event id
        body.extend_from_slice(&0x1000_0000u32.to_be_bytes()); // Modified
        body.extend_from_slice(&(i as u64 + 100).to_le_bytes()); // node id
    }
    let mut page = b"2SLD\0\0\0\0".to_vec();
    page.extend_from_slice(&((12 + body.len()) as u32).to_le_bytes());
    page.extend_from_slice(&body);
    page.truncate(page.len() - chop);

    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&page).unwrap();
    let path = dir.join("0000000000000010");
    fs::write(&path, gz.finish().unwrap()).unwrap();
    path
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn json_to_stdout_has_every_record() {
    let out = fse_dump(&["dump", "--json", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout_lines(&out).len(), FIXTURE_RECORDS);
}

#[test]
fn event_ids_sit_below_the_file_name() {
    let out = fse_dump(&["dump", "--json", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));

    let ids: Vec<u64> = stdout_lines(&out)
        .iter()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            // A hex string with the `hex` feature, a plain number without it
            match &v["event_id"] {
                serde_json::Value::String(id) => {
                    u64::from_str_radix(id.trim_start_matches("0x"), 16).unwrap()
                }
                serde_json::Value::Number(n) => n.as_u64().unwrap(),
                other => panic!("unexpected event_id {other:?}"),
            }
        })
        .collect();

    let max = ids.iter().max().unwrap();
    let min = ids.iter().min().unwrap();
    assert!(
        *max < 0x342c4f2,
        "max id {max:#x} must be below the file name"
    );
    assert!(
        *min > 0x3400000,
        "min id {min:#x} should be close to the file name"
    );
}

#[test]
fn yaml_alone_is_a_valid_output() {
    let out = fse_dump(&["dump", "--yaml", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    let docs = stdout_lines(&out).iter().filter(|l| *l == "---").count();
    assert_eq!(docs, FIXTURE_RECORDS);
}

#[test]
fn two_stdout_outputs_are_rejected() {
    let out = fse_dump(&["dump", "--yaml", "-", "--json", "-", FIXTURE]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("more than one"), "{}", stderr(&out));
}

#[test]
fn no_output_type_is_rejected() {
    let out = fse_dump(&["dump", FIXTURE]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("at least one output"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn missing_input_fails() {
    let out = fse_dump(&["dump", "--json", "-", "/definitely/not/here"]);
    assert!(!out.status.success());
    assert!(stdout_lines(&out).is_empty());
}

#[test]
fn unreadable_input_does_not_stop_the_others() {
    let out = fse_dump(&["dump", "--json", "-", "/definitely/not/here", FIXTURE]);
    assert!(!out.status.success(), "a bad input must fail the run");
    assert!(
        stderr(&out).contains("/definitely/not/here"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        stdout_lines(&out).len(),
        FIXTURE_RECORDS,
        "the readable input is still parsed"
    );
}

#[test]
fn truncated_input_emits_what_it_can_and_fails() {
    let dir = scratch("truncated");
    let log = truncated_v2_log(&dir, &["/a", "/b", "/c"], 5);

    let out = fse_dump(&["dump", "--json", "-", path_str(&log)]);
    assert!(!out.status.success(), "truncation must fail the run");
    assert!(stderr(&out).contains("truncated"), "{}", stderr(&out));
    let lines = stdout_lines(&out);
    assert_eq!(
        lines.len(),
        2,
        "the complete records are still written: {lines:?}"
    );
}

#[test]
fn unknown_flag_name_fails() {
    let out = fse_dump(&["dump", "--json", "-", "-f", "Bogus", FIXTURE]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("Bogus"), "{}", stderr(&out));
}

#[test]
fn unparseable_input_fails() {
    let dir = scratch("unparseable");
    let bad = dir.join("0000000000000001");
    fs::write(&bad, b"this is not a gzip file").unwrap();

    let out = fse_dump(&["dump", "--json", "-", path_str(&bad)]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("gzip"), "{}", stderr(&out));
}

#[test]
fn old_files_in_a_directory_are_skipped_with_a_warning() {
    // A checkout does not preserve mtimes, so build a directory with one log a year old and
    // one fresh copy, plus a decoy that the hex-name scan must ignore.
    let dir = scratch("old-files");
    let logs = dir.join("logs");
    fs::create_dir(&logs).unwrap();
    let old_log = logs.join("000000000342c4f2");
    fs::copy(FIXTURE, &old_log).unwrap();
    fs::copy(FIXTURE, logs.join("000000000342c4f3")).unwrap();
    fs::copy(FIXTURE, logs.join("not-a-log.gz")).unwrap();
    set_a_year_old(&old_log);

    // The skip is reported even when the records go to stdout
    let out = fse_dump(&["dump", "--json", "-", path_str(&logs)]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout_lines(&out).len(), FIXTURE_RECORDS);
    assert!(stderr(&out).contains("Skipped 1 file"), "{}", stderr(&out));

    let out = fse_dump(&["dump", "--json", "-", "--days", "0", path_str(&logs)]);
    assert!(out.status.success(), "{}", stderr(&out));
    // Both logs are parsed and the decoy is not
    assert_eq!(stdout_lines(&out).len(), 2 * FIXTURE_RECORDS);
}

#[test]
fn nothing_left_after_the_days_cutoff_fails() {
    let dir = scratch("all-old");
    let logs = dir.join("logs");
    fs::create_dir(&logs).unwrap();
    let old_log = logs.join("000000000342c4f2");
    fs::copy(FIXTURE, &old_log).unwrap();
    set_a_year_old(&old_log);

    let target = dir.join("out.json");
    let out = fse_dump(&["dump", "--json", path_str(&target), path_str(&logs)]);
    assert!(!out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("Skipped 1 file"), "{err}");
    assert!(err.contains("No fsevents files found"), "{err}");
    assert!(
        !target.exists(),
        "no output file should be created when nothing is parsed"
    );
}

#[test]
fn uncreatable_output_fails() {
    let out = fse_dump(&["dump", "--json", "/definitely/not/here/out.json", FIXTURE]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("Couldn't create"), "{}", stderr(&out));
}

#[test]
fn per_file_outputs_land_next_to_the_input() {
    let dir = scratch("per-file");
    let input = dir.join("000000000342c4f2");
    fs::copy(FIXTURE, &input).unwrap();

    let out = fse_dump(&["dump", "--csvs", "--jsons", "--yamls", path_str(&input)]);
    assert!(out.status.success(), "{}", stderr(&out));

    let csv = fs::read_to_string(dir.join("000000000342c4f2.csv")).unwrap();
    assert_eq!(
        csv.lines().count(),
        FIXTURE_RECORDS + 1,
        "csv has a header row"
    );
    let header = csv.lines().next().unwrap();
    assert!(header.starts_with("path,event_id,flags,"), "{header}");
    assert_eq!(
        header.contains(",alt_flags,"),
        cfg!(feature = "alt_flags"),
        "{header}"
    );
    assert!(header.contains(",node_id,"), "{header}");

    let json = fs::read_to_string(dir.join("000000000342c4f2.json")).unwrap();
    assert_eq!(json.lines().count(), FIXTURE_RECORDS);

    let yaml = fs::read_to_string(dir.join("000000000342c4f2.yaml")).unwrap();
    assert_eq!(
        yaml.lines().filter(|l| *l == "---").count(),
        FIXTURE_RECORDS
    );
}

#[test]
fn gz_extension_produces_valid_gzip() {
    let dir = scratch("gz");
    let target = dir.join("out.json.gz");

    let out = fse_dump(&["dump", "--json", path_str(&target), FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));

    let mut text = String::new();
    flate2::read::GzDecoder::new(fs::File::open(&target).unwrap())
        .read_to_string(&mut text)
        .expect("output is a complete gzip stream");
    assert_eq!(text.lines().count(), FIXTURE_RECORDS);
}

#[test]
fn uniques_aggregate_by_path() {
    let out = fse_dump(&["dump", "--uniques", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    let lines = stdout_lines(&out);
    let alt = if cfg!(feature = "alt_flags") {
        ",alt_flags"
    } else {
        ""
    };
    assert_eq!(lines[0], format!("path,counts,flags{alt}"));
    assert!(lines.len() > 1 && lines.len() <= FIXTURE_RECORDS + 1);

    let out = fse_dump(&["dump", "--uniques", "-", "--unique-timestamps", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout_lines(&out)[0],
        format!("path,counts,flags{alt},earliest_timestamp,latest_timestamp")
    );
}

#[test]
fn generate_describes_the_shell_argument() {
    let out = fse_dump(&["generate", "--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("shell to generate completions for"), "{help}");
    assert!(!help.contains("csv"), "{help}");
}
