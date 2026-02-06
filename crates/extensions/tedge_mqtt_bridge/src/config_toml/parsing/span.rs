//! Custom span type for parsing substrings of larger documents.
//!
//! When parsing a toml value, the input is a substring of the original file.
//! Chumsky spans are relative to the start of the parsed string (0-based).
//! This module provides `OffsetSpan` which automatically applies a base offset
//! when accessing span boundaries, so errors point to the correct location
//! in the original file.

use chumsky::span::Span;
use chumsky::span::WrappingSpan;
use std::fmt;
use std::ops::Range;

/// A span that automatically applies a base offset when accessed.
///
/// This is useful when parsing a substring of a larger document. The offset
/// is applied in `start()` and `end()` methods, so code that reads the span
/// gets the correct position in the original document without manual arithmetic.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct OffsetSpan {
    local_start: usize,
    local_end: usize,
    offset: usize,
}

impl OffsetSpan {
    /// Convert to a Range with the offset already applied.
    pub fn into_range(self) -> Range<usize> {
        self.start()..self.end()
    }
}

impl Span for OffsetSpan {
    /// The context stores the base offset to apply.
    type Context = usize;
    type Offset = usize;

    fn new(offset: Self::Context, range: Range<usize>) -> Self {
        Self {
            local_start: range.start,
            local_end: range.end,
            offset,
        }
    }

    fn context(&self) -> usize {
        self.offset
    }

    fn start(&self) -> usize {
        self.local_start + self.offset
    }

    fn end(&self) -> usize {
        self.local_end + self.offset
    }
}

impl From<OffsetSpan> for Range<usize> {
    fn from(span: OffsetSpan) -> Self {
        span.into_range()
    }
}

impl fmt::Display for OffsetSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start(), self.end())
    }
}

impl<T> WrappingSpan<T> for OffsetSpan {
    type Spanned = (T, OffsetSpan);

    fn make_wrapped(self, inner: T) -> Self::Spanned {
        (inner, self)
    }

    fn inner_of(spanned: &Self::Spanned) -> &T {
        &spanned.0
    }

    fn span_of(spanned: &Self::Spanned) -> &Self {
        &spanned.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_is_applied_to_start_and_end() {
        let span = OffsetSpan::new(100, 5..10);
        assert_eq!(span.start(), 105);
        assert_eq!(span.end(), 110);
    }

    #[test]
    fn into_range_applies_offset() {
        let span = OffsetSpan::new(50, 0..5);
        assert_eq!(span.into_range(), 50..55);
    }

    #[test]
    fn context_returns_offset() {
        let span = OffsetSpan::new(42, 0..10);
        assert_eq!(span.context(), 42);
    }

    #[test]
    fn offsets_can_be_stringified() {
        let span = OffsetSpan::new(42, 0..10);
        assert_eq!(span.to_string(), "42..52");
    }
}
