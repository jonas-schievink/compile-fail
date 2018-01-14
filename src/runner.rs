//! Runs the compiler and compares its output with the patterns in the compile-fail test.

use compile::Blueprint;
use parse::{Pattern, MessageKind, TestExpectation};
use json::{Message, parse_output};

use std::error::Error;
use std::path::PathBuf;

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
        .find(|pattern| !got.iter().any(|msg| matches(pattern, msg))) {

        return Err(format!("message not found in compiler output: {:?}", not_found).into());
    }

    // match all errors and warnings we `got` against `expected`
    // (ensures that all errors and warnings are expected)
    if let Some(not_found) = got.iter()
        .filter(|got| got.kind == Some(MessageKind::Error) || got.kind == Some(MessageKind::Warning))
        .find(|got| !expected.iter().any(|pattern| matches(pattern, got))) {

        return Err(format!("unexpected error or warning in compiler output (all errors and warnings must be matched by a pattern in the test): {:?}", not_found).into());
    }

    Ok(())
}

fn matches(pattern: &Pattern, msg: &Message) -> bool {
    if pattern.kind != msg.kind {
        // kind must match *exactly*
        return false;
    }

    if pattern.line_num != msg.line_num {
        // line must match *exactly*
        return false;
    }

    if !msg.msg.contains(&pattern.msg) {
        return false;
    }

    info!("matches: pattern {:?} matches message {:?}", pattern, msg);
    true
}

/// Runs the compiler on compile-fail tests and compares the resulting output with the corresponding
/// `TestExpectation`.
pub fn run<I>(blueprint: &Blueprint, tests: I) -> Result<(), Box<Error>>
where I: IntoIterator<Item=(PathBuf, TestExpectation)> {
    for (path, expect) in tests {
        let mut cmd = blueprint.build_command(&path);
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

        compare_messages(&expect.expected_msgs, &msgs)?;
    }

    Ok(())
}
