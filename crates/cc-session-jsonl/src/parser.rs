use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use crate::types::Entry;

/// Error type for JSONL parsing.
#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
    Io(io::Error),
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        ParseError::Json(e)
    }
}

impl From<io::Error> for ParseError {
    fn from(e: io::Error) -> Self {
        ParseError::Io(e)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "JSON parse error: {e}"),
            ParseError::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a single JSONL line into an Entry.
pub fn parse_entry(line: &str) -> Result<Entry, ParseError> {
    Ok(serde_json::from_str(line)?)
}

/// Iterator over entries in a JSONL session file.
pub struct SessionReader {
    lines: io::Lines<BufReader<File>>,
}

impl SessionReader {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            lines: BufReader::new(file).lines(),
        })
    }

    /// Switch to lenient mode: skip unparseable lines instead of returning errors.
    pub fn lenient(self) -> LenientReader {
        LenientReader {
            lines: self.lines,
            errors_skipped: 0,
        }
    }
}

impl Iterator for SessionReader {
    type Item = Result<Entry, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line = match self.lines.next()? {
                Ok(l) => l,
                Err(e) => return Some(Err(ParseError::Io(e))),
            };
            if line.trim().is_empty() {
                continue;
            }
            return Some(parse_entry(&line));
        }
    }
}

/// Lenient iterator that skips unparseable lines.
pub struct LenientReader {
    lines: io::Lines<BufReader<File>>,
    errors_skipped: usize,
}

impl LenientReader {
    /// Number of lines that failed to parse and were skipped.
    pub fn errors_skipped(&self) -> usize {
        self.errors_skipped
    }
}

impl Iterator for LenientReader {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line = self.lines.next()?.ok()?;
            if line.trim().is_empty() {
                continue;
            }
            match parse_entry(&line) {
                Ok(entry) => return Some(entry),
                Err(_) => {
                    self.errors_skipped += 1;
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    // ── parse_entry tests ──

    #[test]
    fn parse_entry_valid_user() {
        let line = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"hello"}}"#;
        let entry = parse_entry(line).unwrap();
        assert!(matches!(entry, Entry::User(_)));
    }

    #[test]
    fn parse_entry_valid_assistant() {
        let line = r#"{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[{"type":"text","text":"hi"}]}}"#;
        let entry = parse_entry(line).unwrap();
        assert!(matches!(entry, Entry::Assistant(_)));
    }

    #[test]
    fn parse_entry_invalid_json() {
        let line = "this is not json at all";
        let result = parse_entry(line);
        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::Json(_) => {} // expected
            other => panic!("Expected Json error, got: {other}"),
        }
    }

    #[test]
    fn parse_entry_empty_string() {
        // Empty string is not valid JSON
        let result = parse_entry("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_entry_unknown_type() {
        let line = r#"{"type":"never-seen-before","data":"x"}"#;
        let entry = parse_entry(line).unwrap();
        assert!(matches!(entry, Entry::Unknown));
    }

    // ── SessionReader tests ──

    #[test]
    fn session_reader_open_and_iterate() {
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"one"}}
{"type":"user","uuid":"u2","sessionId":"s1","message":{"role":"user","content":"two"}}
{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[{"type":"text","text":"reply"}]}}"#;

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let entries: Vec<_> = reader.collect::<Vec<_>>();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn session_reader_counts_entries() {
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"x"}}
{"type":"ai-title","sessionId":"s1","aiTitle":"title"}
{"type":"tag","sessionId":"s1","tag":"t"}
{"type":"mode","sessionId":"s1","mode":"code"}
{"type":"summary","leafUuid":"l1","summary":"s"}"#;

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let count = reader.count();
        assert_eq!(count, 5);
    }

    #[test]
    fn session_reader_skips_empty_lines() {
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"x"}}


{"type":"user","uuid":"u2","sessionId":"s1","message":{"role":"user","content":"y"}}
"#;

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let entries: Vec<_> = reader.collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn session_reader_returns_error_on_bad_line() {
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"x"}}
NOT VALID JSON
{"type":"user","uuid":"u2","sessionId":"s1","message":{"role":"user","content":"y"}}"#;

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let results: Vec<_> = reader.collect();
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_err()); // bad line
        assert!(results[2].is_ok());
    }

    #[test]
    fn session_reader_nonexistent_file() {
        let result = SessionReader::open("/tmp/nonexistent-file-abc123xyz.jsonl");
        assert!(result.is_err());
    }

    // ── LenientReader tests ──

    #[test]
    fn lenient_reader_skips_bad_lines() {
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"one"}}
GARBAGE LINE
{"not a valid entry
{"type":"user","uuid":"u2","sessionId":"s1","message":{"role":"user","content":"two"}}"#;

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(lenient.errors_skipped(), 2);
    }

    #[test]
    fn lenient_reader_all_bad_lines() {
        let content = "garbage1\ngarbage2\ngarbage3\n";

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        assert!(entries.is_empty());
        assert_eq!(lenient.errors_skipped(), 3);
    }

    #[test]
    fn lenient_reader_empty_file() {
        let file = write_temp_file("");
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        assert!(entries.is_empty());
        assert_eq!(lenient.errors_skipped(), 0);
    }

    #[test]
    fn lenient_reader_skips_empty_lines() {
        let content = r#"
{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"x"}}


{"type":"tag","sessionId":"s1","tag":"ok"}
"#;

        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(lenient.errors_skipped(), 0);
    }

    #[test]
    fn lenient_reader_error_ratio() {
        // Simulates 1 bad line out of 11 total non-empty lines
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!(
                r#"{{"type":"user","uuid":"u{i}","sessionId":"s1","message":{{"role":"user","content":"msg {i}"}}}}"#
            ));
        }
        lines.push("BAD LINE".to_string());
        let content = lines.join("\n");

        let file = write_temp_file(&content);
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        let total = entries.len() + lenient.errors_skipped();
        let error_ratio = lenient.errors_skipped() as f64 / total as f64;
        assert_eq!(entries.len(), 10);
        assert_eq!(lenient.errors_skipped(), 1);
        assert!(error_ratio < 0.1, "Error ratio {error_ratio} should be < 10%");
    }

    // ── ParseError Display ──

    #[test]
    fn parse_error_display_json() {
        let err = parse_entry("not json").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("JSON parse error"));
    }
}
