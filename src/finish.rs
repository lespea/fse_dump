//! Output sinks that can report the errors `Drop` would swallow
//!
//! `BufWriter`, `GzEncoder` and the zstd encoder all flush (and write their trailers) when they
//! are dropped, but a failure there is silently discarded. Every writer ends its stream through
//! [`Finish::finish`] instead, so a short write, a full disk or a closed pipe is reported and
//! changes the exit status.

use std::{
    fs::File,
    io::{self, BufWriter, StdoutLock, Write},
};

pub trait Finish: Write {
    /// Flushes everything, writes any trailer and reports the first error
    fn finish(self) -> io::Result<()>
    where
        Self: Sized;

    /// [`Finish::finish`] for a type-erased sink
    fn finish_boxed(self: Box<Self>) -> io::Result<()>;
}

impl Finish for StdoutLock<'_> {
    fn finish(mut self) -> io::Result<()> {
        self.flush()
    }

    fn finish_boxed(self: Box<Self>) -> io::Result<()> {
        Finish::finish(*self)
    }
}

impl Finish for File {
    fn finish(mut self) -> io::Result<()> {
        self.flush()
    }

    fn finish_boxed(self: Box<Self>) -> io::Result<()> {
        Finish::finish(*self)
    }
}

impl<W: Finish> Finish for BufWriter<W> {
    fn finish(self) -> io::Result<()> {
        self.into_inner()
            .map_err(io::IntoInnerError::into_error)?
            .finish()
    }

    fn finish_boxed(self: Box<Self>) -> io::Result<()> {
        Finish::finish(*self)
    }
}

impl<W: Finish> Finish for flate2::write::GzEncoder<W> {
    fn finish(self) -> io::Result<()> {
        flate2::write::GzEncoder::finish(self)?.finish()
    }

    fn finish_boxed(self: Box<Self>) -> io::Result<()> {
        Finish::finish(*self)
    }
}

#[cfg(feature = "zstd")]
impl<W: Finish> Finish for zstd::stream::write::Encoder<'_, W> {
    fn finish(self) -> io::Result<()> {
        zstd::stream::write::Encoder::finish(self)?.finish()
    }

    fn finish_boxed(self: Box<Self>) -> io::Result<()> {
        Finish::finish(*self)
    }
}

impl Finish for Box<dyn Finish> {
    fn finish(self) -> io::Result<()> {
        <dyn Finish as Finish>::finish_boxed(self)
    }

    fn finish_boxed(self: Box<Self>) -> io::Result<()> {
        Finish::finish(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sink that accepts writes but fails when flushed, like a pipe whose reader has gone
    struct FailsOnFlush;

    impl Write for FailsOnFlush {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "reader went away",
            ))
        }
    }

    impl Finish for FailsOnFlush {
        fn finish(mut self) -> io::Result<()> {
            self.flush()
        }

        fn finish_boxed(self: Box<Self>) -> io::Result<()> {
            Finish::finish(*self)
        }
    }

    #[test]
    fn buffered_finish_reports_the_inner_error() {
        let mut w = BufWriter::with_capacity(1024, FailsOnFlush);
        w.write_all(b"small enough to sit in the buffer").unwrap();
        let err = w.finish().expect_err("the flush failure must surface");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn boxed_finish_reports_the_inner_error() {
        let w: Box<dyn Finish> = Box::new(BufWriter::new(FailsOnFlush));
        let err = w.finish().expect_err("the flush failure must surface");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn gzip_finish_reports_the_inner_error() {
        let w = flate2::write::GzEncoder::new(FailsOnFlush, flate2::Compression::fast());
        let err = Finish::finish(w).expect_err("the flush failure must surface");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
