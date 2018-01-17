//! Parses compile-fail tests to extract expected errors.

// Note: This does not support any kind of header directive that compiletest-rs supports

use json::Message;

use std::error::Error;
use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;

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

/// Describes which part of a message should be matched by a pattern.
#[derive(Debug, Eq, PartialEq)]
pub enum Matcher {
    /// Match the error code (eg. `E0918`).
    ///
    /// Since error codes don't change across Rust versions, this is a more future-proof alternative
    /// to matching error message strings.
    Code(String),

    /// Match the error message reported by the compiler (eg. `cannot borrow immutable ...`).
    ///
    /// Since error messages can change between Rust versions, matching error codes should be
    /// preferred.
    Msg(String),
}

/// A pattern that can match a compiler message.
#[derive(Debug, Eq, PartialEq)]
pub struct Pattern {
    /// The kind of message we expect.
    pub kind: Option<MessageKind>,
    /// Describes which messages this pattern matches.
    pub matcher: Matcher,
    /// The line at which the message must point.
    pub line_num: usize,
}

impl Pattern {
    /// Determines whether this `Pattern` matches a `Message` from the compiler.
    pub fn matches(&self, msg: &Message) -> bool {
        if self.kind != msg.kind {
            // kind must match *exactly*
            return false;
        }

        if self.line_num != msg.line_num {
            // line must match *exactly*
            return false;
        }

        // The pattern must be a substring of the message. For this reason, patterns may not be the
        // empty string (they would match everything).
        match self.matcher {
            Matcher::Code(ref code) if msg.code.as_ref() != Some(code) => {
                return false;
            }
            Matcher::Msg(ref message) if !msg.msg.contains(message) => {
                return false;
            }
            _ => {}
        }

        info!("matches: pattern {:?} matches message {:?}", self, msg);
        true
    }
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

        let patterns = Parser::new().parse(&content)?;

        if patterns.is_empty() {
            return Err(format!("no error patterns found in {}", path.display()).into());
        }

        Ok(TestExpectation {
            expected_msgs: patterns,
        })
    }
}

struct Parser {
    expected_msgs: Vec<Pattern>,
    /// Last line number that contained a parsed `Message`. 0 if none were parsed yet.
    last_line_with_pattern: usize,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            expected_msgs: Vec::new(),
            last_line_with_pattern: 0,
        }
    }

    pub fn parse(mut self, content: &str) -> Result<Vec<Pattern>, Box<Error>> {
        for (lineno, line) in content.lines()
            .enumerate()
            .map(|(lineno, line)| (lineno + 1, line)) {

            if let Some(pat) = self.parse_line(lineno, line)? {
                self.last_line_with_pattern = lineno;
                self.expected_msgs.push(pat);
            }
        }

        Ok(self.expected_msgs)
    }

    /// Parses a line which may contain a `Pattern`.
    pub fn parse_line(&self, lineno: usize, line: &str) -> Result<Option<Pattern>, Box<Error>> {
        const START: &'static str = "//~";
        if let Some(start) = line.find(START) {
            // line contains a `//~` pattern
            let pat = &line[start+START.len()..];
            let pattern = self.parse_pattern(pat, lineno)?;
            Ok(Some(pattern))
        } else {
            Ok(None)
        }
    }

    fn parse_pattern(&self, mut pattern: &str, lineno: usize) -> Result<Pattern, Box<Error>> {
        // The beginning of the pattern determines the line it matches.
        // "|"         => same line as pattern on last line
        // "^" times N => N lines above the current one
        // _           => this line
        let mut chars = pattern.chars();
        let target_line = if chars.next() == Some('|') {
            // This form uses the same target line as the pattern in the line before (which is
            // required).
            pattern = chars.as_str();

            // The last line must contain a pattern.
            if self.last_line_with_pattern != 0 && self.last_line_with_pattern == lineno - 1 {
                self.expected_msgs.last().unwrap().line_num
            } else {
                return Err(format!(
                    "in line {}: a `//~|` pattern must be directly preceded by another pattern",
                    lineno
                ).into());
            }
        } else {
            // reset iterator
            let offset = pattern.chars().take_while(|&c| c == '^').count();
            pattern = &pattern[offset..];
            debug!("offset: {} (current line={}), left = '{}'", offset, lineno, pattern);

            match lineno.checked_sub(offset) {
                Some(n) if n > 0 => {
                    // valid line number
                    n
                }
                _ => {
                    return Err(format!("in line {}: invalid line offset before line 1", lineno).into());
                }
            }
        };

        // The next item is the message kind (error/warn/note/etc). This is (for now) mandatory,
        // even though rustc apparently doesn't always attach a kind.
        pattern = pattern.trim_left();
        let kind_str = pattern.chars().take_while(|c| c.is_alphabetic()).collect::<String>();
        let kind = kind_str.parse::<MessageKind>()
            .map_err(|()| format!("'{}' is an invalid message kind", kind_str))?;
        pattern = &pattern[kind_str.len()..];
        debug!("kind = {} = {:?}, left = '{}'", kind_str, kind, pattern);

        // Now, we can either match an error code in brackets like `error[E0001]`, or a message
        // after a colon (`error: cannot borrow ...`).
        let mut chars = pattern.chars();
        let matcher = match chars.next() {
            Some(':') => {
                let message = chars.as_str().trim_left();
                pattern = &pattern[0..0];   // consumed
                if message.is_empty() {
                    return Err(format!("in line {}: error patterns may not be empty", lineno).into());
                }

                Matcher::Msg(message.to_string())
            }
            Some('[') => {
                let code = chars.take_while(|&c| c != ']').collect::<String>();
                pattern = &pattern[code.len()+2..];
                Matcher::Code(code)
            }
            _ => return Err(format!("expected `: <message>` or `[Exxxx]`").into()),
        };

        // Make sure `pattern` is now empty
        if !pattern.is_empty() {
            return Err(format!("unconsumed input in pattern: '{}'", pattern).into());
        }

        Ok(Pattern {
            matcher,
            kind: Some(kind),
            line_num: target_line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(lineno: usize, line: &str) -> Pattern {
        let p = Parser::new();
        p.parse_line(lineno, line).unwrap().unwrap()
    }

    fn patterns(text: &str) -> Vec<Pattern> {
        let p = Parser::new();
        p.parse(text).unwrap()
    }

    fn invalid_pattern(line: &str, err_msg: &str) {
        let err = Parser::new().parse_line(1, line).unwrap_err().to_string();
        assert!(err.contains(err_msg), "'{}' does not contain '{}'", err, err_msg);
    }

    #[test]
    fn rejects_invalid_patterns() {
        invalid_pattern("some code //~", "");
        invalid_pattern("//~^ error: msg", "line"); // invalid line
        invalid_pattern("//~| error: msg", "must be directly preceded");
        invalid_pattern("//~ invalid: good message", "invalid message kind");
        invalid_pattern("//~ error:", "error patterns may not be empty");
        invalid_pattern("//~ error another: bla", "expected `:");
        invalid_pattern("//~ error[code]: but also message", "unconsumed input");
    }

    #[test]
    fn parses_patterns() {
        assert_eq!(pattern(1, "//~ eRrOr: message"), Pattern {
            kind: Some(MessageKind::Error),
            matcher: Matcher::Msg("message".to_string()),
            line_num: 1,
        });
        assert_eq!(pattern(1, "//~ ERROR[E0001]"), Pattern {
            kind: Some(MessageKind::Error),
            matcher: Matcher::Code("E0001".to_string()),
            line_num: 1,
        });
        assert_eq!(pattern(4, "//~^^^ ERROR[E0001]"), Pattern {
            kind: Some(MessageKind::Error),
            matcher: Matcher::Code("E0001".to_string()),
            line_num: 1,
        });
        assert_eq!(patterns("\
                //~ ERROR[E0001]\n\
                //~|   note: massage   "), vec![
            Pattern {
                kind: Some(MessageKind::Error),
                matcher: Matcher::Code("E0001".to_string()),
                line_num: 1,
            },
            Pattern {
                kind: Some(MessageKind::Note),
                matcher: Matcher::Msg("massage   ".to_string()),
                line_num: 1,
            },
        ]);
        assert_eq!(patterns("\
                hello i am good code yes
                //~^ ERROR[some code]\n\
                //~|warn: massage"), vec![
            Pattern {
                kind: Some(MessageKind::Error),
                matcher: Matcher::Code("some code".to_string()),
                line_num: 1,
            },
            Pattern {
                kind: Some(MessageKind::Warning),
                matcher: Matcher::Msg("massage".to_string()),
                line_num: 1,
            },
        ]);
    }
}
