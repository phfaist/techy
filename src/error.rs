//! Span-based diagnostics, the tolerant-parsing policy, and the parse abort error
//! ([`ParseError`]).
//!
//! Diagnostics carry Arc-based [`SourceSpan`]s, so they are self-contained and outlive the
//! parse that produced them — no `'src` lifetime spreads through error signatures. Library
//! conditions are reported through diagnostics (accumulated by the parser session, available
//! on the parse result), never through a logging side channel (ARCHITECTURE.md Decision 5).
//!
//! The token-level error type carrying a recovery token (`TokenError`) lives with `Token` in
//! the token layer (Phase 2); the [`Recovery`] policy that governs it is defined here, as is
//! [`ParseError`] — the construct-parsing abort (Phase 6), which deliberately carries **no**
//! recovery payload (DESIGN_RATIONALE.md §3.8).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::source::{SourceOrigin, SourceProvenance, SourceSpan};
use crate::token::TokenErrorKind;

/// How severe a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational note.
    Note,
    /// Something suspicious that did not prevent parsing.
    Warning,
    /// A genuine error (in tolerant mode, recorded instead of aborting the parse).
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}

/// A single condition reported during parsing, anchored to a source location.
#[derive(Debug, Clone)]
pub struct Diagnostic<O: SourceOrigin = Option<String>> {
    severity: Severity,
    message: String,
    span: SourceSpan<O>,
}

impl<O: SourceOrigin> Diagnostic<O> {
    /// Create a diagnostic.
    pub fn new(severity: Severity, message: impl Into<String>, span: SourceSpan<O>) -> Self {
        Diagnostic { severity, message: message.into(), span }
    }

    /// Create an error-severity diagnostic.
    pub fn error(message: impl Into<String>, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Error, message, span)
    }

    /// Create a warning-severity diagnostic.
    pub fn warning(message: impl Into<String>, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Warning, message, span)
    }

    /// Create a note-severity diagnostic.
    pub fn note(message: impl Into<String>, span: SourceSpan<O>) -> Self {
        Diagnostic::new(Severity::Note, message, span)
    }

    /// The severity of this diagnostic.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Where in the source the condition occurred.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }

    /// Render a human-readable multi-line report: message, position (line/column via
    /// [`LineIndex`](crate::source::LineIndex), origin label), and the source's provenance
    /// chain (`included from …` / `synthesized from …`).
    pub fn render(&self) -> String {
        let mut msg = self.to_string();
        msg.push_str("\n  at: ");
        msg.push_str(&format_position(&self.span));

        let mut provenance = self.span.source().provenance();
        loop {
            match provenance {
                SourceProvenance::Primary => break,
                SourceProvenance::Resolved { reference, triggered_at } => {
                    msg.push_str(&format!(
                        "\n  included from {} ({})",
                        format_position(triggered_at),
                        reference,
                    ));
                    provenance = triggered_at.source().provenance();
                }
                SourceProvenance::Synthesized { description, triggered_at } => {
                    msg.push_str(&format!(
                        "\n  synthesized from {} ({})",
                        format_position(triggered_at),
                        description,
                    ));
                    provenance = triggered_at.source().provenance();
                }
            }
        }

        msg
    }
}

impl<O: SourceOrigin> fmt::Display for Diagnostic<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.message)
    }
}

/// An ordered collection of [`Diagnostic`]s, as accumulated over one parse.
#[derive(Debug, Clone)]
pub struct Diagnostics<O: SourceOrigin = Option<String>> {
    items: Vec<Diagnostic<O>>,
}

impl<O: SourceOrigin> Diagnostics<O> {
    /// Create an empty collection.
    pub fn new() -> Self {
        Diagnostics { items: Vec::new() }
    }

    /// Append a diagnostic.
    pub fn push(&mut self, diagnostic: Diagnostic<O>) {
        self.items.push(diagnostic);
    }

    /// Number of diagnostics recorded.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no diagnostics were recorded.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether any recorded diagnostic has [`Severity::Error`].
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    /// Iterate over the diagnostics in the order they were recorded.
    pub fn iter(&self) -> core::slice::Iter<'_, Diagnostic<O>> {
        self.items.iter()
    }

    /// The diagnostics as a slice.
    pub fn as_slice(&self) -> &[Diagnostic<O>] {
        &self.items
    }
}

impl<O: SourceOrigin> Default for Diagnostics<O> {
    fn default() -> Self {
        Diagnostics::new()
    }
}

impl<O: SourceOrigin> IntoIterator for Diagnostics<O> {
    type Item = Diagnostic<O>;
    type IntoIter = alloc::vec::IntoIter<Diagnostic<O>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, O: SourceOrigin> IntoIterator for &'a Diagnostics<O> {
    type Item = &'a Diagnostic<O>;
    type IntoIter = core::slice::Iter<'a, Diagnostic<O>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// Tolerant-parsing policy: what to do when an error carries a recovery possibility.
///
/// Under `Strict`, the parse aborts on the first error. Under `Tolerant`, a diagnostic is
/// recorded and parsing continues with the error's recovery token where one is available
/// (the recovery-token mechanism itself arrives with the token layer, Phase 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Abort on the first error.
    Strict,
    /// Record a diagnostic and continue whenever recovery is possible.
    Tolerant,
}

/// What aborted a parse (see [`ParseError`]).
///
/// Closed enum per the naming rule (`…Kind`), but `#[non_exhaustive]`: parse-level
/// conditions are added as the construct parsers grow (Phase 6.2–6.6).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// A tokenization error, escalated to an abort under [`Recovery::Strict`] (tolerant
    /// mode continues with the error's recovery token instead).
    Token(TokenErrorKind),
    /// A parse-level syntax condition — the same message that tolerant mode records as
    /// an error-severity [`Diagnostic`] before continuing with its local recovery.
    Syntax {
        /// Human-readable description of the condition.
        message: String,
    },
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::Token(kind) => fmt::Display::fmt(kind, f),
            ParseErrorKind::Syntax { message } => f.write_str(message),
        }
    }
}

/// The abort error of construct parsing: a strict-mode escalation or a genuinely
/// unrecoverable condition.
///
/// **`Err` means abort** (DESIGN_RATIONALE.md §3.8): recovery happens where a problem is
/// detected (the session's recovery helper), abnormal endings of sub-parses travel as
/// data (`StopCause`, Phase 6.2), and nobody ever continues *past* a `ParseError` — it
/// carries no recovery payload and bubbles freely. Like [`Diagnostic`], it holds an
/// Arc-based [`SourceSpan`], so it is self-contained and outlives the parse.
#[derive(Debug, Clone)]
pub struct ParseError<O: SourceOrigin = Option<String>> {
    kind: ParseErrorKind,
    span: SourceSpan<O>,
}

impl<O: SourceOrigin> ParseError<O> {
    /// Create a parse error.
    pub fn new(kind: ParseErrorKind, span: SourceSpan<O>) -> ParseError<O> {
        ParseError { kind, span }
    }

    /// What aborted the parse.
    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }

    /// Where in the source the condition occurred.
    pub fn span(&self) -> &SourceSpan<O> {
        &self.span
    }

    /// Render a human-readable multi-line report (message, position, provenance chain),
    /// like [`Diagnostic::render`].
    pub fn render(&self) -> String {
        Diagnostic::error(self.kind.to_string(), self.span.clone()).render()
    }
}

impl<O: SourceOrigin> fmt::Display for ParseError<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.kind, f)
    }
}

impl<O: SourceOrigin> core::error::Error for ParseError<O> {}

/// Format a span's starting position for display: `@ (line 2, col 5) [origin]`, falling
/// back to `@ char pos 42` when line information is unavailable (huge sources, see
/// [`LineIndex`](crate::source::LineIndex)). The `[origin]` part is omitted when the
/// source's origin has no label.
pub fn format_position<O: SourceOrigin>(span: &SourceSpan<O>) -> String {
    let source = span.source();
    let mut line_index = source.line_index();

    let pos_str = match line_index.line_col(span.start()) {
        Some((line, col)) => format!("@ (line {}, col {})", line, col),
        None => format!("@ char pos {}", span.start()),
    };

    match source.origin().label() {
        Some(label) => format!("{} [{}]", pos_str, label),
        None => pos_str,
    }
}

/// Format a traceback showing open blocks (innermost first), one line per block, using each
/// block's own source for position and origin information.
///
/// Returns an empty string if `open_blocks` is empty.
///
/// # Example output
///
/// ```text
/// Open blocks:
///   @ (line 8, col 1): environment 'document'
///   @ (line 5, col 3): macro '\section'
/// ```
pub fn format_traceback<O: SourceOrigin>(open_blocks: &[(SourceSpan<O>, String)]) -> String {
    if open_blocks.is_empty() {
        return String::new();
    }

    let mut result = String::from("Open blocks:");
    for (span, what) in open_blocks {
        result.push_str("\n  ");
        result.push_str(&format_position(span));
        result.push_str(": ");
        result.push_str(what);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use std::sync::Arc;

    fn origin(url: &str) -> Option<String> {
        Some(url.to_string())
    }

    fn arc_source(content: &str) -> Arc<Source> {
        Arc::new(Source::new(content))
    }

    #[test]
    fn diagnostic_render_includes_line_info() {
        let source = arc_source("Hello\n\\unknown\nworld");
        let span = SourceSpan::new(&source, 6..14);
        let diagnostic = Diagnostic::error("unknown macro", span);

        let rendered = diagnostic.render();
        assert!(rendered.contains("error: unknown macro"));
        assert!(rendered.contains("line 2"));
    }

    #[test]
    fn diagnostic_span_accessors() {
        let source = arc_source("Hello\nWorld\nTest");
        let span = SourceSpan::new(&source, 10..15);
        let diagnostic = Diagnostic::error("test", span);

        assert_eq!(diagnostic.span().start(), 10);
        assert_eq!(diagnostic.span().end(), 15);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.message(), "test");
    }

    #[test]
    fn diagnostic_render_with_origin() {
        let source: Arc<Source> = Arc::new(
            Source::new("Hello\n\\unknown\nworld").with_origin(origin("test.tex")),
        );
        let span = SourceSpan::new(&source, 6..14);
        let diagnostic = Diagnostic::error("unknown macro", span);

        let rendered = diagnostic.render();
        assert!(rendered.contains("line 2"));
        assert!(rendered.contains("[test.tex]"));
    }

    #[test]
    fn diagnostic_render_walks_provenance_chain() {
        let document: Arc<Source> = Arc::new(
            Source::new(r"\input{main.tex}").with_origin(origin("document.tex")),
        );
        let main: Arc<Source> = Arc::new(Source::resolved(
            "\\mycommand\n",
            "main.tex",
            SourceSpan::entire(&document),
        ));
        let expanded: Arc<Source> = Arc::new(Source::synthesized(
            "expansion text",
            "macro expansion",
            SourceSpan::new(&main, 0..10),
        ));

        let diagnostic = Diagnostic::error("boom", SourceSpan::new(&expanded, 0..9));
        let rendered = diagnostic.render();

        assert!(rendered.contains("error: boom"));
        assert!(rendered.contains("synthesized from"));
        assert!(rendered.contains("(macro expansion)"));
        assert!(rendered.contains("included from"));
        assert!(rendered.contains("(main.tex)"));
        assert!(rendered.contains("[document.tex]"));
    }

    #[test]
    fn diagnostics_collection() {
        let source = arc_source("Hello");
        let mut diagnostics: Diagnostics = Diagnostics::new();
        assert!(diagnostics.is_empty());
        assert!(!diagnostics.has_errors());

        diagnostics.push(Diagnostic::warning("odd", SourceSpan::new(&source, 0..1)));
        assert!(!diagnostics.has_errors());

        diagnostics.push(Diagnostic::error("bad", SourceSpan::new(&source, 1..2)));
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.has_errors());

        let severities: Vec<Severity> = diagnostics.iter().map(|d| d.severity()).collect();
        assert_eq!(severities, vec![Severity::Warning, Severity::Error]);
    }

    #[test]
    fn format_traceback_single_block() {
        let source = arc_source("Hello\nWorld\nTest");

        let open_blocks =
            vec![(SourceSpan::new(&source, 6..11), "environment 'document'".to_string())];

        let traceback = format_traceback(&open_blocks);
        assert_eq!(traceback, "Open blocks:\n  @ (line 2, col 1): environment 'document'");
    }

    #[test]
    fn format_traceback_multiple_open_blocks() {
        let source = arc_source("Hello\nWorld\nTest\nMore");

        let open_blocks = vec![
            (SourceSpan::new(&source, 12..16), "macro '\\textbf'".to_string()),
            (SourceSpan::new(&source, 6..11), "environment 'document'".to_string()),
            (SourceSpan::new(&source, 0..5), "macro '\\section'".to_string()),
        ];

        let traceback = format_traceback(&open_blocks);
        assert_eq!(
            traceback,
            "Open blocks:\n  @ (line 3, col 1): macro '\\textbf'\n  @ (line 2, col 1): environment 'document'\n  @ (line 1, col 1): macro '\\section'"
        );
    }

    #[test]
    fn format_traceback_empty_open_blocks() {
        let open_blocks: Vec<(SourceSpan, String)> = vec![];
        assert_eq!(format_traceback(&open_blocks), "");
    }

    #[test]
    fn format_traceback_with_origin() {
        let source: Arc<Source> = Arc::new(
            Source::new("Hello\nWorld\nTest").with_origin(origin("test.tex")),
        );

        let open_blocks =
            vec![(SourceSpan::new(&source, 6..11), "environment 'document'".to_string())];

        let traceback = format_traceback(&open_blocks);
        assert_eq!(
            traceback,
            "Open blocks:\n  @ (line 2, col 1) [test.tex]: environment 'document'"
        );
    }

    #[test]
    fn format_traceback_multiple_blocks_with_origin() {
        let source: Arc<Source> = Arc::new(
            Source::new("Hello\nWorld\nTest\nMore").with_origin(origin("myfile.tex")),
        );

        let open_blocks = vec![
            (SourceSpan::new(&source, 12..16), "macro '\\textbf'".to_string()),
            (SourceSpan::new(&source, 6..11), "environment 'document'".to_string()),
        ];

        let traceback = format_traceback(&open_blocks);
        assert_eq!(
            traceback,
            "Open blocks:\n  @ (line 3, col 1) [myfile.tex]: macro '\\textbf'\n  @ (line 2, col 1) [myfile.tex]: environment 'document'"
        );
    }

    #[test]
    fn format_position_char_pos_fallback() {
        // A source too large for line indexing falls back to raw byte positions.
        let content = "a\n".repeat(DEFAULT_MAX_TEST_LEN);
        let source = arc_source(&content);
        let span = SourceSpan::new(&source, 42..43);

        let formatted = format_position(&span);
        assert_eq!(formatted, "@ char pos 42");
    }

    // Exceeds LineIndex's default max scan length of 100_000 bytes ("a\n" is 2 bytes).
    const DEFAULT_MAX_TEST_LEN: usize = 60_000;
}
