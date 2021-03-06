//! Test progress reporting.

use Config;

use termcolor::{ColorChoice, StandardStream, WriteColor, Color, ColorSpec};
use std::io::{self, Write};
use std::fmt::Display;
use std::error::Error;
use std::thread::panicking;
use std::str;

enum Out {
    Console(StandardStream),
    /// Buffers output instead of printing.
    Quiet(Vec<u8>),
}

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match *self {
            Out::Console(ref mut s) => s.write(buf),
            Out::Quiet(ref mut b) => b.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match *self {
            Out::Console(ref mut s) => s.flush(),
            Out::Quiet(ref mut b) => b.flush(),
        }
    }
}

impl WriteColor for Out {
    fn supports_color(&self) -> bool {
        match *self {
            Out::Console(ref s) => s.supports_color(),
            Out::Quiet(_) => false,
        }
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        match *self {
            Out::Console(ref mut s) => s.set_color(spec),
            Out::Quiet(_) => Ok(()),
        }
    }

    fn reset(&mut self) -> io::Result<()> {
        match *self {
            Out::Console(ref mut s) => s.reset(),
            Out::Quiet(_) => Ok(()),
        }
    }
}

pub struct TestStatus<E> {
    out: Out,
    errors: Vec<(String, E)>,
    num_tests: usize,
    num_passed: usize,
    defused: bool,
}

impl<E> TestStatus<E> {
    pub fn new(config: &Config, num_tests: usize) -> Self {
        Self {
            out: if config.no_console_output {
                Out::Quiet(Vec::new())
            } else {
                Out::Console(StandardStream::stdout(ColorChoice::Auto))
            },
            errors: Vec::new(),
            num_tests,
            num_passed: 0,
            defused: false,
        }
    }

    pub fn print_header(&mut self) -> io::Result<()> {
        writeln!(self.out, "running {} compile-fail test{}",
                 self.num_tests,
                 if self.num_tests == 1 { "" } else { "s" })
    }

    /// Prints the short result of a single test (passed / failed).
    pub fn print_test<T>(&mut self, name: &str, result: Result<T, E>) -> io::Result<()> {
        write!(self.out, "test {} ... ", name)?;
        self.colored_status(result.is_ok())?;
        writeln!(self.out)?;

        if let Err(e) = result {
            self.errors.push((name.to_string(), e));
        } else {
            self.num_passed += 1;
        }

        Ok(())
    }

    pub fn print_result(&mut self) -> io::Result<()>
        where E: Display {

        write!(self.out, "test result: ")?;
        let success = self.errors.is_empty();
        self.colored_status(success)?;
        writeln!(self.out, ". {} passed; {} failed", self.num_passed, self.errors.len())?;
        writeln!(self.out)?;

        for &(ref name, ref err) in self.errors.iter() {
            writeln!(self.out, "---- test {} ----", name)?;
            writeln!(self.out, "{}", err)?;
            writeln!(self.out)?;
        }

        Ok(())
    }

    /// Turns this `TestStatus` into a summarizing result that is `Ok` if all tests passed and `Err`
    /// if at least one test failed.
    ///
    /// This method must be called or the `Drop` impl of `TestStatus` will panic.
    pub fn into_global_result(mut self) -> Result<(), Box<Error>> {
        self.defused = true;
        if self.errors.is_empty() {
            Ok(())
        } else {
            let msg = match self.out {
                Out::Console(_) => {
                    // We already printed everything to the console, a summary is enough
                    String::new()
                }
                Out::Quiet(ref b) => {
                    format!("{}\n\n", str::from_utf8(b).expect("produced non-utf8 output"))
                }
            };


            Err(format!("{}{} compile-fail tests failed", msg, self.errors.len()).into())
        }
    }

    fn colored_status(&mut self, pass: bool) -> io::Result<()> {
        let (color, msg) = match pass {
            true => (Color::Green, "ok"),
            false => (Color::Red, "FAILED"),
        };
        let _ = self.out.set_color(&ColorSpec::new().set_fg(Some(color)));
        write!(self.out, "{}", msg)?;
        let _ = self.out.reset();
        Ok(())
    }
}

impl<E> Drop for TestStatus<E> {
    fn drop(&mut self) {
        if !panicking() {
            assert!(self.defused, "TestStatus::into_global_result was not called");
        }
    }
}
