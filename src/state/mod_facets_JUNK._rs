//! Parsing state management.

use crate::error::TokenizerError;
use crate::token::tokenreader::Result as


use crate::state::parsingstatedatatrait::parsing_state_data_trait; // macro


pub trait ParsingStateSpecialsLibraryProvider : Default {
    fn test_for_specials_strings(&self, s : & 'a str, parsing_state: &ParsingState)
     -> Option<(&'a str, usize)>;
}


parsing_state_data_trait! {
    pub trait ParsingStateWhitespace;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateWhitespaceData {
        whitespace_handling_enabled : bool = true,
        whitespace_chars : String = " \t\n".to_string(),
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct GroupTypeData {
    open_delimiter : String = "{".to_string(),
    close_delimiter : String = "}".to_string(),
    enabled : bool = true,
}

parsing_state_data_trait! {
    pub trait ParsingStateGroups;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateGroupsData<GroupType> {
        groups_enabled : bool = true,
        /// Pairs of opening/closing group delimiters.  In standard LaTeX, this would
        /// be ('{','}').  We use the same mechanism, however, to parse optional arguments
        /// ("\cite[optional-arg]{key}") as well as inline/display math mode ("\[...\]",
        /// "$...$", "$$...$$", etc.)
        group_types : Vec<GroupTypeData> = vec![ GroupTypeData::default() ],
    }
}

parsing_state_data_trait! {
    pub trait ParsingStateMacros;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateMacrosData {
        macros_enabled: bool = true,
        macro_escape_char: char = '\\',
        macro_alpha_chars: String = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
    }
}

parsing_state_data_trait! {
    pub trait ParsingStateEnvironments;
    #[derive(Debug,Clone,Copy,PartialEq)]
    pub struct ParsingStateEnvironmentsData {
        environments_enabled: bool = true,
        // environments are identified by the parser/nodes-collector (not the tokenizer
        // as in pylatexenc).  That class can decide how exactly environments are
        // recognized/parsed.
    }
}

parsing_state_data_trait! {
    pub trait ParsingStateSpecials;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateSpecialsData {
        specials_enabled: bool = true,
        // specials are tested using the library.
    }
}

parsing_state_data_trait! {
    pub trait ParsingStateComments;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateCommentsData {
        comments_enabled : bool = true,
        comment_start : String = "%".to_string(),
    }
}

parsing_state_data_trait! {
    pub trait ParsingStateMultiNewlineParagraphs;
    #[derive(Debug,Clone,Copy,PartialEq)]
    pub struct ParsingStateMultiNewlineParagraphsData {
        multi_newline_paragraphs_enabled: bool = true,
    }
}

parsing_state_data_trait! {
    pub trait ParsingStateForbidden;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateForbiddenData {
        forbidden_characters: String,
        forbidden_specials: Vec<String>,
    }
}


parsing_state_data_trait! {
    pub trait ParsingStateTechyContentMode;
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateTechyContentModeData {
        in_math_mode : bool = false,
    }
}



/// Configuration for Parsing behavior.
///
/// Controls which language features are enabled and how tokens are recognized.

pub trait ParsingStateTrait {
    type ParsingStateWhitespace : ParsingStateWhitespace;
    type ParsingStateGroups : ParsingStateGroups;
    type ParsingStateMacros : ParsingStateMacros;
    type ParsingStateEnvironments : ParsingStateEnvironments;
    type ParsingStateSpecials : ParsingStateSpecials;
    type ParsingStateComments : ParsingStateComments;
    type ParsingStateMultiNewlineParagraphs : ParsingStateMultiNewlineParagraphs;
    type ParsingStateForbidden : ParsingStateForbidden;
    type LibraryProvider : ParsingStateSpecialsLibraryProvider;

    fn whitespace(&self) -> & ParsingStateWhitespace;
    fn groups(&self) -> & ParsingStateGroups;
    fn macros(&self) -> & ParsingStateMacros;
    fn environments(&self) -> & ParsingStateEnvironments;
    fn specials(&self) -> & ParsingStateSpecials;
    fn comments(&self) -> & ParsingStateComments;
    fn multi_newline_paragraphs(&self) -> & ParsingStateMultiNewlineParagraphs;
    fn forbidden(&self) -> & ParsingStateForbidden;
    fn library_provider(&self) -> & LibraryProvider;
}


#[derive(Debug,Clone,Default)]
pub struct ParsingStateData<
    ParsingStateWhitespace : ParsingStateWhitespace,
    ParsingStateGroups : ParsingStateGroups,
    ParsingStateMacros : ParsingStateMacros,
    ParsingStateEnvironments : ParsingStateEnvironments,
    ParsingStateSpecials : ParsingStateSpecials,
    ParsingStateComments : ParsingStateComments,
    ParsingStateMultiNewlineParagraphs : ParsingStateMultiNewlineParagraphs,
    ParsingStateForbidden : ParsingStateForbidden,
    LibraryProvider : ParsingStateSpecialsLibraryProvider
    > {

    whitespace : ParsingStateWhitespace,

    groups : ParsingStateGroups,

    macros : ParsingStateMacros,

    environments : ParsingStateEnvironments,

    specials : ParsingStateSpecials,

    comments : ParsingStateComments,

    multi_newline_paragraphs: ParsingStateMultiNewlineParagraphs,

    forbidden : ParsingStateForbidden,

    library_provider : LibraryProvider

}

impl ParsingStateTrait for ParsingStateData<
    ParsingStateWhitespace : ParsingStateWhitespace,
    ParsingStateGroups : ParsingStateGroups,
    ParsingStateMacros : ParsingStateMacros,
    ParsingStateEnvironments : ParsingStateEnvironments,
    ParsingStateSpecials : ParsingStateSpecials,
    ParsingStateComments : ParsingStateComments,
    ParsingStateMultiNewlineParagraphs : ParsingStateMultiNewlineParagraphs,
    ParsingStateForbidden : ParsingStateForbidden,
    LibraryProvider : ParsingStateSpecialsLibraryProvider
    > {

    type ParsingStateWhitespace = ParsingStateWhitespace;
    type ParsingStateGroups = ParsingStateGroups;
    type ParsingStateMacros = ParsingStateMacros;
    type ParsingStateEnvironments = ParsingStateEnvironments;
    type ParsingStateSpecials = ParsingStateSpecials;
    type ParsingStateComments = ParsingStateComments;
    type ParsingStateMultiNewlineParagraphs = ParsingStateMultiNewlineParagraphs;
    type ParsingStateForbidden = ParsingStateForbidden;
    type LibraryProvider = ParsingStateSpecialsLibraryProvider;
    
    fn whitespace(&self) -> & ParsingStateWhitespace { &self.whitespace };
    fn groups(&self) -> & ParsingStateGroups { &self.groups };
    fn macros(&self) -> & ParsingStateMacros { &self.macros };
    fn environments(&self) -> & ParsingStateEnvironments  { &self.environments };
    fn specials(&self) -> & ParsingStateSpecials { &self.specials };
    fn comments(&self) -> & ParsingStateComments { &self.comments };
    fn multi_newline_paragraphs(&self) -> & ParsingStateMultiNewlineParagraphs  { &self.multi_newline_paragraphs };
    fn forbidden(&self) -> & ParsingStateForbidden { &self.forbidden };
    fn library_provider(&self) -> & LibraryProvider { &self.library_provider };


    fn derived(&self) -> _ParsingStateDerivedBuilder<Self> {
        _ParsingStateDerivedBuilder<Self>(self)
    }
}

struct _ParsingStateDerivedBuilder<PS : ParsingStateTrait> {
    ......
}




#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenPrefixType {
    GroupOpen,
    GroupClose,
    // if ambiguous (exists as open or close delimiter/might depend on context):
    GroupOpenOrClose,
}


#[derive(Debug,Clone,PartialEq)]
pub struct ParsingState<PS : ParsingStateTrait> {
    state : PS
    cached_prefix_strings : Vec<(&str, TokenPrefixType)>;
}
impl ParsingState<PS : ParsingStateTrait> {

    pub fn state(&self) -> & PS { self.state }

    pub fn whitespace(&self) -> & PS::ParsingStateWhitespace {
        self.state.whitespace()
    }

    pub fn groups(&self) -> & PS::ParsingStateGroups {
        self.state.groups()
    }

    pub fn macros(&self) -> & PS::ParsingStateMacros {
        self.state.macros()
    }

    pub fn environments(&self) -> & PS::ParsingStateEnvironments {
        self.state.environments()
    }

    pub fn specials(&self) -> & PS::ParsingStateSpecials {
        self.state.specials()
    }

    pub fn comments(&self) -> & PS::ParsingStateComments {
        self.state.comments()
    }

    pub fn multi_newline_paragraphs(&self) -> & PS::ParsingStateMultiNewlineParagraphs {
        self.state.multi_newline_paragraphs()
    }

    pub fn forbidden(&self) -> & PS::ParsingStateForbidden {
        self.state.forbidden()
    }

    pub fn library_provider(&self) -> &LibraryProvider {
        self.state.library_provider()
    }

    fn new(state : PS) -> Self {
        let mut items: Vec<(String, TokenPrefixType)> = Vec::new();

        // Helper to add or merge prefix items
        let mut add_or_merge_prefix = |s: &str, typ: TokenPrefixType| {
            if !s.is_empty() {
                match items.iter().position(|x| x.0 == s) {
                    None => {
                        items.push((s.to_string(), typ));
                    }
                    Some(pos) => {
                        // If the string already exists as GroupOpen or GroupClose,
                        // and we're adding the opposite type, mark it as GroupOpenOrClose
                        let existing_typ = items[pos].1;
                        if (existing_typ == TokenPrefixType::GroupOpen && typ == TokenPrefixType::GroupClose)
                            || (existing_typ == TokenPrefixType::GroupClose && typ == TokenPrefixType::GroupOpen)
                        {
                            items[pos].1 = TokenPrefixType::GroupOpenOrClose;
                        }
                        // Otherwise, do not warn for duplicates, because groups might have
                        // the same open/close sequence (e.g. "$...$" and "$$...$$")
                    }
                }
            }
        };

        // Add group open & close delimiters
        let groups = state.groups();
        if groups.groups_enabled() {
            for group_type in &groups.group_types {
                if (group_type.enabled) {
                    let open = group_type.open_delimiter;
                    let close = group_type.close_delimiter;
                    add_or_merge_prefix(open, TokenPrefixType::GroupOpen);
                    add_or_merge_prefix(close, TokenPrefixType::GroupClose);
                }
            }
        }

        // Sort by length (descending) - longer strings first to match greedily
        items.sort_unstable_by_key(|a| a.0.len());

        Self {
            state,
            cached_prefix_strings: items,
        }
    }
}

