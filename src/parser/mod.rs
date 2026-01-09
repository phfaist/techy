//! High-level parsing API.
//!
//! The `Parser` provides the main entry point for parsing LaTeX-like documents.


use crate::source::Source;
use crate::source::SourceLocation;


pub trait LanguageSpecification {
    type ParsingState;
    type SourceOrigin : Default;

    type Source = Source<Self::SourceOrigin>
    type SourceLocation<'src> = SourceLocation<'src, Self::SourceOrigin>;
}



