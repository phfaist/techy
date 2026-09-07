//! Argument codes: the short strings, such as `"o"` and `"m"`, that declare which
//! arguments a callable takes.
//!
//! [`argument_specs`] is the entry point most definitions use: it takes one code per
//! argument (`["o", "m"]`) and returns the configured [`ArgumentSpec`]s that a spec
//! type stores — what [`MacroSpec::new`](super::MacroSpec::new) and its siblings take.
//! [`argument_specs_named`] builds the same specs from `(code, name)` pairs, so the
//! arguments can be read back by name, and [`argument_specs_from_str`] reads a whole
//! argument structure from one compact string (`"om"`) — the form pylatexenc's
//! definitions are written in.
//!
//! The complete code table is on [`argument_specs`]; the guide introduces the codes
//! under [argument codes](crate::guide::specs#argument-codes).
//!
//! Which parser a code selects depends on the code alone and never on anything a parse
//! discovers, so the codes are resolved once, when the spec is built: a code that is
//! not one of the known ones is an [`ArgumentCodeError`] returned right there, never a
//! parse-time diagnostic and never a silent fallback.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    EmbellishmentsArgumentParser, GroupArgumentParser, MarkerArgumentParser,
    OptionalGroupArgumentParser, VerbatimArgumentParser,
};
use crate::node::ArgumentExt;
use crate::spec::{ArgumentParser, ArgumentSpec};
use crate::token::GroupRule;

use super::{LatexlikeGroupType, LatexlikeLang};

/// A malformed argument code, returned when the argument specs are built.
///
/// [`argument_specs`], [`argument_specs_named`] and [`argument_specs_from_str`] all
/// return this. An unusable code is a mistake in a definition, so it is reported
/// before any document is parsed rather than diagnosed during a parse.
///
/// Every error locates itself with two coordinates. `index` is the position of the
/// offending element in the list the codes came in, and is `None` when they came
/// through [`argument_specs_from_str`]'s single string. `offset` is a byte offset
/// into that particular string — the element at `index`, or the whole compact string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgumentCodeError {
    /// The character at `offset` begins no known argument code.
    UnknownCode {
        /// The list element holding the code, if the codes came in as a list.
        index: Option<usize>,
        /// Byte offset into the code string.
        offset: usize,
        /// The offending character.
        code: char,
    },
    /// The code at `offset` requires delimiter/marker characters that are missing —
    /// the string ended, or whitespace stood where a parameter character must be
    /// (whitespace separates codes; it cannot be a parameter).
    TruncatedCode {
        /// The list element holding the code, if the codes came in as a list.
        index: Option<usize>,
        /// Byte offset of the code character itself.
        offset: usize,
        /// The code character whose parameters are missing.
        code: char,
    },
    /// A list element continues past its single code ([`argument_specs`] only —
    /// one code per element; in the compact string the next code simply follows).
    TrailingCode {
        /// The list element holding the code.
        index: usize,
        /// Byte offset of the first unexpected character.
        offset: usize,
        /// The first unexpected character.
        trailing: char,
    },
    /// A list element is empty or whitespace-only ([`argument_specs`] only — an
    /// empty *list* declares zero arguments; an empty *element* declares nothing).
    EmptyCode {
        /// The offending list element.
        index: usize,
    },
}

impl fmt::Display for ArgumentCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn at(f: &mut fmt::Formatter<'_>, index: Option<usize>, offset: usize) -> fmt::Result {
            match index {
                Some(index) => write!(f, "at offset {offset} of code string {index}"),
                None => write!(f, "at offset {offset}"),
            }
        }
        match self {
            ArgumentCodeError::UnknownCode { index, offset, code } => {
                write!(f, "unknown argument code ‘{code}’ ")?;
                at(f, *index, *offset)
            }
            ArgumentCodeError::TruncatedCode { index, offset, code } => {
                write!(f, "argument code ‘{code}’ ")?;
                at(f, *index, *offset)?;
                write!(f, " is missing its parameter character(s)")
            }
            ArgumentCodeError::TrailingCode { index, offset, trailing } => write!(
                f,
                "unexpected ‘{trailing}’ at offset {offset} of code string {index} \
                 (one argument code per string)"
            ),
            ArgumentCodeError::EmptyCode { index } => {
                write!(f, "code string {index} is empty (one argument code per string)")
            }
        }
    }
}

impl core::error::Error for ArgumentCodeError {}

/// Builds the argument structure of a callable from one argument code per argument.
///
/// `argument_specs(["o", "m"])` declares an optional `[…]` argument followed by a
/// mandatory one and returns one [`ArgumentSpec`] per code, in invocation order —
/// ready for [`MacroSpec::new`](super::MacroSpec::new),
/// [`SpecialsSpec::new`](super::SpecialsSpec::new) or
/// [`EnvironmentSpec::new`](super::EnvironmentSpec::new).
/// [`Package::define_macro`](crate::core::specs::Package::define_macro) spells the
/// definition and its registration in a single line.
///
/// Every element holds exactly one code, together with its parameter characters where
/// the code takes them (`["t!", "r()", "v||"]`). Whitespace around an element is
/// ignored; anything else after the code is a
/// [`TrailingCode`](ArgumentCodeError::TrailingCode) error. To write all the codes in
/// one string instead, use [`argument_specs_from_str`]; to name the arguments, use
/// [`argument_specs_named`].
///
/// # The codes
///
/// The example column spells one invocation of a macro `\m` declared with that code
/// alone. "Absent" and the diagnosed cases are explained under [When an argument is
/// not there](#when-an-argument-is-not-there).
///
/// | code | argument | example | when it is not there | parser |
/// |---|---|---|---|---|
/// | `m` or `{` | a group of the content class — `{…}` in LaTeX — or, failing that, a single expression | `\m{arg}`, `\m1` | missing mandatory argument | [`GroupArgumentParser`] |
/// | `o` or `[` | an optional `[…]` group | `\m[arg]` | absent, silently | [`OptionalGroupArgumentParser`] |
/// | `s` or `*` | an optional `*` marker | `\m*` | absent, silently | [`MarkerArgumentParser`] |
/// | `t<c>` | an optional marker made of the single character `<c>` | `t!` matches `\m!` | absent, silently | [`MarkerArgumentParser`] |
/// | `r<c1><c2>` | a mandatory group delimited by `<c1>`…`<c2>`, with no expression fallback | `r()` matches `\m(arg)` | missing mandatory argument | [`GroupArgumentParser::with_rule`] |
/// | `d<c1><c2>` | an optional group delimited by `<c1>`…`<c2>` | `d<>` matches `\m<arg>` | absent, silently | [`OptionalGroupArgumentParser`] |
/// | `v` | verbatim text between two delimiter characters, the opening one being whichever character comes next | `\m+raw %text+` | expected verbatim delimiter | [`VerbatimArgumentParser`] |
/// | `v<c1><c2>` | verbatim text between the prescribed delimiters | `v+-` matches `\m+raw %text-` | expected verbatim delimiter | [`VerbatimArgumentParser`] |
/// | `e{<chars>}` | embellishments: each character between the braces is a marker that may be followed by one expression, in any order, each marker at most once | `e{^_}` matches `\m^{a}_{b}` | absent, silently | [`EmbellishmentsArgumentParser`] |
/// | `AnyDelimited` | a mandatory group delimited by any of `{}`, `[]`, `()`, `<>` | `\m<arg>` | missing mandatory argument | [`GroupArgumentParser::any_of`] |
/// | `AnyDelimitedOptional` | the same pairs, optional | `\m(arg)` | absent, silently | [`OptionalGroupArgumentParser::any_of`] |
/// | `BracedOnly` | a mandatory group of the content class, with no expression fallback | `\m{arg}` | missing mandatory argument | [`GroupArgumentParser::with_expression_fallback`]`(false)` |
///
/// The last three are **word codes**: each one is a whole list element, so they are
/// available in this function and in [`argument_specs_named`], but not in a compact
/// string, where `AnyDelimited` reads as the unknown code `A`.
///
/// A group class is not a fixed delimiter spelling: `m` and `BracedOnly` accept
/// whatever the parsing state declares as content-group delimiters, so with `<`…`>`
/// declared there, `\m<arg>` satisfies them too. The pairs of `r`, `d` and the
/// `AnyDelimited` codes are the opposite: those delimiters are created for that one
/// argument and are recognized nowhere else, so `(` remains an ordinary character
/// outside it.
///
/// # When an argument is not there
///
/// A mandatory code that finds nothing it accepts reports a
/// [`MissingMandatoryArgument`](crate::core::constructs::MissingMandatoryArgument),
/// and the `v` codes report an
/// [`ExpectedVerbatimDelimiter`](crate::core::constructs::ExpectedVerbatimDelimiter)
/// — for a bare `v` only at the end of the input, since any character serves as its
/// opening delimiter. Both follow the parse's recovery setting: a tolerant parse
/// records the diagnostic, reports the argument absent and consumes nothing, while a
/// strict parse stops with an error.
///
/// An optional code that finds nothing simply reports the argument absent: no
/// diagnostic, nothing consumed, and the source that follows is parsed as ordinary
/// content. Absent arguments still occupy their position in the invocation's
/// [`ParsedArguments`](crate::core::node::ParsedArguments), so argument numbering
/// never shifts; ask
/// [`ParsedArgument::is_provided`](crate::core::node::ParsedArgument::is_provided)
/// which ones were there.
///
/// # Writing the parameterized codes
///
/// The parameter characters follow their code letter immediately, in the same
/// element, and whitespace is never one of them: `t<c>` takes the marker character,
/// `r<c1><c2>` and `d<c1><c2>` take the opening and the closing delimiter in that
/// order, `v<c1><c2>` the two verbatim delimiters, and `e{<chars>}` takes a brace pair
/// enclosing at least one marker character.
///
/// | write | to declare |
/// |---|---|
/// | `["t*"]` | an optional `*` marker (the same as `["s"]`) |
/// | `["r()"]` | a mandatory `(…)` argument |
/// | `["d<>"]` | an optional `<…>` argument |
/// | `["v\|\|"]` | verbatim text between two `\|` characters |
/// | `["e{^_'}"]` | embellishments over the three markers `^`, `_` and `'` |
///
/// A missing parameter character is a
/// [`TruncatedCode`](ArgumentCodeError::TruncatedCode) error, so `["r("]`,
/// `["t"]` and `["e{}"]` all fail. Only single-character markers can be written this
/// way: for a multi-character one, build the
/// [`MarkerArgumentParser`] or [`EmbellishmentsArgumentParser`] directly.
///
/// In this list form the two `v` shapes need no disambiguation — `["v"]` is the
/// auto-matched form and `["v||"]` the prescribed one — whereas the compact string
/// has a rule for it, documented on [`argument_specs_from_str`].
///
/// # The single-expression fallback of `m`
///
/// `m` keeps TeX's fallback deliberately: when no group opens, the argument is the
/// next single expression, so `\frac12` reads as two one-character arguments. The
/// trap is that a *missing* group is then not diagnosed either — the argument
/// silently takes whatever sibling content follows. Where that matters, such as for
/// machine-written or configuration-like arguments, use the word code `BracedOnly`:
/// the same mandatory group with the fallback off, which accepts a real group or
/// nothing.
///
/// # Errors
///
/// Returns an [`ArgumentCodeError`] naming the offending element and the byte offset
/// within it: an unknown code character, a code whose parameter characters are
/// missing, a second code in the same element, or an empty element. Nothing is built
/// when any code fails, and no code has a silent fallback — a definition either
/// declares exactly what it says or it fails here.
///
/// # Examples
///
/// ```
/// use techy::core::{Language, ParsingState};
/// use techy::error::Recovery;
/// use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
/// use techy::core::specs::Package;
///
/// let mut package = Package::new("mydefs");
/// package.insert(
///     CallableType::Macro,
///     "includegraphics",
///     MacroSpec::new(argument_specs(["o", "{"]).unwrap()),
/// );
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
/// );
///
/// let result = language.parse(r"\includegraphics[width=5cm]{fig.png}").unwrap();
/// let node = result.tree.root().child(0).unwrap();
/// assert!(node.arguments().unwrap().get(0).unwrap().is_provided());
/// assert_eq!(
///     node.argument_content_nodes(1).unwrap().iter().next().unwrap().chars(),
///     Some("fig.png"),
/// );
/// ```
///
/// The specs this function builds carry no argument names and no per-argument
/// parsing-state changes; [`argument_specs_named`] adds the names, and
/// [`ArgumentSpec`]'s own builders add a state change (a `\text`-style argument that
/// leaves math mode, for instance). Building an [`ArgumentSpec`] around a parser
/// value directly is always available — the codes are a convenience, not a
/// requirement.
///
/// The function is generic over the language family (`LLL`, [`LatexlikeLang`]), which
/// is ordinarily inferred from the spec that receives the result: the mandatory and
/// verbatim group classes come from that family's role constructors
/// ([`content_group`](LatexlikeGroupType::content_group),
/// [`verbatim_group`](LatexlikeGroupType::verbatim_group)).
pub fn argument_specs<LLL, I>(
    codes: I,
) -> Result<Vec<Arc<ArgumentSpec<LLL>>>, ArgumentCodeError>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    codes
        .into_iter()
        .enumerate()
        .map(|(index, code)| {
            let parser = scan_element(code.as_ref(), index)?;
            Ok(Arc::new(ArgumentSpec::new_unnamed(parser)))
        })
        .collect()
}

/// Builds the argument structure from `(code, name)` pairs, naming every argument.
///
/// `argument_specs_named([("o", "greeting"), ("m", "name")])` builds exactly what
/// [`argument_specs`] builds from `["o", "m"]`, with each [`ArgumentSpec`] carrying a
/// name ([`ArgumentSpec::new`]). The arguments can then be read back by name
/// ([`argument_content_nodes_named`](crate::core::node::NodeRef::argument_content_nodes_named)
/// and its siblings), which is the more robust access path: its error contract
/// distinguishes a misspelled name from an argument that was merely absent.
///
/// The codes, the word codes and the error cases are [`argument_specs`]'s; the
/// `index` of an [`ArgumentCodeError`] is the position of the offending pair.
pub fn argument_specs_named<LLL, I, C, N>(
    codes: I,
) -> Result<Vec<Arc<ArgumentSpec<LLL>>>, ArgumentCodeError>
where
    LLL: LatexlikeLang,
    ArgumentExt<LLL>: Default,
    I: IntoIterator<Item = (C, N)>,
    C: AsRef<str>,
    N: Into<Box<str>>,
{
    codes
        .into_iter()
        .enumerate()
        .map(|(index, (code, name))| {
            let parser = scan_element(code.as_ref(), index)?;
            Ok(Arc::new(ArgumentSpec::new(parser, name)))
        })
        .collect()
}

/// Scan one whole list element (see [`argument_specs`]): a word code claims the
/// trimmed element; otherwise exactly one character code with its parameters,
/// surrounded by optional whitespace.
fn scan_element<LLL: LatexlikeLang>(
    code: &str,
    index: usize,
) -> Result<Arc<dyn ArgumentParser<LLL>>, ArgumentCodeError>
where
    ArgumentExt<LLL>: Default,
{
    // The word codes claim whole (trimmed) list elements before the
    // character-code scan sees them.
    if let Some(parser) = scan_word_code(code.trim()) {
        return Ok(parser);
    }
    let mut chars = code.char_indices().peekable();
    let parser = scan_code(&mut chars, Some(index))?
        .ok_or(ArgumentCodeError::EmptyCode { index })?;
    if let Some((offset, trailing)) = chars.find(|(_, c)| !c.is_whitespace()) {
        return Err(ArgumentCodeError::TrailingCode { index, offset, trailing });
    }
    Ok(parser)
}

/// Resolve a whole-element word code (`AnyDelimited` / `AnyDelimitedOptional` /
/// `BracedOnly`) — list-form only (see [`argument_specs`]).
fn scan_word_code<LLL: LatexlikeLang>(code: &str) -> Option<Arc<dyn ArgumentParser<LLL>>>
where
    ArgumentExt<LLL>: Default,
{
    Some(match code {
        "AnyDelimited" => Arc::new(GroupArgumentParser::any_of(any_delimited_rules())),
        "AnyDelimitedOptional" => Arc::new(
            OptionalGroupArgumentParser::any_of(any_delimited_rules())
                .with_unwrap_lone_group(LLL::GroupTypeId::content_group()),
        ),
        "BracedOnly" => Arc::new(
            GroupArgumentParser::new(LLL::GroupTypeId::content_group())
                .with_expression_fallback(false),
        ),
        _ => return None,
    })
}

/// The default delimiter alternatives of the `AnyDelimited` codes (pylatexenc's
/// list): `{}`, `[]`, `()`, `<>`, minted as content-class rules per use.
fn any_delimited_rules<LLL: LatexlikeLang>() -> Vec<Arc<GroupRule<LLL>>> {
    [('{', '}'), ('[', ']'), ('(', ')'), ('<', '>')]
        .into_iter()
        .map(|(open, close)| minted_rule(open, close))
        .collect()
}

/// Builds the argument structure from one compact string holding every code (`"om"`).
///
/// This is the form pylatexenc's definitions are written in, accepted verbatim so
/// that such a definition can be ported without rewriting its argument string;
/// [`argument_specs`], one code per argument, is the form to prefer for new
/// definitions. The codes are the same ones, minus the word codes `AnyDelimited`,
/// `AnyDelimitedOptional` and `BracedOnly`: those are whole list elements, and here
/// their first character reads as an unknown code. The code table is on
/// [`argument_specs`].
///
/// Codes may be separated by whitespace (`"mo s t! r() d<> v"`), but the parameter
/// characters of `t`, `r`, `d`, `v` and `e` must follow their code immediately.
///
/// **The `v` rule.** A `v` followed directly by a non-whitespace character reads that
/// character and the one after it as its prescribed delimiters (`"v||"`), so a bare
/// auto-delimited `v` must stand last or be separated from the next code by
/// whitespace: `"v {"` is a verbatim argument followed by a mandatory one, whereas
/// `"v{"` is a `v` whose second delimiter character is missing.
///
/// # Errors
///
/// Returns an [`ArgumentCodeError`] with `index` set to `None` and `offset` a byte
/// offset into the whole string: [`UnknownCode`](ArgumentCodeError::UnknownCode) for
/// a character that begins no code, [`TruncatedCode`](ArgumentCodeError::TruncatedCode)
/// for missing parameter characters. The two remaining cases belong to the list form
/// and cannot arise here. An empty or whitespace-only string is not an error: it
/// declares no arguments.
pub fn argument_specs_from_str<LLL: LatexlikeLang>(
    codes: &str,
) -> Result<Vec<Arc<ArgumentSpec<LLL>>>, ArgumentCodeError>
where
    ArgumentExt<LLL>: Default,
{
    let mut specs = Vec::new();
    let mut chars = codes.char_indices().peekable();
    while let Some(parser) = scan_code(&mut chars, None)? {
        specs.push(Arc::new(ArgumentSpec::new_unnamed(parser)));
    }
    Ok(specs)
}

/// Scan one code (with its parameter characters) off `chars`, skipping leading
/// whitespace; `Ok(None)` when the string is exhausted first. `index` is the list
/// coordinate threaded into errors (`None` in the compact-string form).
fn scan_code<LLL: LatexlikeLang>(
    chars: &mut core::iter::Peekable<core::str::CharIndices<'_>>,
    index: Option<usize>,
) -> Result<Option<Arc<dyn ArgumentParser<LLL>>>, ArgumentCodeError>
where
    ArgumentExt<LLL>: Default,
{
    let (offset, code) = loop {
        match chars.next() {
            Some((_, c)) if c.is_whitespace() => continue,
            Some(pair) => break pair,
            None => return Ok(None),
        }
    };
    let parameter = |chars: &mut core::iter::Peekable<core::str::CharIndices>| {
        match chars.next() {
            Some((_, c)) if !c.is_whitespace() => Ok(c),
            _ => Err(ArgumentCodeError::TruncatedCode { index, offset, code }),
        }
    };
    let parser: Arc<dyn ArgumentParser<LLL>> = match code {
        'm' | '{' => Arc::new(GroupArgumentParser::new(LLL::GroupTypeId::content_group())),
        'o' | '[' => Arc::new(optional_group_parser('[', ']')),
        's' | '*' => Arc::new(MarkerArgumentParser::new("*")),
        't' => Arc::new(MarkerArgumentParser::new(String::from(parameter(&mut *chars)?))),
        'r' => {
            let open = parameter(&mut *chars)?;
            let close = parameter(&mut *chars)?;
            Arc::new(GroupArgumentParser::with_rule(minted_rule(open, close)))
        }
        'd' => {
            let open = parameter(&mut *chars)?;
            let close = parameter(&mut *chars)?;
            Arc::new(optional_group_parser(open, close))
        }
        'e' => {
            // `e{<chars>}`: the braces must follow immediately; one marker per
            // character between them, at least one, no whitespace (whitespace
            // separates codes; it cannot be a parameter).
            if parameter(&mut *chars)? != '{' {
                return Err(ArgumentCodeError::TruncatedCode { index, offset, code });
            }
            let mut markers: Vec<String> = Vec::new();
            loop {
                match chars.next() {
                    Some((_, '}')) => break,
                    Some((_, c)) if !c.is_whitespace() => markers.push(String::from(c)),
                    _ => {
                        return Err(ArgumentCodeError::TruncatedCode { index, offset, code })
                    }
                }
            }
            if markers.is_empty() {
                return Err(ArgumentCodeError::TruncatedCode { index, offset, code });
            }
            Arc::new(EmbellishmentsArgumentParser::new(markers))
        }
        'v' => match chars.peek() {
            Some((_, c)) if !c.is_whitespace() => {
                let open = parameter(&mut *chars)?;
                let close = parameter(&mut *chars)?;
                Arc::new(
                    VerbatimArgumentParser::new(LLL::GroupTypeId::verbatim_group())
                        .with_delimiters(open, close),
                )
            }
            _ => Arc::new(VerbatimArgumentParser::new(LLL::GroupTypeId::verbatim_group())),
        },
        _ => return Err(ArgumentCodeError::UnknownCode { index, offset, code }),
    };
    Ok(Some(parser))
}

/// The minted per-use content-class rule of the `o`/`r`/`d` codes.
fn minted_rule<LLL: LatexlikeLang>(open: char, close: char) -> Arc<GroupRule<LLL>> {
    Arc::new(GroupRule {
        group_type: LLL::GroupTypeId::content_group(),
        open: String::from(open),
        close: String::from(close),
    })
}

/// The optional-group shape shared by `o` and `d`: minted delimiters, with the
/// protective lone `{…}` group unwrapping (the parse-time resolution of pylatexenc's
/// `unwrap_double_group` accessor default).
fn optional_group_parser<LLL: LatexlikeLang>(
    open: char,
    close: char,
) -> OptionalGroupArgumentParser<LLL> {
    OptionalGroupArgumentParser::new(minted_rule(open, close))
        .with_unwrap_lone_group(LLL::GroupTypeId::content_group())
}

#[cfg(test)]
mod tests {
    use super::super::{
        CallableType, GroupType, Latexlike, LatexlikeDriver, MacroSpec, MathGroupForm,
    };
    use super::*;
    use crate::engine::{Language, ParseResult};
    use crate::error::Recovery;
    use crate::latexlike::check_latexlike_tree_invariants;
    use crate::node::NodeRef;
    use crate::scopes::Package;
    use crate::state::ParsingState;
    use alloc::format;
    use alloc::string::ToString;

    // --- the code grammar ---------------------------------------------------------------

    /// The parser type each code resolved to, read off the spec's Debug rendering
    /// (each standard parser type names itself there).
    fn parser_debug(spec: &ArgumentSpec<Latexlike>) -> String {
        format!("{:?}", spec.parser)
    }

    #[test]
    fn empty_lists_and_whitespace_only_compact_strings_declare_no_arguments() {
        assert!(argument_specs::<Latexlike, _>(Vec::<&str>::new()).unwrap().is_empty());
        assert!(argument_specs_from_str::<Latexlike>("").unwrap().is_empty());
        assert!(argument_specs_from_str::<Latexlike>("  \t ").unwrap().is_empty());
    }

    #[test]
    fn the_codes_resolve_to_their_parsers() {
        let specs = argument_specs(["m", "o", "s", "t!", "r()", "d<>", "v"]).unwrap();
        assert_eq!(specs.len(), 7);
        assert!(parser_debug(&specs[0]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[0]).contains("group_type: Content"));
        assert!(parser_debug(&specs[1]).contains("OptionalGroupArgumentParser"));
        assert!(parser_debug(&specs[2]).contains("marker: \"*\""));
        assert!(parser_debug(&specs[3]).contains("marker: \"!\""));
        assert!(parser_debug(&specs[4]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[4]).contains("rule"));
        assert!(parser_debug(&specs[5]).contains("OptionalGroupArgumentParser"));
        assert!(parser_debug(&specs[6]).contains("VerbatimArgumentParser"));
        // None of the factory's specs carry names or state deltas.
        assert!(specs.iter().all(|spec| spec.name.is_none()));
        assert!(specs.iter().all(|spec| spec.parsing_state_delta.is_none()));
    }

    #[test]
    fn the_shorthand_aliases_match_their_letters() {
        let letters = argument_specs(["m", "o", "s"]).unwrap();
        let aliases = argument_specs(["{", "[", "*"]).unwrap();
        for (letter, alias) in letters.iter().zip(&aliases) {
            assert_eq!(parser_debug(letter), parser_debug(alias));
        }
    }

    #[test]
    fn the_compact_string_matches_the_list_form() {
        let compact = argument_specs_from_str("mo s t! r() d<> v").unwrap();
        let listed = argument_specs(["m", "o", "s", "t!", "r()", "d<>", "v"]).unwrap();
        assert_eq!(compact.len(), listed.len());
        for (c, l) in compact.iter().zip(&listed) {
            assert_eq!(parser_debug(c), parser_debug(l));
        }
    }

    #[test]
    fn list_elements_hold_one_code_and_tolerate_surrounding_whitespace() {
        let specs = argument_specs([" m ", "\tt!", "v "]).unwrap();
        assert_eq!(specs.len(), 3);
        assert!(parser_debug(&specs[0]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[1]).contains("marker: \"!\""));
        // Trailing whitespace does not turn `v` into the prescribed-delimiter form.
        assert!(parser_debug(&specs[2]).contains("delimiters: None"));
    }

    #[test]
    fn v_takes_delimiters_exactly_when_followed_directly() {
        let auto = argument_specs_from_str("v").unwrap();
        assert!(parser_debug(&auto[0]).contains("delimiters: None"));

        let fixed = argument_specs_from_str("v||").unwrap();
        assert!(parser_debug(&fixed[0]).contains("delimiters: Some(('|', '|'))"));

        // Whitespace separates: a bare `v` before another code.
        let separated = argument_specs_from_str("v {").unwrap();
        assert_eq!(separated.len(), 2);
        assert!(parser_debug(&separated[0]).contains("delimiters: None"));
        assert!(parser_debug(&separated[1]).contains("GroupArgumentParser"));

        // Directly followed means the delimiters must both be there.
        assert_eq!(
            argument_specs_from_str::<Latexlike>("v{").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 0, code: 'v' }
        );

        // In list form there is nothing to disambiguate.
        let auto = argument_specs(["v"]).unwrap();
        assert!(parser_debug(&auto[0]).contains("delimiters: None"));
        let fixed = argument_specs(["v||"]).unwrap();
        assert!(parser_debug(&fixed[0]).contains("delimiters: Some(('|', '|'))"));
    }

    #[test]
    fn malformed_compact_strings_report_offset_and_code() {
        assert_eq!(
            argument_specs_from_str::<Latexlike>("m x").unwrap_err(),
            ArgumentCodeError::UnknownCode { index: None, offset: 2, code: 'x' }
        );
        assert_eq!(
            argument_specs_from_str::<Latexlike>("t").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 0, code: 't' }
        );
        // Whitespace cannot be a parameter character.
        assert_eq!(
            argument_specs_from_str::<Latexlike>("t !").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 0, code: 't' }
        );
        assert_eq!(
            argument_specs_from_str::<Latexlike>("or(").unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: None, offset: 1, code: 'r' }
        );
        assert_eq!(
            argument_specs_from_str::<Latexlike>("x").unwrap_err().to_string(),
            "unknown argument code ‘x’ at offset 0"
        );
        assert_eq!(
            argument_specs_from_str::<Latexlike>("d<").unwrap_err().to_string(),
            "argument code ‘d’ at offset 0 is missing its parameter character(s)"
        );
    }

    #[test]
    fn malformed_code_lists_report_index_offset_and_code() {
        assert_eq!(
            argument_specs::<Latexlike, _>(["m", "x"]).unwrap_err(),
            ArgumentCodeError::UnknownCode { index: Some(1), offset: 0, code: 'x' }
        );
        assert_eq!(
            argument_specs::<Latexlike, _>(["o", "r("]).unwrap_err(),
            ArgumentCodeError::TruncatedCode { index: Some(1), offset: 0, code: 'r' }
        );
        // One code per element: a second code is trailing, not concatenated.
        assert_eq!(
            argument_specs::<Latexlike, _>(["mo"]).unwrap_err(),
            ArgumentCodeError::TrailingCode { index: 0, offset: 1, trailing: 'o' }
        );
        assert_eq!(
            argument_specs::<Latexlike, _>(["m", "o m"]).unwrap_err(),
            ArgumentCodeError::TrailingCode { index: 1, offset: 2, trailing: 'm' }
        );
        // Empty elements are bugs, not zero-argument declarations.
        assert_eq!(
            argument_specs::<Latexlike, _>(["m", ""]).unwrap_err(),
            ArgumentCodeError::EmptyCode { index: 1 }
        );
        assert_eq!(
            argument_specs::<Latexlike, _>([" \t"]).unwrap_err(),
            ArgumentCodeError::EmptyCode { index: 0 }
        );
        // Display strings name the list element.
        assert_eq!(
            argument_specs::<Latexlike, _>(["x"]).unwrap_err().to_string(),
            "unknown argument code ‘x’ at offset 0 of code string 0"
        );
        assert_eq!(
            argument_specs::<Latexlike, _>(["r<"]).unwrap_err().to_string(),
            "argument code ‘r’ at offset 0 of code string 0 is missing its parameter \
             character(s)"
        );
        assert_eq!(
            argument_specs::<Latexlike, _>(["mo"]).unwrap_err().to_string(),
            "unexpected ‘o’ at offset 1 of code string 0 (one argument code per string)"
        );
        assert_eq!(
            argument_specs::<Latexlike, _>([""]).unwrap_err().to_string(),
            "code string 0 is empty (one argument code per string)"
        );
    }

    // --- end-to-end through the preset (the stdarg port slice) ---------------------------

    /// A language defining `\m` with the given argument codes (a compact string, or a
    /// single word code like `AnyDelimited`, which only the list form accepts).
    fn language(recovery: Recovery, codes: &str) -> Language<Latexlike> {
        let specs = argument_specs_from_str(codes)
            .or_else(|_| argument_specs([codes]))
            .unwrap();
        let mut package = Package::new("factory-tests");
        package.insert(CallableType::Macro, "m", Arc::new(MacroSpec::new(specs)));
        Language::new(
            LatexlikeDriver::new(recovery),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        )
    }

    fn parse_ok(codes: &str, input: &str) -> ParseResult<Latexlike> {
        let result = language(Recovery::Strict, codes).parse(input).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn macro_node(result: &ParseResult<Latexlike>) -> NodeRef<'_, Latexlike> {
        let node = result.tree.root().child(0).expect("the macro node");
        assert_eq!(node.macro_name(), Some("m"));
        node
    }

    fn content_chars(node: NodeRef<'_, Latexlike>, i: usize) -> String {
        node.argument_content_nodes(i)
            .expect("provided argument")
            .iter().map(|child| child.chars().unwrap_or("<non-chars>").to_string())
            .collect()
    }

    #[test]
    fn m_code_parses_a_brace_group_with_the_expression_fallback() {
        // pylatexenc test_arg_m_0: content = the group's children, spans exact.
        let result = parse_ok("m", r"\m{mandatory argument} (more stuff)");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..22);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("mandatory argument"));
        assert_eq!(content[0].span().range(), 3..21);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" (more stuff)"));

        // The single-expression fallback.
        let result = parse_ok("mm", r"\m 1 2");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "1");
        assert_eq!(content_chars(m, 1), "2");
    }

    #[test]
    fn m_code_keeps_a_leading_comment_as_region_noise() {
        // pylatexenc test_arg_m_precomment: the comment always stays in the region
        // (techy keeps it as a node; `return_full_node_list` dissolved into regions),
        // and the content designation excludes it.
        let result = parse_ok("m", "\\m %comment here\n{mandatory argument}");
        let m = macro_node(&result);
        let region: Vec<_> = m.argument_nodes(0).unwrap().iter().collect();
        assert_eq!(region.len(), 2);
        assert_eq!(
            region[0].comment().map(|data| data.content.resolve(region[0].source())),
            Some("comment here")
        );
        assert_eq!(content_chars(m, 0), "mandatory argument");
    }

    #[test]
    fn o_code_parses_an_optional_bracket_group() {
        let result = parse_ok("om", r"\m[opt]{x}");
        let m = macro_node(&result);
        assert!(m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(content_chars(m, 0), "opt");

        // Absent: silent, nothing consumed — `[` only ever opens right there.
        let result = parse_ok("om", r"\m{x} [not an option]");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(content_chars(m, 1), "x");
    }

    #[test]
    fn o_code_unwraps_a_lone_protective_brace_group() {
        // `[{…}]` protecting a literal `]`: the content designation resolves to the
        // inner group's children (pylatexenc's unwrap_double_group, at parse time).
        let result = parse_ok("o", r"\m[{a]b}]");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "a]b");
    }

    #[test]
    fn s_and_t_codes_parse_optional_markers() {
        // pylatexenc test_arg_star_0/_1 (the marker's pre-space becomes noise).
        let result = parse_ok("sm", r"\m*{x}");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "*");

        let result = parse_ok("sm", r"\m {x}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());

        let result = parse_ok("t!m", r"\m!{x}");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "!");
    }

    #[test]
    fn r_code_parses_a_required_delimited_group() {
        let result = parse_ok("r()", r"\m(a,b) x");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..7);
        assert_eq!(content_chars(m, 0), "a,b");
        let group = m.child(0).unwrap();
        assert_eq!(group.group_delimiters(), Some(("(", ")")));
        assert_eq!(group.group_type(), Some(GroupType::Content));

        // Nested pairs balance; braces protect the closer.
        let result = parse_ok("r()", r"\m(a(b)c)");
        assert_eq!(content_chars(macro_node(&result), 0), "a<non-chars>c");
        let result = parse_ok("r()", r"\m(a{b)c}d)");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content[1].group_delimiters(), Some(("{", "}")));
    }

    #[test]
    fn r_code_missing_is_diagnosed_with_no_expression_fallback() {
        let err = language(Recovery::Strict, "r()").parse(r"\m x").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        let result = language(Recovery::Tolerant, "r()").parse(r"\m x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        // `x` was not swallowed as an expression: it stays sibling content (the
        // blank is the trigger's own syntactic post-space).
        assert_eq!(m.post_space(), Some(" "));
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("x"));
    }

    #[test]
    fn d_code_parses_an_optional_delimited_group() {
        let result = parse_ok("d<>m", r"\m<opt>{x}");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "opt");

        let result = parse_ok("d<>m", r"\m{x}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
    }

    #[test]
    fn e_code_and_word_codes_resolve_to_their_parsers() {
        let specs = argument_specs(["e{^_}", "AnyDelimited", " AnyDelimitedOptional "]).unwrap();
        assert!(parser_debug(&specs[0]).contains("EmbellishmentsArgumentParser"));
        assert!(parser_debug(&specs[0]).contains("markers: [\"^\", \"_\"]"));
        assert!(parser_debug(&specs[1]).contains("GroupArgumentParser"));
        assert!(parser_debug(&specs[1]).contains("rules"));
        assert!(parser_debug(&specs[2]).contains("OptionalGroupArgumentParser"));

        // `e{…}` also reads in the compact form; the word codes are list-form only.
        let compact = argument_specs_from_str("me{^_}o").unwrap();
        assert_eq!(compact.len(), 3);
        assert!(parser_debug(&compact[1]).contains("EmbellishmentsArgumentParser"));
        assert_eq!(
            argument_specs_from_str::<Latexlike>("AnyDelimited").unwrap_err(),
            ArgumentCodeError::UnknownCode { index: None, offset: 0, code: 'A' }
        );

        // Malformed `e` codes: missing braces, empty marker set, whitespace inside,
        // unterminated set.
        for bad in ["e", "ex", "e{}", "e{^", "e{^ _}"] {
            assert_eq!(
                argument_specs::<Latexlike, _>([bad]).unwrap_err(),
                ArgumentCodeError::TruncatedCode { index: Some(0), offset: 0, code: 'e' },
                "code {bad:?}"
            );
        }
    }

    #[test]
    fn e_code_parses_embellishments_span_exactly() {
        // pylatexenc test_arg_embelishments_1 (`e{_^`}` on `^{test}_x{more stuff}`,
        // shifted by the `\m` trigger): one wrapper group per matched pair, the
        // marker as its opening delimiter, the following `{more stuff}` untouched.
        let result = parse_ok("e{_^`}", "\\m^{test}_x{more stuff} stuff");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..11);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);

        let sup = content[0];
        assert_eq!(sup.group_delimiters(), Some(("^", "")));
        assert_eq!(sup.group_type(), None); // classless synthesized wrapper
        assert_eq!(sup.span().range(), 2..9);
        let sup_value = sup.child(0).unwrap();
        assert_eq!(sup_value.group_delimiters(), Some(("{", "}")));
        assert_eq!(sup_value.span().range(), 3..9);
        assert_eq!(sup_value.child(0).unwrap().chars(), Some("test"));

        let sub = content[1];
        assert_eq!(sub.group_delimiters(), Some(("_", "")));
        assert_eq!(sub.span().range(), 9..11);
        assert_eq!(sub.child(0).unwrap().chars(), Some("x"));

        // The next group belongs to the enclosing content.
        assert!(result.tree.root().child(1).unwrap().is_group());

        // The extraction helper reads the run by marker.
        let fields = crate::extract::split_embellishments_drop_annotations(
            m.argument_content_nodes(0).unwrap(),
        )
        .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(
            crate::extract::content_as_chars(fields.get("^").unwrap().value_content().unwrap())
                .unwrap(),
            "test"
        );
        assert_eq!(
            crate::extract::content_as_chars(fields.get("_").unwrap().value().unwrap())
                .unwrap(),
            "x"
        );
        assert!(fields.get("`").is_none());
    }

    #[test]
    fn e_code_embellishments_match_in_any_order_each_at_most_once() {
        // pylatexenc test_arg_embelishments_2: no marker at all — silently absent.
        let result = parse_ok("e{_^}", r"\m more stuff");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());

        // Any source order.
        let result = parse_ok("e{^_}", r"\m_{b}^{a}");
        let m = macro_node(&result);
        let fields =
            crate::extract::split_embellishments_drop_annotations(m.argument_content_nodes(0).unwrap())
                .unwrap();
        let keys: Vec<_> = fields.iter().map(|entry| entry.key().to_string()).collect();
        assert_eq!(keys, ["_", "^"]);

        // Each marker at most once: the second `^{b}` stays enclosing content.
        let result = parse_ok("e{^_}", r"\m^{a}^{b}");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..6);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("^"));
    }

    #[test]
    fn e_code_allows_noise_before_markers_and_whitespace_before_arguments() {
        // Noise ahead of each marker: whitespace and comments become region noise.
        let result = parse_ok("e{^_}", "\\m %pre\n^{a} _{b}!");
        let m = macro_node(&result);
        let region: Vec<_> = m.argument_nodes(0).unwrap().iter().collect();
        assert!(region.iter().any(|node| node.comment().is_some()));
        let fields =
            crate::extract::split_embellishments_drop_annotations(m.argument_content_nodes(0).unwrap())
                .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("!"));

        // Plain whitespace between a marker and its expression is allowed (pylatexenc
        // `allow_pre_space` parity; TeX's `x^ 2`), staged inside the wrapper…
        let result = parse_ok("e{^_}", r"\m^ {a}");
        let m = macro_node(&result);
        let wrapper = m.argument_content_nodes(0).unwrap().first().unwrap();
        assert_eq!(wrapper.group_delimiters(), Some(("^", "")));
        assert_eq!(wrapper.child_count(), 2);
        assert_eq!(wrapper.child(0).unwrap().chars(), Some(" "));
        // …and the extraction values stay noise-free regardless.
        let fields =
            crate::extract::split_embellishments_drop_annotations(m.argument_content_nodes(0).unwrap())
                .unwrap();
        let sup = fields.get("^").unwrap();
        assert_eq!(sup.value().unwrap().len(), 1);
        assert_eq!(
            crate::extract::content_as_chars(sup.value_content().unwrap()).unwrap(),
            "a"
        );

        // A comment after the marker is not tolerated: the marker unmatches whole
        // (silently — `^%c…{a}` is ordinary enclosing content).
        let result = parse_ok("e{^_}", "\\m^%c\n{a}");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("^"));

        // A committed pair before the violation stays: `_{b}` parses, the dangling
        // `^` before a comment ends the run.
        let result = parse_ok("e{^_}", "\\m_{b}^%c\n{a}");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].group_delimiters(), Some(("_", "")));
    }

    #[test]
    fn any_delimited_code_takes_any_default_pair_and_narrows_the_contents() {
        // pylatexenc test_arg_any_delimited_angleb.
        let result = parse_ok("AnyDelimited", r"\m<delimited>more stuff");
        let m = macro_node(&result);
        assert_eq!(m.span().range(), 0..13);
        let group = m.child(0).unwrap();
        assert_eq!(group.group_delimiters(), Some(("<", ">")));
        assert_eq!(group.group_type(), Some(GroupType::Content));
        assert_eq!(content_chars(m, 0), "delimited");
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("more stuff"));

        // Braces (a default pair and the base rule alike) still match…
        let result = parse_ok("AnyDelimited", r"\m{x}");
        assert_eq!(content_chars(macro_node(&result), 0), "x");

        // pylatexenc test_multidelim_sg: inside `<…>`, braces protect a stray `>`…
        let result = parse_ok("AnyDelimited", r"\m<Hello {there>}>");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0].chars(), Some("Hello "));
        assert_eq!(content[1].group_delimiters(), Some(("{", "}")));
        assert_eq!(content[1].child(0).unwrap().chars(), Some("there>"));

        // …and test_multidelim_sg2: the matched pair nests, the unmatched `]` is a
        // plain character (the contents keep only the encountered pair).
        let result = parse_ok("AnyDelimited", r"\m<Hello <there]>>");
        let m = macro_node(&result);
        let content: Vec<_> = m.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content.len(), 2);
        assert_eq!(content[1].group_delimiters(), Some(("<", ">")));
        assert_eq!(content[1].child(0).unwrap().chars(), Some("there]"));
    }

    #[test]
    fn any_delimited_mandatory_vs_optional() {
        // pylatexenc test_multidelim_mandatory: no pair opens — diagnosed.
        let err = language(Recovery::Strict, "AnyDelimited").parse(r"\m x").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        // pylatexenc test_multidelim_optional: absent is silent.
        let result = parse_ok("AnyDelimitedOptional", r"\m juice");
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());

        // The optional flavor unwraps a lone protective brace group, like `o`.
        let result = parse_ok("AnyDelimitedOptional", r"\m[{a]b}]");
        assert_eq!(content_chars(macro_node(&result), 0), "a]b");
    }

    #[test]
    fn argument_specs_named_builds_named_specs() {
        let specs = argument_specs_named::<Latexlike, _, _, _>([
            ("o", "greeting"),
            ("m", "name"),
        ])
        .unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].name.as_deref(), Some("greeting"));
        assert_eq!(specs[1].name.as_deref(), Some("name"));
        assert!(parser_debug(&specs[0]).contains("OptionalGroupArgumentParser"));
        assert!(parser_debug(&specs[1]).contains("GroupArgumentParser"));

        // Word codes participate; the error coordinates name the pair index.
        let specs =
            argument_specs_named::<Latexlike, _, _, _>([("BracedOnly", "payload")]).unwrap();
        assert_eq!(specs[0].name.as_deref(), Some("payload"));
        assert_eq!(
            argument_specs_named::<Latexlike, _, _, _>([("m", "a"), ("x", "b")])
                .unwrap_err(),
            ArgumentCodeError::UnknownCode { index: Some(1), offset: 0, code: 'x' }
        );
    }

    #[test]
    fn named_specs_feed_the_by_name_accessors() {
        let mut package: Package<Latexlike> = Package::new("factory-tests");
        package.insert(
            CallableType::Macro,
            "m",
            Arc::new(MacroSpec::new(
                argument_specs_named([("o", "greeting"), ("m", "name")]).unwrap(),
            )),
        );
        let language = Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            crate::state::ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );
        let result = language.parse(r"\m{world}").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let m = result.tree.root().child(0).unwrap();
        assert!(matches!(m.argument_nodes_named("greeting"), Ok(None))); // absent
        let name: String = m
            .argument_content_nodes_named("name")
            .unwrap()
            .unwrap()
            .iter()
            .map(|node| node.chars().unwrap_or("").to_string())
            .collect();
        assert_eq!(name, "world");
    }

    #[test]
    fn braced_only_code_takes_a_content_group_with_no_fallback() {
        // The group parses exactly like `m`…
        let result = parse_ok("BracedOnly", r"\m{arg} rest");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "arg");

        // …but a bare expression is NOT swallowed (no fallback): strict diagnoses
        // the missing mandatory argument; tolerant leaves `x` as sibling content.
        let err = language(Recovery::Strict, "BracedOnly").parse(r"\m x").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");
        let result = language(Recovery::Tolerant, "BracedOnly").parse(r"\m x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let m = macro_node(&result);
        assert!(!m.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("x"));

        // Word codes are list-form only: a compact string reads `B` as unknown.
        assert_eq!(
            argument_specs_from_str::<Latexlike>("BracedOnly").unwrap_err(),
            ArgumentCodeError::UnknownCode { index: None, offset: 0, code: 'B' }
        );
    }

    #[test]
    fn braced_only_accepts_any_content_class_group() {
        // "Braced" names the content *class*, not literal `{}`: with `«…»` declared
        // as a content pair in the parsing state, a `«…»` group satisfies it.
        use super::super::default_token_rules;
        use crate::state::{GroupOverrides, ParsingState, ParsingStateDelta, TokenRulesOverrides};

        let mut package = Package::new("factory-tests");
        package.insert(
            CallableType::Macro,
            "m",
            Arc::new(MacroSpec::new(argument_specs(["BracedOnly"]).unwrap())),
        );
        let mut groups = default_token_rules::<Latexlike>().groups.rules;
        groups.push(Arc::new(GroupRule {
            group_type: GroupType::Content,
            open: "«".into(),
            close: "»".into(),
        }));
        let seed = ParsingState::<Latexlike>::lang_initial_with_packages([package])
            .expect("seed state")
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                groups: GroupOverrides { rules: Some(groups), ..GroupOverrides::default() },
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Strict), seed);
        let result = language.parse("\\m«arg»").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), "arg");
    }

    #[test]
    fn v_codes_parse_delimited_verbatim() {
        // Auto-matched delimiter: raw content, comment and escape chars included.
        let result = parse_ok("v", r"\m|a%\x{|z");
        let m = macro_node(&result);
        assert_eq!(content_chars(m, 0), r"a%\x{");
        let group = m.child(0).unwrap();
        assert_eq!(group.group_type(), Some(GroupType::Verbatim));
        assert_eq!(group.group_delimiters(), Some(("|", "|")));
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("z"));

        // Prescribed delimiters.
        let result = parse_ok("v+-", r"\m+ab-");
        assert_eq!(content_chars(macro_node(&result), 0), "ab");
    }

    #[test]
    fn a_forbidden_char_delimits_verbatim_inside_math() {
        // `$` is forbidden inside math mode and also closes the math group, so it is
        // the sharp case for the delimiter probe: the probe clears both the close
        // expectation and the forbidden set, and reads it as an ordinary delimiter.
        // Neither recovery policy has anything to report.
        for recovery in [Recovery::Strict, Recovery::Tolerant] {
            let result = language(recovery, "v").parse(r"$\m$x$$ y").unwrap();
            check_latexlike_tree_invariants(&result.tree);
            assert!(
                result.diagnostics.is_empty(),
                "unexpected diagnostics under {recovery:?}: {:?}",
                result.diagnostics
            );

            let math = result.tree.root().child(0).expect("the math group");
            assert_eq!(math.group_type(), Some(GroupType::Math(MathGroupForm::Inline)));
            assert_eq!(math.group_delimiters(), Some(("$", "$")));
            assert_eq!(math.span().range(), 0..7);

            let m = math.child(0).expect("the macro inside the math group");
            assert_eq!(m.macro_name(), Some("m"));
            assert_eq!(content_chars(m, 0), "x");
            let verbatim = m.child(0).expect("the verbatim group");
            assert_eq!(verbatim.group_type(), Some(GroupType::Verbatim));
            assert_eq!(verbatim.group_delimiters(), Some(("$", "$")));

            assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" y"));
        }
    }
}
