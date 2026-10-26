//! Errors returned by parsing and serialization.

use std::fmt;

/// A syntax or resource-limit error, positioned in the original UTF-8 input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable explanation.
    pub message: String,
    /// Zero-based UTF-8 byte offset.
    pub offset: usize,
    /// One-based source line.
    pub line: usize,
    /// One-based Unicode scalar column (not a byte or display-width column).
    pub column: usize,
}
impl ParseError {
    pub(crate) fn at(source: &str, offset: usize, message: impl Into<String>) -> Self {
        let before = &source[..offset];
        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        Self {
            message: message.into(),
            offset,
            line,
            column,
        }
    }
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{} (byte {})",
            self.message, self.line, self.column, self.offset
        )
    }
}
impl std::error::Error for ParseError {}

/// A caller-built value has no faithful SxPB spelling, or exceeds the depth limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializeError {
    /// Human-readable explanation.
    pub message: String,
}
impl SerializeError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for SerializeError {}
