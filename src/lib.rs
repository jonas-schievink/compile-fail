#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate env_logger;
extern crate cargo;
extern crate tempdir;

mod compile;
mod config;
mod json;
mod parse;
mod runner;

pub use config::Config;
use compile::Blueprint;
use parse::TestExpectation;

use tempdir::TempDir;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

#[macro_export]
macro_rules! run_tests {
    () => {
        run_tests!($crate::Config {
            wrapper_test: file!(),
            ..$crate::Config::default()
        });
    };
    ( $e:expr ) => {
        #[test]
        fn compile_fail() {
            $crate::run_tests($e);
        }
    };
}

/// Locates compile-fail tests in the configured directory (`tests/compile-fail/*` by default).
fn find_tests(config: &Config) -> Result<Vec<PathBuf>, Box<Error>> {
    info!("searching for compile-fail tests, config = {:?}", config);

    let mut tests = Vec::new();

    for entry in fs::read_dir(&config.cfail_path)
        .map_err(|e| format!("couldn't open {}: {}", config.cfail_path.display(), e))? {

        let entry = entry?;

        let ftype = entry.file_type()?;
        if !ftype.is_file() {
            return Err(format!(
                "unsupported file type of compile-fail test '{}': {:?}",
                entry.path().display(), ftype
            ).into());
        }

        let mut s = String::new();
        fs::File::open(entry.path())?.read_to_string(&mut s)?;

        info!("found compile-fail test at {}", entry.path().display());
        tests.push(entry.path().to_owned());
    }

    // As a safeguard, raise an error when no test was found. This often indicates that a wrong
    // directory was specified.
    if tests.is_empty() {
        return Err(format!("no compile-fail test found in {}", config.cfail_path.display()).into());
    }

    Ok(tests)
}

/// Runs all compile-fail tests and returns the test result as a `Result` instead of panicking on
/// errors.
///
/// Apart from that, works the same way `run_tests` does.
pub fn try_run_tests(config: Config) -> Result<(), Box<Error>> {
    let _ = env_logger::init();

    let tests = find_tests(&config)?.into_iter()
        .map(|path| TestExpectation::parse(&path).map(|exp| (path, exp)))
        .collect::<Result<Vec<_>, _>>()?;

    let mut blueprint = Blueprint::obtain(&config)?;

    let tempdir = TempDir::new("rust-compile-fail")?;
    info!("temporary output directory at {}", tempdir.path().display());
    blueprint.set_out_dir(tempdir.path().to_owned());
    runner::run(&blueprint, &tests)?;

    Ok(())
}

/// Runs all compile-fail tests.
///
/// This function **must** be called from a test function named `compile_fail` contained in an
/// integration test. The `run_tests!` macro will autogenerate such a function.
///
/// If any compile-fail test fails (or a different error was encountered), this will panic.
pub fn run_tests(config: Config) {
    // Attempt to build the (currently running) compile_fail test
    match try_run_tests(config) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", e);
            panic!();
        }
    }
}
