#[macro_use] extern crate compile_fail;

run_compile_fail_tests!();

// These 2 lines are already enough for the common configuration.
// It will generate a `compile_fail` test function that will run all compile-fail tests from the
// `tests/compile-fail` directory.
