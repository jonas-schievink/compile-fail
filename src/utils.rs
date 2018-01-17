use std::io::{self, Write, BufRead};

/// An `io::Write` implementor that logs everything using `debug!`.
pub struct LogWriter {
    prefix: &'static str,
    buffer: Vec<u8>,
}

impl LogWriter {
    pub fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            buffer: Vec::with_capacity(4096),
        }
    }

    fn flush_line(&mut self) -> io::Result<()> {
        let mut s = String::new();
        let bytes = (&mut &*self.buffer).read_line(&mut s)?;
        self.buffer.drain(0..bytes);
        debug!("{}: {}", self.prefix, s.trim());
        Ok(())
    }
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.write_all(buf)?;
        if buf.contains(&b'\n') {
            self.flush_line()?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
