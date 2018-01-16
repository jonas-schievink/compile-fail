// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Adapted from `compiletest-rs`.

use parse::MessageKind;
use serde_json as json;
use std::str::FromStr;
use std::path::Path;
use std::error::Error;

#[derive(Debug)]
pub struct Message {
    /// The kind of message. The compiler doesn't always attach one, so this may be `None`.
    ///
    /// (TODO: Verify and maybe get rid of the `Option`)
    pub kind: Option<MessageKind>,
    /// The primary message as rendered by rustc.
    pub msg: String,
    /// The code of the error this message is a part of.
    pub code: Option<String>,
    /// The line at which the message points.
    pub line_num: usize,
}

// These structs are a subset of the ones found in
// `syntax::json`.

#[derive(Serialize, Deserialize)]
struct Diagnostic {
    /// The primary error message.
    message: String,
    code: Option<DiagnosticCode>,
    /// "error: internal compiler error", "error", "warning", "note", "help".
    level: String,
    spans: Vec<DiagnosticSpan>,
    /// Associated diagnostic messages.
    children: Vec<Diagnostic>,
    /// The message as rustc would render it.
    rendered: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct DiagnosticSpan {
    file_name: String,
    /// 1-based.
    line_start: usize,
    line_end: usize,
    /// 1-based, character offset.
    column_start: usize,
    column_end: usize,
    /// Is this a "primary" span -- meaning the point, or one of the points,
    /// where the error occurred?
    is_primary: bool,
    /// Label that should be placed at this location (if any).
    label: Option<String>,
    /// If we are suggesting a replacement, this will contain text
    /// that should be sliced in atop this span.
    suggested_replacement: Option<String>,
    /// Macro invocations that created the code at this span, if any.
    expansion: Option<Box<DiagnosticSpanMacroExpansion>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct DiagnosticSpanMacroExpansion {
    /// span where macro was applied to generate this code; note that
    /// this may itself derive from a macro (if
    /// `span.expansion.is_some()`)
    span: DiagnosticSpan,

    /// name of macro that was applied (e.g., `foo!` or `#[derive(Eq)]`)
    macro_decl_name: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct DiagnosticCode {
    /// The code itself.
    code: String,
    /// An explanation for the code.
    explanation: Option<String>,
}

pub fn parse_output(file_name: &str, output: &str) -> Result<Vec<Message>, Box<Error>> {
    // this probably wants `try_fold`
    output.lines()
        .map(|line| parse_line(file_name, line))
        .fold(Ok(vec![]), |state, result| {
            state.and_then(|mut msgs| result.map(|mut new_msgs| {
                msgs.append(&mut new_msgs);
                msgs
            }))
        })
}

fn parse_line(file_name: &str, line: &str) -> Result<Vec<Message>, Box<Error>> {
    // The compiler sometimes intermingles non-JSON stuff into the
    // output.  This hack just skips over such lines. Yuck.
    if line.starts_with('{') {
        let diagnostic = json::from_str::<Diagnostic>(line)?;
        let mut expected_errors = vec![];
        push_expected_errors(&mut expected_errors, &diagnostic, &[], file_name);
        Ok(expected_errors)
    } else {
        Ok(vec![])
    }
}

fn push_expected_errors(expected_errors: &mut Vec<Message>,
                        diagnostic: &Diagnostic,
                        default_spans: &[&DiagnosticSpan],
                        file_name: &str) {
    let spans_in_this_file: Vec<_> = diagnostic.spans
        .iter()
        .filter(|span| Path::new(&span.file_name) == Path::new(&file_name))
        .collect();

    let primary_spans: Vec<_> = spans_in_this_file.iter()
        .cloned()
        .filter(|span| span.is_primary)
        .take(1) // sometimes we have more than one showing up in the json; pick first
        .collect();
    let primary_spans = if primary_spans.is_empty() {
        // subdiagnostics often don't have a span of their own;
        // inherit the span from the parent in that case
        default_spans
    } else {
        &primary_spans
    };

    let code = diagnostic.code.clone().map(|code| code.code);

    // Convert multi-line messages into multiple expected
    // errors. We expect to replace these with something
    // more structured shortly anyhow.
    let mut message_lines = diagnostic.message.lines();
    if let Some(first_line) = message_lines.next() {
        for span in primary_spans {
            // We should perhaps `.unwrap()` the parse
            let kind = MessageKind::from_str(&diagnostic.level).ok();
            expected_errors.push(Message {
                line_num: span.line_start,
                code: code.clone(),
                kind,
                msg: first_line.to_string(),
            });
        }
    }
    for next_line in message_lines {
        for span in primary_spans {
            expected_errors.push(Message {
                line_num: span.line_start,
                code: code.clone(),
                kind: None,
                msg: next_line.to_string(),
            });
        }
    }

    // If the message has a suggestion, register that.
    for span in primary_spans {
        if let Some(ref suggested_replacement) = span.suggested_replacement {
            for (index, line) in suggested_replacement.lines().enumerate() {
                expected_errors.push(Message {
                    line_num: span.line_start + index,
                    kind: Some(MessageKind::Suggestion),
                    msg: line.to_string(),
                    code: code.clone(),
                });
            }
        }
    }

    // Add notes for the backtrace
    for span in primary_spans {
        for frame in &span.expansion {
            push_backtrace(expected_errors, frame, file_name);
        }
    }

    // Add notes for any labels that appear in the message.
    for span in spans_in_this_file.iter()
        .filter(|span| span.label.is_some()) {
        expected_errors.push(Message {
            line_num: span.line_start,
            kind: Some(MessageKind::Note),
            msg: span.label.clone().unwrap(),
            code: code.clone(),
        });
    }

    // Flatten out the children.
    for child in &diagnostic.children {
        push_expected_errors(expected_errors, child, primary_spans, file_name);
    }
}

fn push_backtrace(expected_errors: &mut Vec<Message>,
                  expansion: &DiagnosticSpanMacroExpansion,
                  file_name: &str) {
    if Path::new(&expansion.span.file_name) == Path::new(&file_name) {
        expected_errors.push(Message {
            line_num: expansion.span.line_start,
            kind: Some(MessageKind::Note),
            msg: format!("in this expansion of {}", expansion.macro_decl_name),
            code: None,
        });
    }

    for previous_expansion in &expansion.span.expansion {
        push_backtrace(expected_errors, previous_expansion, file_name);
    }
}
