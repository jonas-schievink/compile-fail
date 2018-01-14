//! Parses compile-fail tests to extract expected errors.

// Note: This does not support any kind of directive that compiletest-rs supports

use regex::Regex;
use std::error::Error;
use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;

// FIXME: This should reject any `//~` that is invalid
// FIXME: We should also allow `error[E0308]{: error pattern}` style patterns

/// The different messages rustc can emit.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum MessageKind {
    Error,
    Warning,
    Note,
    Help,
    Suggestion,
}

impl FromStr for MessageKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use self::MessageKind::*;

        Ok(match &*s.to_lowercase() {
            "error" => Error,
            "warning" | "warn" => Warning,
            "note" => Note,
            "help" => Help,
            "suggestion" => Suggestion,
            _ => return Err(()),
        })
    }
}

/// A pattern that can match a compiler message.
#[derive(Debug)]
pub struct Pattern {
    /// The kind of message we expect.
    pub kind: Option<MessageKind>,
    /// The regex this message must match.
    pub msg: String,
    /// The line at which the message must point.
    pub line_num: usize,
}

/// Expected compiler messages/errors parsed from a test.
#[derive(Debug)]
pub struct TestExpectation {
    pub expected_msgs: Vec<Pattern>,
}

impl TestExpectation {
    /// Read the file at `path` and parse all expected errors.
    pub fn parse(path: &Path) -> Result<Self, Box<Error>> {
        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        drop(file);

        let mut parser = Parser::new(&content);
        parser.parse()?;

        if parser.expected_msgs.is_empty() {
            return Err(format!("no error patterns found in {}", path.display()).into());
        }

        Ok(TestExpectation {
            expected_msgs: parser.expected_msgs,
        })
    }
}

struct Parser<'a> {
    expected_msgs: Vec<Pattern>,
    /// Last line number that contained a parsed `Message`. 0 if none were parsed yet.
    last_line_with_msg: usize,
    content: &'a str,

    // we support 2 kinds of patterns:
    //~(^*) {kind}:{msg}    regex1
    //~| {kind}:{msg}       regex2
    regex1: Regex,
    regex2: Regex,

    // Extracts a pattern starting with `//~`.
    //pattern_regex: Regex,
}

impl<'a> Parser<'a> {
    fn new(content: &'a str) -> Self {
        Self {
            expected_msgs: Vec::new(),
            last_line_with_msg: 0,
            content,
            regex1: Regex::new(r"//~(^*) ([[:alpha:]]+):(.+)").unwrap(),
            regex2: Regex::new(r"//~\| ([[:alpha:]]+):(.+)").unwrap(),
        }
    }

    fn parse(&mut self) -> Result<(), Box<Error>> {
        for (lineno, line) in self.content.lines()
            .enumerate()
            .map(|(lineno, line)| (lineno + 1, line)) {

            self.parse_line(lineno, line)?;
        }

        Ok(())
    }

    /// Parses a line and adds any contained message to the expected message list.
    fn parse_line(&mut self, lineno: usize, line: &str) -> Result<(), Box<Error>> {
        let (target_line, kind, pattern) = {
            if let Some(cap) = self.regex1.captures(line) {
                debug!("matched line with regex1: {}", line);

                let (offset, kind, pattern) = (
                    cap.get(1).unwrap().as_str(),
                    cap.get(2).unwrap().as_str(),
                    cap.get(3).unwrap().as_str()
                );
                debug!("offset string: {} (len={}, current line={})", offset, offset.len(), lineno);

                match lineno.checked_sub(offset.len()) {
                    Some(n) if n > 0 => {
                        // valid line number
                        (n, kind, pattern)
                    }
                    _ => {
                        return Err(format!("in line {}: invalid line offset before line 1", lineno).into());
                    }
                }
            } else if let Some(cap) = self.regex2.captures(line) {
                debug!("matched line with regex2: {}", line);

                // This form uses the same target line as the pattern in the line before (which is
                // required).

                // The last line must contain a pattern.
                let last_line = if self.last_line_with_msg == lineno - 1 {
                    self.expected_msgs.last().unwrap().line_num
                } else {
                    return Err(format!(
                        "in line {}: a `//|` pattern must be directly preceded by another pattern",
                        lineno
                    ).into());
                };

                let (kind, pattern) = (
                    cap.get(1).unwrap().as_str(),
                    cap.get(2).unwrap().as_str()
                );

                (last_line, kind, pattern)
            } else {
                // no match, no pattern, go on
                return Ok(());
            }
        };

        let kind = kind.parse::<MessageKind>()
            .map_err(|()| format!("invalid message type '{}'", kind))?;
        let pattern = pattern.trim().to_string();

        if pattern.is_empty() {
            return Err(format!("in line {}: empty pattern", lineno).into());
        }

        let msg = Pattern {
            kind: Some(kind),   // TODO
            msg: pattern,
            line_num: target_line,
        };

        info!("parsed message pattern: {:?}", msg);
        self.expected_msgs.push(msg);

        Ok(())
    }
}
