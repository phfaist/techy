//! Parsing state management.

use crate::error::TokenizerError;
use crate::token::tokenreader::Result as


use crate::state::parsingstatedatatrait::parsing_state_data_trait; // macro


pub trait ParsingStateSpecialsLibraryProvider {
    fn test_for_specials_strings(&self, s : & 'a str, parsing_state: &ParsingState)
     -> Option<(&'a str, usize)>;
}


parsing_state_data_trait! {
    #[derive(Debug,Clone,PartialEq)]
    pub struct ParsingStateWhitespaceData {
        enable_whitespace_handling : bool = true,
        whitespace_chars : String = " \t\n".to_string(),
    }
}
impl Default for ParsingStateWhitespaceData {
    fn default() -> Self {
        Self {
            enable_whitespace_handling: true,
            whitespace_chars: " \t\n".to_string(),
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct GroupTypeData {
    open_delimiter: String,
    close_delimiter: String,
    enabled: bool,
}
impl Default for GroupTypeData {
    fn default() -> Self {
        Self {
            open_delimiter: "{".to_string(),
            close_delimiter: "}".to_string(),
            enabled: true,
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateGroupData {
    enable_groups : bool,
    /// Pairs of opening/closing group delimiters.  In standard LaTeX, this would
    /// be ('{','}').  We use the same mechanism, however, to parse optional arguments
    /// ("\cite[optional-arg]{key}") as well as inline/display math mode ("\[...\]",
    /// "$...$", "$$...$$", etc.)
    group_types : Vec<GroupTypeData>,
}
impl Default for ParsingStateGroupData {
    fn default() -> Self {
        Self {
            enable_groups : true,
            group_types : vec![ GroupTypeData::default() ],
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateMacrosData {
    enable_macros: bool,
    macro_escape_char: char,
    macro_alpha_chars: String,
}
impl Default for ParsingStateMacrosData {
    fn default() -> Self {
        Self {
            enable_macros: true,
            macro_escape_char: '\\',
            macro_alpha_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
        }
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub struct ParsingStateEnvironmentData {
    enable_environments: bool,
    // environments are identified by the parser/nodes-collector (not the tokenizer
    // as in pylatexenc).  That class can decide how exactly environments are
    // recognized/parsed.
}
impl Default for ParsingStateEnvironmentData {
    fn default() -> Self {
        Self {
            enable_environments: true,
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateSpecialsData {
    enable_specials: bool,
    // specials are tested using the library.
}
impl Default for ParsingStateSpecialsData {
    fn default() -> Self {
        Self {
            enable_specials: true,
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateCommentsData {
    enable_comments : bool,
    comment_start : String,
}
impl Default for ParsingStateCommentsData {
    fn default() -> Self {
        Self {
            enable_comments: true,
            comment_start: "%".to_string(),
        }
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub struct ParsingStateMultiNewlineParagraphsData {
    enable_multi_newline_paragraphs: bool,
}
impl Default for ParsingStateMultiNewlineParagraphsData {
    fn default() -> Self {
        Self {
            enable_multi_newline_paragraphs: true,
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateForbiddenData {
    forbidden_characters: String,
    forbidden_specials: Vec<String>,
}
impl Default for ParsingStateForbiddenData {
    fn default() -> Self {
        Self {
            // Forbidden characters - use Unicode escape sequences
            // \r (carriage return) - ensure proper newlines with \n only
            forbidden_characters: "\r".to_string(),
            forbidden_specials: vec![],
        }
    }
}

#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateLatexModeData {
    in_math_mode: bool,
}
impl Default for ParsingStateLatexModeData {
    fn default() -> Self {
        Self {
            // Forbidden characters - use Unicode escape sequences
            // \r (carriage return) - ensure proper newlines with \n only
            in_math_mode: false,
        }
    }
}


/// Configuration for Parsing behavior.
///
/// Controls which language features are enabled and how tokens are recognized.
#[derive(Default,Debug,Clone)]
pub struct ParsingStateData<LibraryProvider : ParsingStateSpecialsLibraryProvider> {
    
    whitespace : ParsingStateWhitespaceData,

    groups : ParsingStateGroupData,

    environments : ParsingStateEnvironmentData,

    specials : ParsingStateSpecialsData,

    comments : ParsingStateCommentsData,

    multi_newline_paragraphs: ParsingStateMultiNewlineParagraphsData,

    forbidden : ParsingStateForbiddenData,

    library_provider : LibraryProvider

}









// #[derive(Debug, Clone, Copy, PartialEq)]
// pub enum TokenPrefixType {
//     GroupOpen,
//     GroupClose,
//     // if ambiguous (exists as open or close delimiter/might depend on context):
//     GroupOpenOrClose,
// }

// /// Instances implementing ParsingState are a token that identifies the current
// /// state of how to parse things.  The state is communicated to the tokenizer and
// /// the parsers by exposing some methods that explicitly parse some low-level
// /// stuff.
// pub trait ParsingState {

//     type ParsingStateData;
//     fn data(&self) -> &ParsingStateData;

//     fn detect_whitespace<'a>(&self, s: &'a str) -> TokenReaderResult<(&'a str, usize)>;

//     fn detect_delimiters_prefix<'a>(&self, &'a str)
//      -> TokenReaderResult<Option<(&'a str, usize, TokenPrefixType)>>;

//     fn detect_macro<'a>(&self, &'a str) -> TokenReaderResult<Option<(&'a str, usize)>>;

//     fn detect_comment_start<'a>(&self, &'a str) -> TokenReaderResult<Option<(&'a str, usize)>>;

//     fn detect_multi_newline_paragraphs_in_whitespace(&self, &'a str, )
//      -> TokenReaderResult<Option<(usize,usize)>>;

//     fn detect_forbidden(&self, &'a str) -> TokenReaderResult<Option<(&'a str, usize)>>;

// }


#[derive(Debug,Clone,PartialEq)]
pub struct ParsingStateFromData<LibraryProvider> {
    data : ParsingStateData
    cached_prefix_strings : Vec<(&str, TokenPrefixType)>;
}
impl ParsingStateFromData<LibraryProvider> {
    fn new(data : ParsingStateData) -> Self {
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
        if data.groups.enable_groups {
            for group_type in &data.groups.group_types {
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
            data,
            cached_prefix_strings: items,
        }
    }
}
impl ParsingState for ParsingStateFromData<LibraryProvider> {

    type ParsingStateData = ParsingStateData;

    fn data(&self) -> &ParsingStateData { &self.data }


}

