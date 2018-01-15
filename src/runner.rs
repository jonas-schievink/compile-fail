//! Runs the compiler and compares its output with the patterns in the compile-fail test.

use Config;
use compile::Blueprint;
use parse::{Pattern, MessageKind, TestExpectation};
use json::{Message, parse_output};
use status::TestStatus;

use std::error::Error;
use std::path::{Path, PathBuf};

/// Compares messages parsed from a compile-fail test (`expected`) with messages output by rustc
/// (`got`).
///
/// Errors and warnings that were not in `expected` always cause a fatal error, while notes and
/// suggestions can be left out for brevity. Everything in `expected` must match an equivalent
/// message (same kind and line) in `got`. Additionally, the message itself must be matched by the
/// regex in `expected`.
fn compare_messages(expected: &[Pattern], got: &[Message]) -> Result<(), Box<Error>> {
    // match everything in `expected` against `got` (ensures that we got everything we expected)
    if let Some(not_found) = expected.iter()
        .find(|pattern| !got.iter().any(|msg| pattern.matches(msg))) {

        return Err(format!("message not found in compiler output: {:?}", not_found).into());
    }

    // match all errors and warnings we `got` against `expected`
    // (ensures that all errors and warnings are expected)
    if let Some(not_found) = got.iter()
        .filter(|got| got.kind == Some(MessageKind::Error) || got.kind == Some(MessageKind::Warning))
        .find(|got| !expected.iter().any(|pattern| pattern.matches( got))) {

        return Err(format!("unexpected error or warning in compiler output (all errors and warnings must be matched by a pattern in the test): {:?}", not_found).into());
    }

    Ok(())
}

/// Runs the compiler on compile-fail tests and compares the resulting output with the corresponding
/// `TestExpectation`.
pub fn run(config: &Config, blueprint: &Blueprint, tests: &[(PathBuf, TestExpectation)]) -> Result<(), Box<Error>> {
    let mut status = TestStatus::new(config, tests.len());
    status.print_header()?;

    for &(ref path, ref expect) in tests.iter() {
        status.print_test(&path.file_name().unwrap().to_string_lossy(), run_test(blueprint, (path, expect)))?;
    }

    status.print_result()?;
    status.into_global_result()
}

/// Runs a test, does not print to the console (but might log).
fn run_test(blueprint: &Blueprint, (path, expect): (&Path, &TestExpectation)) -> Result<(), Box<Error>> {
    let mut cmd = blueprint.build_command(path);
    cmd.args(&["--error-format", "json"]);
    debug!("running {:?}", cmd);

    let output = cmd.output()?;
    if output.status.success() {
        return Err(format!("compilation of compile-fail test {} succeeded", path.display()).into());
    }

    debug!("{} stdout bytes, {} stderr bytes", output.stdout.len(), output.stderr.len());

    let filename = path.display().to_string();
    let output = String::from_utf8(output.stderr).expect("rustc output wasn't utf-8");

    let msgs = parse_output(&filename, &output)?;
    info!("rustc msgs: {:#?}", msgs);

    compare_messages(&expect.expected_msgs, &msgs).map_err(|e| {
        // attach compiler output
        format!("{}\n\nrustc output:\n{:#?}", e, msgs)

        // Who even needs error-chain, quick-error, failure or any of that stuff?
    })?;

    Ok(())
}
