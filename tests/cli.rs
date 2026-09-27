//! End-to-end checks of the command line surface

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output},
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

/// A fresh scratch directory per test
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fse_dump-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
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
    // The fixture was written in 2024, so the default 90 day window excludes it. Write to a
    // file rather than stdout so warnings are not silenced.
    let dir = scratch("old-files");
    let target = dir.join("out.json");
    let out = fse_dump(&["dump", "--json", path_str(&target), "testfiles/v3"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("Skipped 1 file"), "{err}");
    assert!(err.contains("No fsevents files found"), "{err}");
    assert!(
        !target.exists(),
        "no output file should be created when nothing is parsed"
    );

    let out = fse_dump(&["dump", "--json", "-", "--days", "0", "testfiles/v3"]);
    assert!(out.status.success(), "{}", stderr(&out));
    // The directory scan only picks up hex-named files, so test_1.gz is not parsed twice
    assert_eq!(stdout_lines(&out).len(), FIXTURE_RECORDS);
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
