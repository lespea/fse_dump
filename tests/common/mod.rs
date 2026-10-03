//! Helpers shared by the end-to-end test binaries
#![allow(dead_code)]

use std::{
    fs,
    io::Write,
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, SystemTime},
};

/// A real v3 log; see [`FIXTURE_RECORDS`]
pub const FIXTURE: &str = "testfiles/v3/000000000342c4f2";
/// How many records [`FIXTURE`] holds
pub const FIXTURE_RECORDS: usize = 2730;

/// The binary under test, ready for arguments
pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fse_dump"))
}

/// Runs the binary to completion
pub fn fse_dump(args: &[&str]) -> Output {
    bin().args(args).output().expect("failed to run fse_dump")
}

pub fn stdout_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Every stdout line parsed as a json object
pub fn json_lines(out: &Output) -> Vec<serde_json::Value> {
    stdout_lines(out)
        .iter()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}")))
        .collect()
}

/// The fields that identify a record regardless of the file it came from or the feature set
pub fn record_key(v: &serde_json::Value) -> (String, String, String) {
    (
        v["path"].as_str().unwrap().to_owned(),
        v["event_id"].to_string(),
        v["flags"].as_str().unwrap().to_owned(),
    )
}

/// A fresh scratch directory per test, removed again when the test is done
pub struct Scratch(PathBuf);

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

pub fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "fse_dump-{}-{}-{name}",
        env!("CARGO_CRATE_NAME"),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

pub fn set_a_year_old(path: &Path) {
    let a_year_ago = SystemTime::now() - Duration::from_secs(365 * 24 * 60 * 60);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(a_year_ago)
        .unwrap();
}

/// A gzipped v2 log called `name` holding one Modified record per path, cut `chop` bytes short
/// of the page's declared length (0 leaves it intact)
pub fn v2_log(dir: &Path, name: &str, paths: &[&str], chop: usize) -> PathBuf {
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
    let path = dir.join(name);
    fs::write(&path, gz.finish().unwrap()).unwrap();
    path
}
