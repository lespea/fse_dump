//! End-to-end checks of the command line surface

use std::{fs, io::Read};

use serde::Deserialize;

mod common;
use common::*;

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
    let log = v2_log(&dir, "0000000000000010", &["/a", "/b", "/c"], 5);

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

#[test]
fn csv_to_stdout_has_a_header_and_every_record() {
    let out = fse_dump(&["dump", "--csv", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));

    let mut rdr = csv::Reader::from_reader(out.stdout.as_slice());
    let header: Vec<_> = rdr.headers().unwrap().iter().map(str::to_owned).collect();
    assert_eq!(&header[..3], ["path", "event_id", "flags"], "{header:?}");
    assert_eq!(rdr.records().count(), FIXTURE_RECORDS);
}

#[test]
fn yaml_to_stdout_is_a_document_stream() {
    let out = fse_dump(&["dump", "--yaml", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));

    let text = String::from_utf8(out.stdout).unwrap();
    let docs = serde_yaml_ng::Deserializer::from_str(&text)
        .map(|doc| serde_yaml_ng::Value::deserialize(doc).expect("each document parses"))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), FIXTURE_RECORDS);
    assert!(docs.iter().all(|d| d.get("path").is_some()));
}

#[test]
fn gzip_flag_compresses_stdout() {
    let out = fse_dump(&["dump", "--json", "-", "--gzip", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));

    let mut text = String::new();
    flate2::read::GzDecoder::new(out.stdout.as_slice())
        .read_to_string(&mut text)
        .expect("stdout is a complete gzip stream");
    assert_eq!(text.lines().count(), FIXTURE_RECORDS);
}

#[cfg(feature = "zstd")]
#[test]
fn zstd_outputs_are_valid_by_extension_and_by_flag() {
    let dir = scratch("zst");
    let target = dir.join("out.json.zst");

    let out = fse_dump(&["dump", "--json", path_str(&target), FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    let bytes = zstd::decode_all(fs::File::open(&target).unwrap()).expect("a complete zstd frame");
    assert_eq!(
        String::from_utf8(bytes).unwrap().lines().count(),
        FIXTURE_RECORDS
    );

    let out = fse_dump(&["dump", "--json", "-", "--zstd", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    let bytes = zstd::decode_all(out.stdout.as_slice()).expect("a complete zstd frame");
    assert_eq!(
        String::from_utf8(bytes).unwrap().lines().count(),
        FIXTURE_RECORDS
    );
}

#[test]
fn compression_levels_are_validated() {
    let out = fse_dump(&["dump", "--json", "-", "--glevel", "15", FIXTURE]);
    assert!(!out.status.success());
    assert!(
        stdout_lines(&out).is_empty(),
        "nothing is parsed before the options are checked"
    );

    #[cfg(feature = "zstd")]
    {
        let out = fse_dump(&["dump", "--json", "-", "--zlevel", "99", FIXTURE]);
        assert!(!out.status.success());
        assert!(stdout_lines(&out).is_empty());
    }
}

/// The flag names of every record in the fixture, in file order
///
/// Names are compared whole: "FolderCreated" is not "Created".
fn fixture_flags() -> Vec<Vec<String>> {
    let out = fse_dump(&["dump", "--json", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    json_lines(&out).iter().map(flag_names).collect()
}

fn flag_names(v: &serde_json::Value) -> Vec<String> {
    v["flags"]
        .as_str()
        .unwrap()
        .split(" | ")
        .map(str::to_owned)
        .collect()
}

#[test]
fn any_flags_keep_records_with_at_least_one_of_them() {
    let out = fse_dump(&[
        "dump",
        "--json",
        "-",
        "-f",
        "Created",
        "-f",
        "ItemCloned",
        FIXTURE,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));

    let got = json_lines(&out);
    let has = |f: &[String]| f.iter().any(|n| n == "Created" || n == "ItemCloned");
    assert!(got.iter().all(|v| has(&flag_names(v))));
    let expected = fixture_flags().iter().filter(|f| has(f)).count();
    assert_eq!(got.len(), expected);
    assert!(
        expected > 0 && expected < FIXTURE_RECORDS,
        "the filter must bite"
    );
}

#[test]
fn all_flags_require_every_one_of_them() {
    let out = fse_dump(&[
        "dump",
        "--json",
        "-",
        "--all-flags",
        "Created",
        "--all-flags",
        "Removed",
        FIXTURE,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));

    let got = json_lines(&out);
    let has = |f: &[String]| f.iter().any(|n| n == "Created") && f.iter().any(|n| n == "Removed");
    assert!(got.iter().all(|v| has(&flag_names(v))));
    let expected = fixture_flags().iter().filter(|f| has(f)).count();
    assert_eq!(got.len(), expected);
    assert!(
        expected > 0 && expected < FIXTURE_RECORDS,
        "the filter must bite"
    );
}

#[test]
fn any_and_all_flags_are_mutually_exclusive() {
    let out = fse_dump(&[
        "dump",
        "--json",
        "-",
        "-f",
        "Created",
        "--all-flags",
        "Removed",
        FIXTURE,
    ]);
    assert!(!out.status.success());
    assert!(stdout_lines(&out).is_empty());
}

#[test]
fn path_filter_is_a_regex_over_the_volume_relative_path() {
    let out = fse_dump(&[
        "dump",
        "--json",
        "-",
        "-p",
        r"^Users/[^/]+/Library",
        FIXTURE,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));

    let re = regex::Regex::new(r"^Users/[^/]+/Library").unwrap();
    let got = json_lines(&out);
    assert!(got.iter().all(|v| re.is_match(v["path"].as_str().unwrap())));

    let all = fse_dump(&["dump", "--json", "-", FIXTURE]);
    let expected = json_lines(&all)
        .iter()
        .filter(|v| re.is_match(v["path"].as_str().unwrap()))
        .count();
    assert_eq!(got.len(), expected);
    assert!(
        expected > 0 && expected < FIXTURE_RECORDS,
        "the filter must bite"
    );
}

#[test]
fn invalid_path_filter_fails() {
    let out = fse_dump(&["dump", "--json", "-", "-p", "(unclosed", FIXTURE]);
    assert!(!out.status.success());
    assert!(stdout_lines(&out).is_empty());
}

#[test]
fn inputs_are_written_in_command_line_order() {
    let dir = scratch("order");
    let one = v2_log(&dir, "0000000000000010", &["/one"], 0);
    let two = v2_log(&dir, "0000000000000011", &["/two"], 0);

    let out = fse_dump(&["dump", "--json", "-", path_str(&two), path_str(&one)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let paths: Vec<_> = json_lines(&out)
        .iter()
        .map(|v| v["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(paths, ["/two", "/one"]);
}

#[test]
fn record_fields_follow_the_feature_set() {
    let out = fse_dump(&["dump", "--json", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));
    let rec = &json_lines(&out)[0];

    assert_eq!(rec["event_id"].is_string(), cfg!(feature = "hex"), "{rec}");
    assert_eq!(
        rec["event_id"]
            .as_str()
            .is_some_and(|s| s.starts_with("0x")),
        cfg!(feature = "hex"),
        "{rec}"
    );
    assert_eq!(
        rec.get("extra_id").is_some(),
        cfg!(feature = "extra_id"),
        "{rec}"
    );
    assert_eq!(
        rec.get("alt_flags").is_some(),
        cfg!(feature = "alt_flags"),
        "{rec}"
    );
    assert!(rec.get("node_id").is_some(), "{rec}");
    assert!(rec["file_timestamp"].is_string(), "{rec}");
    assert!(
        rec.get("flag").is_none(),
        "the raw flag word is not serialized: {rec}"
    );
}

#[test]
fn unique_counts_add_up_to_the_record_total() {
    let out = fse_dump(&["dump", "--uniques", "-", FIXTURE]);
    assert!(out.status.success(), "{}", stderr(&out));

    let mut rdr = csv::Reader::from_reader(out.stdout.as_slice());
    let counts: Vec<u64> = rdr
        .records()
        .map(|r| r.unwrap()[1].parse().unwrap())
        .collect();
    assert_eq!(counts.iter().sum::<u64>(), FIXTURE_RECORDS as u64);
    assert!(
        counts.len() < FIXTURE_RECORDS,
        "paths repeat in the fixture"
    );
}

#[test]
fn combined_and_per_file_outputs_work_together() {
    let dir = scratch("combined-and-per-file");
    let input = dir.join("000000000342c4f2");
    fs::copy(FIXTURE, &input).unwrap();

    let out = fse_dump(&["dump", "--json", "-", "--csvs", path_str(&input)]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout_lines(&out).len(), FIXTURE_RECORDS);
    let csv = fs::read_to_string(dir.join("000000000342c4f2.csv")).unwrap();
    assert_eq!(csv.lines().count(), FIXTURE_RECORDS + 1);
}

#[test]
fn version_flag_prints_the_crate_version() {
    let out = fse_dump(&["--version"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
}

#[test]
fn help_lists_the_subcommands_this_build_has() {
    let out = fse_dump(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("dump"), "{help}");
    assert!(help.contains("generate"), "{help}");
    assert_eq!(help.contains("watch"), cfg!(feature = "watch"), "{help}");
}

#[test]
fn completions_are_generated_for_every_shell() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let out = fse_dump(&["generate", shell]);
        assert!(out.status.success(), "{shell}: {}", stderr(&out));
        let script = String::from_utf8_lossy(&out.stdout);
        assert!(script.contains("fse_dump"), "{shell}: {script}");
        assert!(script.contains("dump"), "{shell}: {script}");
    }
}
