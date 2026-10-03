//! End-to-end checks of watch mode against the platform's real directory watcher
//!
//! Each test starts `fse_dump watch` on a scratch directory, drops files into it and reads what
//! comes out, so the kqueue (macOS), inotify (Linux) and ReadDirectoryChangesW (Windows) paths
//! are all exercised wherever the suite runs. Shutdown is the one place the platforms differ:
//! on unix the process is sent a real SIGINT or SIGTERM and its output must be finished cleanly,
//! on Windows there is no signal to send from a test so it is killed after the records arrived.
#![cfg(feature = "watch")]

use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Child, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

mod common;
use common::*;

/// How long the watcher may take to come up
const SETUP: Duration = Duration::from_secs(30);
/// How long a new file may take to be noticed, debounced (2 s), parsed and written
const NOTICE: Duration = Duration::from_secs(60);
/// How long a clean shutdown may take once asked for
const SHUTDOWN: Duration = Duration::from_secs(30);

enum Chunk {
    Out(Vec<u8>),
    Err(String),
}

/// A running `fse_dump watch` whose output is collected as it arrives
struct WatchRun {
    child: Child,
    chunks: Receiver<Chunk>,
    stdout: Vec<u8>,
    stderr: String,
}

impl WatchRun {
    /// Starts watching `dir` with `args` and waits until the watcher reports it is in place
    fn start(dir: &Path, args: &[&str]) -> Self {
        Self::start_with_env(dir, args, &[])
    }

    /// [`WatchRun::start`] with extra environment variables for the process
    fn start_with_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        let mut child = bin()
            .arg("watch")
            .args(args)
            .arg(dir)
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start fse_dump watch");

        let (send, chunks) = mpsc::channel();
        let mut out = child.stdout.take().unwrap();
        let out_send = send.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = out.read(&mut buf)
                && n > 0
                && out_send.send(Chunk::Out(buf[..n].to_vec())).is_ok()
            {}
        });
        let err = BufReader::new(child.stderr.take().unwrap());
        thread::spawn(move || {
            for line in err.lines().map_while(Result::ok) {
                if send.send(Chunk::Err(line)).is_err() {
                    break;
                }
            }
        });

        let mut run = Self {
            child,
            chunks,
            stdout: Vec::new(),
            stderr: String::new(),
        };
        assert!(
            run.wait_until(SETUP, |r| r.stderr.contains("Watching ")),
            "the watcher never came up:\n{}",
            run.stderr
        );
        run
    }

    /// Collects output until `done` holds or `timeout` passes; true if it held
    fn wait_until(&mut self, timeout: Duration, done: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if done(self) {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(Chunk::Out(bytes)) => self.stdout.extend(bytes),
                Ok(Chunk::Err(line)) => {
                    self.stderr.push_str(&line);
                    self.stderr.push('\n');
                }
                Err(RecvTimeoutError::Timeout) => return done(self),
                Err(RecvTimeoutError::Disconnected) => return done(self),
            }
        }
    }

    fn stdout_lines(&self) -> usize {
        self.stdout.iter().filter(|b| **b == b'\n').count()
    }

    /// Waits for `n` complete lines on stdout
    fn expect_lines(&mut self, n: usize) {
        assert!(
            self.wait_until(NOTICE, |r| r.stdout_lines() >= n),
            "wanted {n} lines, got {}:\n{}",
            self.stdout_lines(),
            self.stderr
        );
    }

    /// Waits for the parser to report `path` done
    fn expect_parsed(&mut self, path: &Path) {
        let msg = format!("Finished parsing {}", path.display());
        assert!(
            self.wait_until(NOTICE, |r| r.stderr.contains(&msg)),
            "never saw {msg:?}:\n{}",
            self.stderr
        );
    }

    /// Asks the process to stop and collects everything it wrote
    ///
    /// On unix that is a real signal (`INT` or `TERM`) and the stream is expected to be finished
    /// cleanly; on Windows the process is killed, so only what was flushed before is returned.
    fn stop(mut self, signal: &str) -> Finished {
        #[cfg(unix)]
        {
            let status = std::process::Command::new("kill")
                .arg(format!("-{signal}"))
                .arg(self.child.id().to_string())
                .status()
                .expect("kill runs");
            assert!(status.success(), "kill -{signal} failed");
        }
        #[cfg(not(unix))]
        {
            let _ = signal;
            self.child.kill().expect("kill works");
        }

        let deadline = Instant::now() + SHUTDOWN;
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("the watch did not stop after {signal}:\n{}", self.stderr);
            }
            thread::sleep(Duration::from_millis(20));
        };
        // Both pipes close when the process is gone, so this drains to disconnection
        self.wait_until(SHUTDOWN, |_| false);

        Finished {
            status,
            stdout: std::mem::take(&mut self.stdout),
            stderr: std::mem::take(&mut self.stderr),
        }
    }
}

impl Drop for WatchRun {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: String,
}

impl Finished {
    fn text(&self) -> String {
        String::from_utf8(self.stdout.clone()).expect("stdout is text")
    }

    /// Every stdout line as json
    fn json(&self) -> Vec<serde_json::Value> {
        self.text()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
            .collect()
    }

    /// On unix a clean shutdown exits 0; on Windows the process was killed
    fn assert_clean(&self) {
        if cfg!(unix) {
            assert!(self.status.success(), "{:?}:\n{}", self.status, self.stderr);
            assert!(
                self.stderr.contains("Interrupted; finishing the output"),
                "{}",
                self.stderr
            );
        }
    }
}

/// The identifying fields of every fixture record, as `dump` writes them
fn fixture_keys(filter: &[&str]) -> Vec<(String, String, String)> {
    let mut args = vec!["dump", "--json", "-"];
    args.extend_from_slice(filter);
    args.push(FIXTURE);
    let out = fse_dump(&args);
    assert!(out.status.success(), "{}", stderr(&out));
    json_lines(&out).iter().map(record_key).collect()
}

/// Drops a copy of the fixture into `dir` under a log-like name
fn add_fixture(dir: &Path, name: &str) -> std::path::PathBuf {
    let log = dir.join(name);
    fs::copy(FIXTURE, &log).unwrap();
    log
}

#[test]
fn a_new_log_is_parsed_and_written_as_json() {
    let dir = scratch("json");
    let mut run = WatchRun::start(&dir, &["-o", "json"]);

    add_fixture(&dir, "000000000342c4f2");
    run.expect_lines(FIXTURE_RECORDS);

    let done = run.stop("INT");
    done.assert_clean();
    let keys: Vec<_> = done.json().iter().map(record_key).collect();
    assert_eq!(
        keys,
        fixture_keys(&[]),
        "watch writes the same records dump does"
    );
}

#[test]
fn csv_output_has_a_header_and_every_record() {
    let dir = scratch("csv");
    let mut run = WatchRun::start(&dir, &["-o", "csv"]);

    add_fixture(&dir, "000000000342c4f2");
    run.expect_lines(FIXTURE_RECORDS + 1);

    let done = run.stop("INT");
    done.assert_clean();
    let mut rdr = csv::Reader::from_reader(done.stdout.as_slice());
    let header: Vec<_> = rdr.headers().unwrap().iter().map(str::to_owned).collect();
    assert_eq!(&header[..3], ["path", "event_id", "flags"], "{header:?}");
    assert_eq!(rdr.records().count(), FIXTURE_RECORDS);
}

#[test]
fn yaml_output_is_a_document_stream() {
    let dir = scratch("yaml");
    let mut run = WatchRun::start(&dir, &["-o", "yaml"]);

    add_fixture(&dir, "000000000342c4f2");
    // Each document is a separator line plus one line per field
    run.wait_until(NOTICE, |r| {
        r.stdout
            .split(|b| *b == b'\n')
            .filter(|l| *l == b"---")
            .count()
            >= FIXTURE_RECORDS
    });

    let done = run.stop("INT");
    done.assert_clean();
    let text = done.text();
    assert_eq!(
        text.lines().filter(|l| *l == "---").count(),
        FIXTURE_RECORDS,
        "{}",
        done.stderr
    );
    assert_eq!(
        text.lines().filter(|l| l.starts_with("path: ")).count(),
        FIXTURE_RECORDS
    );
}

#[test]
fn pretty_json_is_a_multiline_stream() {
    let dir = scratch("pretty");
    let mut run = WatchRun::start(&dir, &["--pretty"]);

    let log = add_fixture(&dir, "000000000342c4f2");
    run.expect_parsed(&log);
    // Every record ends with a closing brace on its own line
    run.wait_until(NOTICE, |r| {
        r.stdout
            .split(|b| *b == b'\n')
            .filter(|l| *l == b"}")
            .count()
            >= FIXTURE_RECORDS
    });

    let done = run.stop("INT");
    done.assert_clean();
    let text = done.text();
    assert!(
        text.lines().count() > FIXTURE_RECORDS,
        "one record spans several lines"
    );
    let values = serde_json::Deserializer::from_str(&text)
        .into_iter::<serde_json::Value>()
        .map(|v| v.expect("each record parses"))
        .collect::<Vec<_>>();
    assert_eq!(values.len(), FIXTURE_RECORDS, "{}", done.stderr);
}

#[test]
fn only_hex_named_files_are_parsed() {
    let dir = scratch("names");
    let mut run = WatchRun::start_with_env(&dir, &[], &[("RUST_LOG", "debug")]);

    // The file fseventsd keeps next to its logs. It is created on its own and acknowledged
    // before the log goes in: notify's kqueue backend reports only one new file per directory
    // write it sees, so files created back to back can go unannounced on macOS.
    let uuid = add_fixture(&dir, "fseventsd-uuid");
    if cfg!(debug_assertions) {
        // A debug build logs the decision to ignore a file, which proves the decoy's create
        // event was processed rather than never delivered
        let ignored = format!("Ignoring non-log file {}", uuid.display());
        assert!(
            run.wait_until(NOTICE, |r| r.stderr.contains(&ignored)),
            "never saw {ignored:?}:\n{}",
            run.stderr
        );
    } else {
        // Release builds compile debug logging out (see the log features in Cargo.toml), so
        // there is nothing to wait for; the watcher only needs a moment to see the decoy
        thread::sleep(Duration::from_secs(3));
    }

    let log = add_fixture(&dir, "000000000342c4f3");
    run.expect_parsed(&log);
    run.expect_lines(FIXTURE_RECORDS);

    let done = run.stop("INT");
    done.assert_clean();
    assert!(
        !done.stderr.contains(&format!("Parsing {}", uuid.display())),
        "{}",
        done.stderr
    );
    assert_eq!(
        done.json().len(),
        FIXTURE_RECORDS,
        "only the log's records are written"
    );
}

#[test]
fn filters_apply_to_watched_logs() {
    let dir = scratch("filters");
    let mut run = WatchRun::start(&dir, &["-f", "Created", "-p", "^Users/"]);
    let expected = fixture_keys(&["-f", "Created", "-p", "^Users/"]);
    assert!(!expected.is_empty() && expected.len() < FIXTURE_RECORDS);

    let log = add_fixture(&dir, "000000000342c4f2");
    run.expect_parsed(&log);
    run.expect_lines(expected.len());

    let done = run.stop("INT");
    done.assert_clean();
    let keys: Vec<_> = done.json().iter().map(record_key).collect();
    assert_eq!(keys, expected);
}

#[test]
fn several_logs_are_written_as_they_appear() {
    let dir = scratch("several");
    let mut run = WatchRun::start(&dir, &[]);

    let first = v2_log(&dir, "0000000000000010", &["/first"], 0);
    run.expect_parsed(&first);
    run.expect_lines(1);
    let second = v2_log(&dir, "0000000000000011", &["/second"], 0);
    run.expect_parsed(&second);
    run.expect_lines(2);

    let done = run.stop("INT");
    done.assert_clean();
    let paths: Vec<_> = done
        .json()
        .iter()
        .map(|v| v["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(paths, ["/first", "/second"]);
}

#[test]
fn logs_in_a_new_subdirectory_are_seen() {
    let dir = scratch("subdir");
    let mut run = WatchRun::start(&dir, &[]);

    let sub = dir.join("nested");
    fs::create_dir(&sub).unwrap();
    // The watcher attaches to a new directory when it processes the directory's own create
    // event, which is not instant; a file written before that can be missed (inotify has no
    // way to watch a directory retroactively). fseventsd keeps its logs flat, so this only
    // needs to show that nested logs are picked up once the directory is being watched.
    thread::sleep(Duration::from_secs(3));
    let log = v2_log(&sub, "0000000000000010", &["/nested"], 0);
    run.expect_parsed(&log);
    run.expect_lines(1);

    let done = run.stop("INT");
    done.assert_clean();
    assert_eq!(done.json()[0]["path"], "/nested");
}

#[test]
fn a_bad_log_is_reported_and_watching_continues() {
    let dir = scratch("bad-log");
    let mut run = WatchRun::start(&dir, &[]);

    // Cut part way through the third record: two records come out, then an error
    let cut = v2_log(&dir, "0000000000000010", &["/a", "/b", "/c"], 5);
    run.expect_parsed_or_failed(&cut);
    run.expect_lines(2);
    assert!(run.stderr.contains("truncated"), "{}", run.stderr);

    // The watch is still alive and parses the next file
    let good = v2_log(&dir, "0000000000000011", &["/after"], 0);
    run.expect_parsed(&good);
    run.expect_lines(3);

    let done = run.stop("INT");
    let paths: Vec<_> = done
        .json()
        .iter()
        .map(|v| v["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(paths, ["/a", "/b", "/after"]);
    if cfg!(unix) {
        assert!(!done.status.success(), "the parse error must fail the run");
        assert!(
            done.stderr.contains("error(s) occurred while watching"),
            "{}",
            done.stderr
        );
    }
}

impl WatchRun {
    /// Waits for the parser to report `path` done or failed
    fn expect_parsed_or_failed(&mut self, path: &Path) {
        let shown = path.display().to_string();
        assert!(
            self.wait_until(NOTICE, |r| r
                .stderr
                .contains(&format!("Error parsing {shown}"))
                || r.stderr.contains(&format!("Finished parsing {shown}"))),
            "never saw {shown} parsed:\n{}",
            self.stderr
        );
    }
}

#[test]
fn the_poll_watcher_sees_new_logs() {
    let dir = scratch("poll");
    let mut run = WatchRun::start(&dir, &["--poll"]);

    let log = add_fixture(&dir, "000000000342c4f2");
    run.expect_parsed(&log);
    run.expect_lines(FIXTURE_RECORDS);

    let done = run.stop("INT");
    done.assert_clean();
    assert_eq!(done.json().len(), FIXTURE_RECORDS);
}

#[cfg(unix)]
#[test]
fn a_gzip_stream_is_finished_on_sigint() {
    let dir = scratch("gzip");
    let mut run = WatchRun::start(&dir, &["--gzip"]);

    let log = add_fixture(&dir, "000000000342c4f2");
    run.expect_parsed(&log);

    let done = run.stop("INT");
    done.assert_clean();
    let mut text = String::new();
    flate2::read::GzDecoder::new(done.stdout.as_slice())
        .read_to_string(&mut text)
        .expect("stdout is a complete gzip stream");
    assert_eq!(text.lines().count(), FIXTURE_RECORDS, "{}", done.stderr);
}

#[cfg(unix)]
#[test]
fn a_gzip_stream_is_finished_on_sigterm() {
    let dir = scratch("gzip-term");
    let mut run = WatchRun::start(&dir, &["--gzip", "-o", "csv"]);

    let log = add_fixture(&dir, "000000000342c4f2");
    run.expect_parsed(&log);

    let done = run.stop("TERM");
    done.assert_clean();
    let mut text = String::new();
    flate2::read::GzDecoder::new(done.stdout.as_slice())
        .read_to_string(&mut text)
        .expect("stdout is a complete gzip stream");
    assert_eq!(text.lines().count(), FIXTURE_RECORDS + 1, "{}", done.stderr);
}

#[cfg(all(unix, feature = "zstd"))]
#[test]
fn a_zstd_stream_is_finished_on_sigint() {
    let dir = scratch("zstd");
    let mut run = WatchRun::start(&dir, &["--zstd"]);

    let log = add_fixture(&dir, "000000000342c4f2");
    run.expect_parsed(&log);

    let done = run.stop("INT");
    done.assert_clean();
    let bytes = zstd::decode_all(done.stdout.as_slice()).expect("stdout is a complete zstd frame");
    assert_eq!(
        String::from_utf8(bytes).unwrap().lines().count(),
        FIXTURE_RECORDS,
        "{}",
        done.stderr
    );
}

#[test]
fn an_invalid_compression_level_is_rejected_before_watching() {
    let dir = scratch("glevel");
    let out = fse_dump(&["watch", "--glevel", "15", path_str(&dir)]);
    assert!(!out.status.success());
    assert!(!stderr(&out).contains("Watching"), "{}", stderr(&out));

    #[cfg(feature = "zstd")]
    {
        let out = fse_dump(&["watch", "--zlevel", "99", path_str(&dir)]);
        assert!(!out.status.success());
        assert!(!stderr(&out).contains("Watching"), "{}", stderr(&out));
    }
}

#[test]
fn an_invalid_filter_is_rejected_before_watching() {
    let dir = scratch("bad-filter");
    let out = fse_dump(&["watch", "-f", "Bogus", path_str(&dir)]);
    assert!(!out.status.success());
    assert!(!stderr(&out).contains("Watching"), "{}", stderr(&out));
}

#[test]
fn a_missing_directory_fails() {
    let dir = scratch("missing");
    let gone = dir.join("not-here");
    let out = fse_dump(&["watch", path_str(&gone)]);
    assert!(!out.status.success());
    assert!(!stderr(&out).contains("Watching"), "{}", stderr(&out));
}
