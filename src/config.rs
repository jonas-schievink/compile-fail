use std::path::PathBuf;

/// Configuration for `compile-fail`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the directory containing the compile-fail tests.
    ///
    /// By default, `tests/compile-fail` is searched.
    pub cfail_path: PathBuf,

    /// Path to the integration test invoking the `compile-fail` runner.
    ///
    /// You can use `file!()` as the value for this.
    ///
    /// By convention, this is `tests/compile-fail.rs`.
    pub wrapper_test: &'static str,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            cfail_path: PathBuf::from("tests/compile-fail"),
            // This default will be overwritten by the `run_tests!` macro, which passes `file!()`.
            wrapper_test: "tests/compile-fail.rs",
        }
    }
}
