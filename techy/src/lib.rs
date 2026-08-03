//! # techy
//!
//! A fast, extensible parser toolkit for LaTeX-like markup languages.
//!
//! techy builds an Abstract Syntax Tree (AST) from LaTeX-like source code, allowing you to
//! analyze, transform, or convert documents. The engine has no privileged language concepts
//! (no built-in math mode, `{`/`}`, `%`, or `\`); the familiar LaTeX behavior is provided by
//! a preset, and custom LaTeX-like languages are defined with the same machinery.
//!
//! New to techy? Start with the [`guide`] — the narrative documentation; the modules
//! below are the API reference.
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
//! The crate is organized in three strata: S0, a `Lang`-free foundation; S1, the
//! mutually recursive core (whose modules are topics, not dependency ranks); and S2,
//! the presets:
//!
//! - [`source`] (S0) — source content, plain byte [`Span`]s, `Arc`-based [`SourceSpan`]s,
//!   provenance, pluggable resolution, lazy line/column analysis.
//! - [`error`] (S0) — span-based structured diagnostics and the tolerant-parsing
//!   policy.
//! - [`token`] (S1) — zero-copy tokens, data-driven tokenization rules, the
//!   `TokenReader<L>` trait and the standard state-driven reader.
//! - [`state`] (S1) — the `Lang` trait, immutable parsing state, and reified state
//!   deltas.
//! - [`spec`] + [`scopes`] (S1) — de-keyed callable specs (argument/slot structures)
//!   and the definition scope stack (dyn `SpecsProvider` entries, lexical shadowing,
//!   in-stack fallback providers, definition/stack delta ops).
//! - [`node`] (S1) — the flat, immutable node tree: closed structural [`NodeKind`],
//!   two-tier ext system, span-or-owned [`TextContent`] payloads, `NodeRef` proxy
//!   access, `check_tree_invariants`.
//! - [`constructs`] (S1) — construct parsing: the [`ConstructParser`] contract and its
//!   one-value context ([`ParseContext`], [`Invocation`]), the content dispatch loop
//!   ([`NodesParser`], stop conditions, [`StopCause`]), groups ([`GroupParser`], the
//!   [`ChildStateSpec`] descent policy), invocations, arguments, environment bodies,
//!   and verbatim.
//! - [`engine`] (S1) — the parse-session machinery: [`Language`], [`ParserSession`],
//!   [`ParseDriver`], [`ParseResult`], and the session derivation seam.
//! - [`latexlike`] preset (S2) — the familiar LaTeX behavior: the [`Latexlike`](latexlike::Latexlike)
//!   lang with text/math modes, scope-stack command resolution, default token rules and
//!   base specials, environments (`\begin`/`\end`), verbatim, and `NodeRef` accessor
//!   sugar. Preset items are namespaced (`techy::latexlike::…`), not re-exported at
//!   the crate root.
//!
//! ## Quick start
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

// no_std-friendly, alloc-only ([§dd-dr:dependencies]); tests build with std for convenience.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

// The `techy-derive` macros emit `::techy::…` paths so that generated code resolves in
// downstream crates; this self-alias makes those paths resolve inside techy itself.
extern crate self as techy;

pub mod constructs;
pub mod engine;
pub mod error;
pub mod latexlike;
pub mod node;
pub mod scopes;
pub mod source;
pub mod spec;
pub mod state;
pub mod token;

// Narrative documentation: markdown pages in the workspace-level `docs/` rendered as
// doc-only modules. `cfg(doc)` keeps them out of compiled code; rustdoc (including
// doctest collection) builds with `--cfg doc`, so code blocks in these pages still run
// as doctests. The docs sidebar pins these pages in a "Guide" section
// (docs/rustdoc-header.html, wired up in .cargo/config.toml); new chapters must also be
// added to GUIDE_PAGES there.
#[cfg(doc)]
#[doc = include_str!("../../docs/guide.md")]
pub mod guide {
    #[doc = include_str!("../../docs/learn-by-example.md")]
    pub mod learn_by_example {}

    #[doc = include_str!("../../docs/parsing-model.md")]
    pub mod parsing_model {}

    #[doc = include_str!("../../docs/concepts-overview.md")]
    pub mod concepts_overview {}
}

/// Support module for `techy-derive`-generated code only: everything the generated code
/// references — `alloc` paths spelled so they resolve from both `std` and `no_std`
/// consumer crates, plus the diagnostics items the derives implement/construct. The
/// derives emit only `::techy::__private::…` paths (the serde discipline), so the
/// public topology never constrains, and is never constrained by, derive output.
/// Not public API.
#[doc(hidden)]
pub mod __private {
    pub use alloc::string::String;
    pub use alloc::vec::Vec;

    pub use crate::error::{DiagnosticInfo, DiagnosticValue, ToDiagnosticValue};
}


// Re-export the public API of the implemented topics.
pub use constructs::{
    ChildStateSpec, CommandResolutionFailed, ConstructParser, ConstructParserResult,
    EnvironmentTerminatorMismatch,
    ExpectedExpressionArgument, ExpressionCallableRequiresContent, GroupChildState,
    GroupParser, ImplementationError, Invocation, InvocationChildState,
    MalformedEnvironmentTerminator, MissingEnvironmentTerminator, MissingMandatoryArgument,
    MissingTerminatorFound, NodesOutcome, NodesParser, ParseContext, ScopeOpFailed, StopCause,
    StopSpec, StrayGroupClose, TokenStopCondition, TokenStopKind, UnclosedGroup,
    UnclosedGroupFound, UnresolvableCommand, UnusableRecoveryToken, UnusableRecoveryTokenKind,
};
pub use engine::{
    CommandResolution, Frame, FrameTitle, Language, ParseDriver, ParseResult, ParserSession,
    ResolvedCallable, StdParseDriver,
};
pub use error::{
    format_position, format_traceback, Diagnostic, DiagnosticData, DiagnosticInfo,
    DiagnosticValue, Diagnostics, ParseError, Recovery, Severity, ToDiagnosticValue,
    TraceFrame,
};
pub use source::{
    resolve_source_reference, LineIndex, MapResolver, NoResolver, ProvenanceChain, ResolveError,
    ResolvedContent, Source, SourceOrigin, SourceProvenance, SourceResolver, SourceSpan,
    Span, TextContent,
};
pub use node::{
    check_tree_invariants, BuildId, CallableData, ChildRegion, ContentNodes, Descendants,
    GroupData, NodeBuildError, NodeData, NodeId, NodeKind, NodeRef, NodeSlice, NodeSliceIter,
    NodeTree, NodeTreeBuilder, ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots,
    StagedNodeView, StagedNodes,
};
pub use scopes::{
    CallableDefinedAsError, CallableQuery, CallableSyntax, DefinitionOp, ErrorCallableSpec,
    FallbackProvider, Package, ProviderError, Scope, ScopeOp, ScopeOpError, ScopeStack,
    ScopeStackError, SearchedProviders, SpecsProvider, SymbolEntry,
};
pub use spec::{
    ArgumentParser, ArgumentSpec, CallableSpec, FrameRole, ParsedArgumentNodes,
    StdCallableSpec,
};
pub use state::{
    ClosedVocabulary, DeriveError, Lang, NodeExtTypes, ParsingState, ParsingStateDelta,
    TrivialLang, StateData, TokenRulesOverrides,
};
pub use token::{
    skip_whitespace, CommandRule, CommentRule, EndOfStreamAfterEscape, ForbiddenChar,
    GroupRule, PrefixTable, SpecialsMatch, StdTokenReader, Token, TokenError, TokenErrorKind,
    TokenKind, TokenReader, TokenRecovery, TokenResult, TokenRules,
    TriggerChars, WhitespaceRules,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
