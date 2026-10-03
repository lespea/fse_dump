//! Process-wide failure accounting
//!
//! Errors are logged where they happen and counted here, so a run can carry on with whatever
//! still works and the exit status still reflects everything that went wrong.

use std::sync::atomic::{AtomicUsize, Ordering};

use color_eyre::{Result, eyre::eyre};

/// Number of errors logged while running; a non-zero count makes the process exit non-zero
static FAILURES: AtomicUsize = AtomicUsize::new(0);

/// Logs an error and records it so the final exit status reflects it
macro_rules! fail {
    ($($arg:tt)*) => {{
        log::error!($($arg)*);
        $crate::fail::record();
    }};
}

/// Counts one failure that has already been logged
#[inline]
pub fn record() {
    FAILURES.fetch_add(1, Ordering::Relaxed);
}

/// Turns the recorded failures into the process's final result
pub fn exit_result(what: &str) -> Result<()> {
    match FAILURES.load(Ordering::Relaxed) {
        0 => Ok(()),
        n => Err(eyre!(
            "{n} error(s) occurred while {what}; see the log above"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The counter is process-wide and other tests in this binary may add to it, so only what
    // this test can see for certain is asserted
    #[test]
    fn recorded_failures_make_the_exit_result_an_error() {
        record();
        let err = exit_result("testing").expect_err("a recorded failure fails the run");
        let msg = err.to_string();
        assert!(msg.contains("error(s) occurred while testing"), "{msg}");
        assert!(FAILURES.load(Ordering::Relaxed) >= 1);
    }
}
