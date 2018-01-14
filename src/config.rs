use std::path::PathBuf;

/// Configuration for `compile-fail`.
pub struct Config {
    /// Path to a directory to search for `compile-fail` tests.
    ///
    /// By default, `tests/compile-fail` is searched.
    pub cfail_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            cfail_path: PathBuf::from("tests/compile-fail"),
        }
    }
}
