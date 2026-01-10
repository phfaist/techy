//! High-level parsing API.
//!
//! The `Parser` provides the main entry point for parsing LaTeX-like documents.


use crate::source::{Source, SourceOrigin, SourceLocation};

use crate::state::*;


pub trait LanguageSpecification {


    type ParsingStateWhitespace : ParsingStateWhitespace = ParsingStateWhitespaceData;
    type ParsingStateGroups : ParsingStateGroups = ParsingStateGroupsData;
    type ParsingStateMacros : ParsingStateMacros = ParsingStateMacros;
    type ParsingStateEnvironments : ParsingStateEnvironments = ParsingStateEnvironmentsData;
    type ParsingStateSpecials : ParsingStateSpecials = ParsingStateSpecialsData;
    type ParsingStateComments : ParsingStateComments = ParsingStateCommentsData;
    type ParsingStateMultiNewlineParagraphs : ParsingStateMultiNewlineParagraphs = ParsingStateMultiNewlineParagraphsData;
    type ParsingStateForbidden : ParsingStateForbidden = ParsingStateForbiddenData;

    type LibraryProvider; // : ... = ...

    type ParsingStateTrait : ParsingStateTrait = ParsingStateData<
        Self::ParsingStateWhitespace,
        Self::ParsingStateGroups,
        Self::ParsingStateMacros,
        Self::ParsingStateEnvironments,
        Self::ParsingStateSpecials,
        Self::ParsingStateComments,
        Self::ParsingStateMultiNewlineParagraphs,
        Self::ParsingStateForbidden,
        Self::LibraryProvider
    >;

    type ParsingState = ParsingState<ParsingStateTrait>;

    type SourceOrigin : SourceOrigin;

    type Source = Source<Self::SourceOrigin>;
    type SourceLocation<'src> = SourceLocation<'src, Self::SourceOrigin>;
}



