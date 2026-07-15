//! # techy
//!
//! A fast, extensible parser toolkit for LaTeX-like markup languages.
//!
//! techy builds an Abstract Syntax Tree (AST) from LaTeX-like source code, allowing you to
//! analyze, transform, or convert documents. The engine has no privileged language concepts
//! (no built-in math mode, `{`/`}`, `%`, or `\`); the familiar LaTeX behavior is provided by
//! a preset, and custom LaTeX-like languages are defined with the same machinery.
//!
//! ## no_std
//!
//! The crate is `no_std`-friendly: it depends only on `core` and `alloc` (sources are shared
//! as `Arc`, so the target must support atomics). Consequently the library performs no I/O
//! of its own — content lookup for `\input`-like constructs is delegated to the embedder via
//! the [`SourceResolver`] trait.
//!
//! ## Architecture
//!
//! The crate is organized in three strata (see `ARCHITECTURE.md` §3): S0, a Lang-free
//! foundation; S1, the mutually-recursive core (whose modules are topics, not dependency
//! ranks); and S2, the presets. It is being rebuilt phase by phase:
//!
//! - [`source`] (S0) — source content, plain byte [`Span`]s, `Arc`-based [`SourceSpan`]s,
//!   provenance, pluggable resolution, lazy line/column analysis. **Implemented (Phase 1).**
//! - [`error`] — span-based diagnostics and the tolerant-parsing policy.
//!   **Implemented (Phase 1).**
//! - [`token`] (S1) — zero-copy tokens, tokenization rules, the `TokenReader<L>` trait and
//!   the standard state-driven reader. **Implemented (Phases 2–3).**
//! - [`state`] (S1) — the `Lang` trait, parsing state, and reified state deltas.
//!   **Implemented (Phase 3).**
//! - [`spec`] + [`library`] (S1) — de-keyed callable specs (argument/slot structures)
//!   and definition libraries (query-based lookup, lexical shadowing, per-type
//!   unknown-callable fallbacks). **Implemented (Phase 4)**; the structure specs and the
//!   `invocation_parser()` escape hatch grow with their consumers in Phase 6.
//! - [`node`] (S1) — the flat, immutable node tree: closed structural [`NodeKind`],
//!   two-tier ext system, span-or-owned [`TextContent`] payloads, `NodeRef` proxy access.
//!   **Implemented (Phase 5)**; the whitespace/span invariants and the concrete
//!   `ArgsLayout` syntax records grow with the parsers in Phase 6.
//! - [`constructs`] + [`engine`] (S1) — construct parsers and the parse-session API.
//!   **In progress (Phase 6)**: the contracts ([`ConstructParser`], [`ParseContext`],
//!   [`Invocation`], [`ParserSession`], [`ParseError`]) landed in 6.1; the content
//!   dispatch loop ([`NodesParser`], stop conditions, [`StopCause`]) landed in 6.2;
//!   groups ([`GroupParser`], the [`ChildStateSpec`] descent policy, the session
//!   derivation seam, `check_tree_invariants`) landed in 6.3; invocations, arguments,
//!   and environment bodies land in 6.4–6.6.
//! - `latexlike` preset (S2) — the familiar LaTeX behavior. *Phase 7.*
//!
//! ## Quick start (what exists today)
//!
//! ```rust
//! use std::sync::Arc;
//! use techy::source::{Source, SourceSpan};
//!
//! let source: Arc<Source> = Arc::new(Source::new(r"Hello \world{}!"));
//! let span = SourceSpan::new(&source, 6..12);
//! assert_eq!(span.content(), r"\world");
//!
//! // Line/column information is computed lazily, for display only:
//! let mut line_index = source.line_index();
//! assert_eq!(line_index.line_col(span.start()), Some((1, 7)));
//! ```

// no_std-friendly, alloc-only (see ARCHITECTURE.md); tests build with std for convenience.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

// The `techy-derive` macros emit `::techy::…` paths so that generated code resolves in
// downstream crates; this self-alias makes those paths resolve inside techy itself.
extern crate self as techy;

pub mod constructs;
pub mod engine;
pub mod error;
pub mod library;
pub mod node;
pub mod source;
pub mod spec;
pub mod state;
pub mod token;

/// Support module for `techy-derive`-generated code only: `alloc` paths spelled so they
/// resolve from both `std` and `no_std` consumer crates. Not public API.
#[doc(hidden)]
pub mod __private {
    pub use alloc::string::String;
    pub use alloc::vec::Vec;
}

// The remaining module of the previous exploratory implementation (`parser`) is kept in
// the tree as a quarry but is not compiled; it is rebuilt phase-by-phase per
// ARCHITECTURE.md §9 (superseded quarry files are kept alongside the new modules as
// `*_JUNK._rs`).

// Re-export the public API of the implemented topics.
pub use constructs::{
    ChildStateSpec, ConstructParser, ConstructParserResult, EnvironmentTerminatorMismatch,
    ExpectedExpressionArgument, ExpressionCallableRequiresContent, GroupChildState,
    GroupParser, ImplementationError, Invocation, InvocationChildState,
    MalformedEnvironmentTerminator, MissingEnvironmentTerminator, MissingMandatoryArgument,
    MissingTerminatorFound, NodesOutcome, NodesParser, ParseContext, StopCause, StopSpec,
    TokenStopCondition, TokenStopKind, UnclosedGroup, UnclosedGroupFound, UnresolvableCommand,
    UnusableRecoveryToken, UnusableRecoveryTokenKind,
};
pub use engine::{Frame, FrameTitle, ParseResult, ParserSession};
pub use error::{
    format_position, format_traceback, Diagnostic, DiagnosticData, DiagnosticInfo,
    DiagnosticValue, Diagnostics, ParseError, Recovery, Severity, ToDiagnosticValue,
    TraceFrame,
};
pub use source::{
    resolve_source, LineIndex, MapResolver, NoResolver, ProvenanceChain, ResolveError,
    ResolvedContent, Source, SourceContent,
    SourceCursor, SourceOrigin, SourceProvenance, SourceResolver, SourceSpan, Span, TextContent,
};
pub use library::{CallableQuery, CallableSyntax, Library, LibraryStack, SpecLookup};
pub use node::{
    check_tree_invariants, BuildId, CallableData, ChildRegion, ContentNodes, GroupData,
    NodeBuildError, NodeData, NodeId, NodeKind, NodeRef, NodeTree, NodeTreeBuilder,
    ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots, StagedNodeView, StagedNodes,
};
pub use spec::{
    ArgumentParser, ArgumentSpec, CallableSpec, FrameRole, ParsedArgumentNodes,
    StdCallableSpec,
};
pub use state::{
    Lang, NodeExtTypes, ParsingState, ParsingStateDelta, ResolvedCallable, SimpleLang, StateData,
    TokenRulesOverrides,
};
pub use token::{
    skip_whitespace, CommandRule, CommentRule, EndOfStreamAfterEscape, ForbiddenChar,
    GroupRule, PrefixTable, SpecialsMatch, StdTokenReader, Token, TokenError, TokenErrorKind,
    TokenKind, TokenReader, TokenRecovery, TokenResult, TokenRules,
    TriggerChars, WhitespaceRules,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
