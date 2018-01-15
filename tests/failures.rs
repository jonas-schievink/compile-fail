extern crate compile_fail;

use compile_fail::*;

use std::fs::read_dir;
use std::path::PathBuf;

/// This tests that compile-fail tests correctly fail when we expect them to.
#[test]
fn failures() {
    let path = PathBuf::from("tests/failures");

    let c = Config {
        cfail_path: path.clone(),
        wrapper_test: file!(),
        no_console_output: true,
    };

    for entry in read_dir(&path).unwrap() {
        let entry = entry.unwrap();

        match run_single_test(c.clone(), entry.path().to_owned()) {
            Ok(()) => panic!("test {} succeeded, but was expected to fail", entry.path().display()),
            Err(e) => {
                // It would be nice to compare the error to the one we expect.
                println!("{}", e);
            }
        }
    }
}

#[test]
#[should_panic(expected = "couldn't open this-dir/does-not-exist")]
fn no_such_dir() {
    let c = Config {
        cfail_path: "this-dir/does-not-exist".into(),
        wrapper_test: file!(),
        no_console_output: true,
    };

    run_tests(c);
}

#[test]
#[should_panic(expected = "no compile-fail test found in tests/empty")]
fn empty_dir() {
    let c = Config {
        cfail_path: "tests/empty".into(),
        wrapper_test: file!(),
        no_console_output: true,
    };

    run_tests(c);
}
