use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use crate::types::{Entry, STRUCT_DRIFT_PREFIX};

/// Error type for JSONL parsing.
///
/// Three distinct failure modes — keep them separate so callers can route
/// differently (e.g. `LenientReader` increments different counters for IO,
/// malformed JSON, and known-type-shape drift):
///
/// - [`ParseError::Io`]: I/O while reading the underlying file.
/// - [`ParseError::Json`]: the JSON text itself is malformed (truncated line,
///   unbalanced braces, etc.).
/// - [`ParseError::StructDrift`]: the line is valid JSON, the `type` field
///   identifies a known entry kind, but the typed payload failed to decode
///   (e.g. `usage.input_tokens` came in as a string). This is the v2
///   "shape changed under us" canary — we deliberately do not soft-degrade
///   here, because soft-degrading a known type would mask schema regressions.
#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
    Io(io::Error),
    StructDrift {
        /// The original `type` discriminator value, e.g. `"assistant"`.
        entry_type: String,
        /// The underlying deserialization error message. Stored as `String`
        /// because the original `serde_json::Error` is consumed when we lift
        /// the sentinel marker out of the deserializer's `Error::custom` path.
        message: String,
    },
}

impl From<io::Error> for ParseError {
    fn from(e: io::Error) -> Self {
        ParseError::Io(e)
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        // Promote the StructDrift sentinel from inside a serde error message
        // back into the typed variant. This is the only way to ship a typed
        // discriminator out of `Deserialize::deserialize` — serde's error
        // type can't carry custom data, so we encode the kind in the error
        // string and parse it here.
        let message = e.to_string();
        if let Some(rest) = message.strip_prefix(STRUCT_DRIFT_PREFIX) {
            if let Some((entry_type, inner)) = rest.split_once(':') {
                return ParseError::StructDrift {
                    entry_type: entry_type.to_string(),
                    message: inner.to_string(),
                };
            }
        }
        ParseError::Json(e)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "JSON parse error: {e}"),
            ParseError::Io(e) => write!(f, "IO error: {e}"),
            ParseError::StructDrift {
                entry_type,
                message,
            } => write!(
                f,
                "struct drift on known entry type `{entry_type}`: {message}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a single JSONL line into an Entry.
pub fn parse_entry(line: &str) -> Result<Entry, ParseError> {
    serde_json::from_str(line).map_err(ParseError::from)
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
            struct_drift_count: 0,
            unknown_count: 0,
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
    struct_drift_count: usize,
    unknown_count: usize,
}

impl LenientReader {
    /// Number of lines that failed to parse and were skipped (malformed JSON
    /// or IO error). Does *not* count `StructDrift` — those are counted
    /// separately via [`LenientReader::struct_drift_count`].
    pub fn errors_skipped(&self) -> usize {
        self.errors_skipped
    }

    /// Number of lines where the JSON was well-formed and the `type`
    /// discriminator was known, but the typed payload failed to decode (v2
    /// `ParseError::StructDrift`). A non-zero value is a hard signal that
    /// Claude Code shipped a schema change for one of the modelled entry
    /// types — bump the cc-session-jsonl dependency.
    pub fn struct_drift_count(&self) -> usize {
        self.struct_drift_count
    }

    /// Number of entries successfully parsed as [`Entry::Passthrough`]
    /// (unrecognized type that carries `uuid` + `sessionId` — DAG continuity
    /// preserved) or [`Entry::Ignored`] (unrecognized type without DAG fields).
    ///
    /// A non-zero value here is an early signal of Claude Code JSONL format
    /// drift: Anthropic has added a new entry type since this library's types
    /// were defined. Consumers should treat a rising ratio as a hint to bump
    /// the `cc-session-jsonl` dependency.
    pub fn unknown_count(&self) -> usize {
        self.unknown_count
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
                Ok(entry) => {
                    if matches!(entry, Entry::Passthrough(_) | Entry::Ignored) {
                        self.unknown_count += 1;
                    }
                    return Some(entry);
                }
                Err(ParseError::StructDrift { .. }) => {
                    // Schema drift: still skip the line so the iterator stays
                    // lenient, but count it on its own axis. Real-data smoke
                    // tests assert `struct_drift_count() == 0`.
                    self.struct_drift_count += 1;
                    continue;
                }
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
        let line = r#"{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[{"type":"text","text":"hi"}]}}"#;
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
    fn parse_entry_unknown_no_dag_fields_becomes_ignored() {
        // No uuid / sessionId → Ignored
        let line = r#"{"type":"never-seen-before","data":"x"}"#;
        let entry = parse_entry(line).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn parse_entry_unknown_with_dag_fields_becomes_passthrough() {
        // Has uuid + sessionId → Passthrough (DAG continuity)
        let line = r#"{"type":"future-feature","uuid":"u1","sessionId":"s1","data":"x"}"#;
        let entry = parse_entry(line).unwrap();
        assert!(matches!(entry, Entry::Passthrough(_)));
    }

    #[test]
    fn parse_entry_known_type_bad_shape_is_struct_drift() {
        // `assistant` is a known type; `message.usage.input_tokens` here is
        // a string instead of a u64 — that's a typed-payload failure, which
        // surfaces as ParseError::StructDrift (not ParseError::Json).
        let line = r#"{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","usage":{"input_tokens":"not-a-number"}}}"#;
        let err = parse_entry(line).unwrap_err();
        match err {
            ParseError::StructDrift { entry_type, .. } => {
                assert_eq!(entry_type, "assistant");
            }
            other => panic!("Expected StructDrift, got: {other}"),
        }
    }

    #[test]
    fn parse_entry_user_bad_shape_is_struct_drift() {
        // `user` is known; `imagePasteIds` should be an array of strings,
        // here it arrives as an object → StructDrift on the user kind.
        let line = r#"{"type":"user","uuid":"u1","sessionId":"s1","imagePasteIds":{"oops":true}}"#;
        let err = parse_entry(line).unwrap_err();
        match err {
            ParseError::StructDrift { entry_type, .. } => assert_eq!(entry_type, "user"),
            other => panic!("Expected StructDrift, got: {other}"),
        }
    }

    // ── SessionReader tests ──

    #[test]
    fn session_reader_open_and_iterate() {
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"one"}}
{"type":"user","uuid":"u2","sessionId":"s1","message":{"role":"user","content":"two"}}
{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[{"type":"text","text":"reply"}]}}"#;

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
        assert_eq!(lenient.struct_drift_count(), 0);
    }

    #[test]
    fn lenient_reader_counts_struct_drift_separately() {
        // 1 OK, 1 struct drift (known type, bad shape), 1 garbage line, 1 OK.
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"ok"}}
{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","usage":{"input_tokens":"oops"}}}
this is not json
{"type":"user","uuid":"u2","sessionId":"s1","message":{"role":"user","content":"ok"}}"#;
        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(lenient.struct_drift_count(), 1);
        assert_eq!(lenient.errors_skipped(), 1);
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
        assert!(
            error_ratio < 0.1,
            "Error ratio {error_ratio} should be < 10%"
        );
    }

    // ── ParseError Display ──

    #[test]
    fn parse_error_display_json() {
        let err = parse_entry("not json").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("JSON parse error"));
    }

    #[test]
    fn parse_error_display_struct_drift() {
        let line = r#"{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","usage":{"input_tokens":"oops"}}}"#;
        let err = parse_entry(line).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("struct drift"));
        assert!(msg.contains("assistant"));
    }

    #[test]
    fn lenient_reader_counts_unknown_entries() {
        // Mix: 2 recognized, 2 unknown (1 Ignored + 1 Passthrough), 1 garbage
        let content = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"ok"}}
{"type":"future-type-alpha","sessionId":"s1","data":1}
GARBAGE
{"type":"future-type-beta","uuid":"u2","sessionId":"s2"}
{"type":"tag","sessionId":"s1","tag":"t"}"#;
        let file = write_temp_file(content);
        let reader = SessionReader::open(file.path()).unwrap();
        let mut lenient = reader.lenient();
        let entries: Vec<_> = lenient.by_ref().collect();
        assert_eq!(entries.len(), 4); // user + 2 unknown (Ignored + Passthrough) + tag
        assert_eq!(lenient.errors_skipped(), 1); // 1 garbage line
        assert_eq!(lenient.unknown_count(), 2); // 1 Ignored + 1 Passthrough
    }
}
