//! [`TackOnFieldsArgumentParser`]: the tack-on information-fields parser
//! (ParserLibraryParity.md N6; pylatexenc's `LatexTackOnInformationFieldMacrosParser`)
//! — absorbing trailing `\label{…}`-style invocations after a construct's declared
//! arguments and attaching them to the invocation node.
//!
//! Used as a callable's **last declared argument** (the FLM `label_arg` precedent):
//! attachment is the argument's region — the whole absorption happens at parse time
//! inside the invocation's own parse, per the decided construct-parser (not
//! postprocessing) ruling. The parser is configured with
//! the accepted field commands by **name**; a peeked
//! [`Command`](TokenKind::Command) token whose name is configured dispatches with the
//! configured [`CallableSpec`] directly — the scope stack is never consulted, so a
//! field command like `\label` need not exist as a language command at all, and
//! LaTeX's "a `\label` anywhere attaches to *something*" quirk is simply not
//! reproduced (decision reason 2). Anything else — an unconfigured command, plain
//! content, a group close, a paragraph break — ends the absorption, silently.
//!
//! # Staged shape
//!
//! Each absorbed field stages a full **`Callable` node** under the configured
//! per-name spec (diverging from pylatexenc's group-wrapper hack): the field's own
//! argument structure is the spec's [`ArgumentSpec`]s — pylatexenc's "default: one
//! expression" is a spec holding one
//! [`ExpressionParser`](super::ExpressionParser) argument; `\label`'s `{…}` is a
//! [`GroupArgumentParser`](super::GroupArgumentParser) or
//! [`CharsGroupArgumentParser`](super::CharsGroupArgumentParser) argument — and the
//! staged node self-describes (name, spec, its own
//! [`ParsedArguments`](crate::node::ParsedArguments) record). Dispatch routes through
//! [`ParseDriver::make_invocation_parser`], so takeover specs and driver interception
//! work as everywhere else. By-name reading is the extraction helper
//! [`split_tack_on_fields`](crate::extract::split_tack_on_fields).
//!
//! The argument's content designation covers the run of field nodes from the first
//! one on — noise between fields included, leading noise excluded (the
//! embellishments-parser convention).
//!
//! # Multiplicity and noise
//!
//! Per-field policy: a field registered with
//! [`with_field`](TackOnFieldsArgumentParser::with_field) may appear **once**; one
//! registered with
//! [`with_repeatable_field`](TackOnFieldsArgumentParser::with_repeatable_field) any
//! number of times (multiple `\label`s after a `\section`). A repeated
//! non-repeatable field diagnoses [`RepeatedTackOnField`] — and, tolerant, the field
//! is **still parsed and kept** in the region (diverging from pylatexenc's
//! parse-and-discard: techy trees keep every byte) — consumers see the
//! diagnostic and the record.
//!
//! Whitespace and comments **between** fields are scanned as region noise (diverging from pylatexenc, whose peek loop stops at a comment):
//! `\section{x} % note` + newline + `\label{y}` still associates. A probe that finds
//! no further field rewinds exactly its own noise scan — absorbed fields stay.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::engine::ParseDriver;
use crate::error::DiagnosticInfo;
use crate::node::{ArgumentExt, ContentNodes};
use crate::source::{SourceSpan, Span};
use crate::spec::{ArgumentParser, ArgumentSpec, CallableSpec, ParsedArgumentNodes};
use crate::state::Lang;
use crate::token::TokenKind;

use super::argument_parsers::{scan_argument_noise, stage_pre_space};
use super::{ConstructParserResult, FromInvocation, Invocation, invocation_frame, ParseContext};

/// Condition: a non-repeatable tack-on information field was specified more than once
/// (`\section{x}\label{a}\label{b}` with `label` registered via
/// [`with_field`](TackOnFieldsArgumentParser::with_field)) — detected by
/// [`TackOnFieldsArgumentParser`]. Tolerant recovery keeps the repeated field parsed
/// in the argument's region (module docs).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.arguments.repeated-tack-on-field",
    message = "information field ‘{escape_char}{name}’ cannot be specified more than once"
)]
pub struct RepeatedTackOnField {
    /// The field command's name (without escape character).
    pub name: String,
    /// The escape character the command was written with.
    pub escape_char: char,
}

/// One configured tack-on field: the command name that triggers it, the spec its
/// invocations parse under, and whether it may repeat.
struct TackOnField<L: Lang> {
    name: Box<str>,
    spec: Arc<dyn CallableSpec<L>>,
    repeatable: bool,
}

/// The tack-on information-fields parser (see the module docs): configured with the
/// invocation form its field nodes record and the accepted field commands by name.
pub struct TackOnFieldsArgumentParser<L: Lang> {
    callable_type: L::CallableTypeId,
    fields: Vec<TackOnField<L>>,
}

impl<L: Lang> TackOnFieldsArgumentParser<L> {
    /// A tack-on fields argument staging `Callable` nodes of invocation form
    /// `callable_type` (a preset's macro form, typically). Register the accepted
    /// fields with [`with_field`](TackOnFieldsArgumentParser::with_field) /
    /// [`with_repeatable_field`](TackOnFieldsArgumentParser::with_repeatable_field);
    /// with none registered the argument is always absent.
    pub fn new(callable_type: L::CallableTypeId) -> TackOnFieldsArgumentParser<L> {
        TackOnFieldsArgumentParser { callable_type, fields: Vec::new() }
    }

    /// Accept the field command `name` (without escape character), parsed under
    /// `spec`, at most **once** per invocation. Field names must be distinct across
    /// registrations (a duplicate is reported as an implementation error when the
    /// parser runs).
    pub fn with_field(
        self,
        name: impl Into<Box<str>>,
        spec: Arc<dyn CallableSpec<L>>,
    ) -> Self {
        self.field(name.into(), spec, false)
    }

    /// Accept the field command `name`, parsed under `spec`, **any number of times**
    /// per invocation.
    pub fn with_repeatable_field(
        self,
        name: impl Into<Box<str>>,
        spec: Arc<dyn CallableSpec<L>>,
    ) -> Self {
        self.field(name.into(), spec, true)
    }

    // Field names must be distinct — a spec-author contract, validated where the
    // parser runs ([§dd-dr:panic-policy]: an outer-layer contract violation is an
    // `Err`, never a panic).
    fn field(mut self, name: Box<str>, spec: Arc<dyn CallableSpec<L>>, repeatable: bool) -> Self {
        self.fields.push(TackOnField { name, spec, repeatable });
        self
    }
}

impl<L: Lang> ArgumentParser<L> for TackOnFieldsArgumentParser<L>
where
    ArgumentExt<L>: Default,
    L::InvocationSyntax: FromInvocation<L>,
{
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        // Spec-author contract, validated where the parser runs
        // ([§dd-dr:panic-policy]: an outer-layer contract violation is an `Err`).
        if let Some(duplicate) = self.fields.iter().enumerate().find_map(|(i, field)| {
            self.fields[..i]
                .iter()
                .any(|earlier| earlier.name == field.name)
                .then_some(&*field.name)
        }) {
            return Err(cx.implementation_error(
                format!(
                    "TackOnFieldsArgumentParser field names must be distinct \
                     (duplicate registration of ‘{duplicate}’)"
                ),
                Span::empty(cx.tokens.pos()),
            ));
        }
        let mut nodes = Vec::new();
        let mut seen = alloc::vec![false; self.fields.len()];
        let mut content_start: Option<u32> = None;

        loop {
            let noise = scan_argument_noise(cx)?;
            let Some(token) = noise.next.clone() else {
                noise.rewind(cx);
                break;
            };
            let TokenKind::Command { name, escape_char, .. } = &token.kind else {
                noise.rewind(cx);
                break;
            };
            let Some(index) =
                self.fields.iter().position(|field| &*field.name == *name)
            else {
                noise.rewind(cx);
                break;
            };
            let field = &self.fields[index];

            // Committed. The multiplicity check runs first: strict aborts before
            // consuming anything; tolerant records the condition and keeps parsing —
            // the repeated field stays in the region (module docs).
            if seen[index] && !field.repeatable {
                cx.recover(
                    RepeatedTackOnField::new(String::from(*name), *escape_char),
                    SourceSpan::new(&cx.source, token.span),
                )?;
            }
            seen[index] = true;

            nodes.extend(noise.nodes);
            stage_pre_space(cx, &mut nodes, token.pre_space)?;

            // The dispatch mirrors the expression position's: resolution is the
            // parser's own configuration (never the scope stack), the trigger is
            // consumed whole before the invocation parser runs, and the parser comes
            // from the driver's interception seam.
            let invocation = Invocation {
                callable_type: self.callable_type,
                name,
                spec: &field.spec,
                token: &token,
            };
            let frame = invocation_frame(cx, &invocation);
            cx.tokens.move_past(&token, true);
            let driver = cx.driver;
            let mut parser = driver.make_invocation_parser(invocation);
            let result = cx.with_frame(frame, |cx| parser.parse(cx));
            drop(parser);
            // An after-effect delta has no scope here: the argument channel
            // deliberately carries none (`ArgumentParser::parse_argument` docs).
            let (id, _delta) = result?;
            if content_start.is_none() {
                content_start = Some(nodes.len() as u32);
            }
            nodes.push(id);
        }

        match content_start {
            // The content: the full run from the first field node on (intermediate
            // noise included, leading noise excluded — module docs).
            Some(start) => {
                let end = nodes.len() as u32;
                Ok(Some(ParsedArgumentNodes::new(
                    nodes,
                    ContentNodes::InRegion(start..end),
                    Default::default(),
                )))
            }
            // No field matched; the reader is back at the entry position (the first
            // iteration's rewind).
            None => Ok(None),
        }
    }

    /// Optional: absent is a valid, silent outcome (the trait default, stated
    /// explicitly — the expression-position guard leans on this answer).
    fn can_match_empty(&self) -> bool {
        true
    }
}

impl<L: Lang> fmt::Debug for TackOnFieldsArgumentParser<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fields = f.debug_struct("TackOnFieldsArgumentParser");
        fields.field("callable_type", &self.callable_type);
        for field in &self.fields {
            fields.field(
                if field.repeatable { "repeatable_field" } else { "field" },
                &field.name,
            );
        }
        fields.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::CharsGroupArgumentParser;
    use crate::engine::Language;
    use crate::error::Recovery;
    use crate::latexlike::{
        argument_specs, CallableType, GroupType, Latexlike, LatexlikeDriver, MacroSpec,
    };
    use crate::extract::{content_as_chars, split_tack_on_fields_drop_annotations};
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::Package;
    use crate::state::ParsingState;
    use alloc::vec;
    use alloc::vec::Vec;

    /// The `\label{…}` field spec: one chars-group argument (the intended pairing of
    /// N4 and N6).
    fn label_spec() -> Arc<dyn CallableSpec<Latexlike>> {
        Arc::new(MacroSpec::new(vec![Arc::new(ArgumentSpec::new_unnamed(Arc::new(
            CharsGroupArgumentParser::new(GroupType::Content),
        )))]))
    }

    /// A language defining `\section{…}` whose second declared argument is the given
    /// tack-on parser. `\label`/`\nonumber` exist **only** through it — they are
    /// deliberately not registered as language commands.
    fn language(
        tack_on: TackOnFieldsArgumentParser<Latexlike>,
        recovery: Recovery,
    ) -> Language<Latexlike> {
        let mut arguments = argument_specs(["m"]).unwrap();
        arguments.push(Arc::new(ArgumentSpec::new_unnamed(Arc::new(tack_on))));
        let mut package = Package::new("tack-on-tests");
        package.insert(CallableType::Macro, "section", Arc::new(MacroSpec::new(arguments)));
        Language::new(
            LatexlikeDriver::new(recovery),
            ParsingState::lang_initial_with_packages([package]),
        )
    }

    fn repeatable_label() -> TackOnFieldsArgumentParser<Latexlike> {
        TackOnFieldsArgumentParser::new(CallableType::Macro)
            .with_repeatable_field("label", label_spec())
    }

    fn parse_ok(
        tack_on: TackOnFieldsArgumentParser<Latexlike>,
        input: &str,
    ) -> crate::engine::ParseResult<Latexlike> {
        let result = language(tack_on, Recovery::Strict).parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn section_node(result: &crate::engine::ParseResult<Latexlike>) -> NodeRef<'_, Latexlike> {
        let node = result.tree.root().child(0).expect("the section node");
        assert_eq!(node.macro_name(), Some("section"));
        node
    }

    #[test]
    fn trailing_fields_attach_to_the_invocation_node() {
        let result = parse_ok(repeatable_label(), r"\section{One} \label{sec:one} after");
        let section = section_node(&result);
        // The invocation's span runs through the absorbed field.
        assert_eq!(section.span().range(), 0..29);

        // The tack-on argument: region = [whitespace noise, the field node]; content
        // designation starts at the field node.
        let argument = section.arguments().unwrap().get(1).unwrap();
        assert!(argument.is_provided());
        let region: Vec<_> = section.argument_nodes(1).unwrap().iter().collect();
        assert_eq!(region.len(), 2);
        assert_eq!(region[0].chars(), Some(" "));
        let content: Vec<_> = section.argument_content_nodes(1).unwrap().iter().collect();
        assert_eq!(content.len(), 1);

        // The field is a full callable node under the configured spec, with its own
        // argument record.
        let label = content[0];
        assert!(label.is_callable());
        assert_eq!(label.name(), Some("label"));
        assert_eq!(label.callable_type(), Some(CallableType::Macro));
        assert_eq!(
            content_as_chars(label.argument_content_nodes(0).unwrap()).unwrap(),
            "sec:one"
        );

        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" after"));
    }

    #[test]
    fn field_commands_are_not_language_commands() {
        // Outside a tack-on position `\label` resolves nowhere — the decided
        // no-attach-anywhere behavior (module docs, decision reason 2).
        let result = language(repeatable_label(), Recovery::Tolerant)
            .parse(r"\label{a}")
            .unwrap();
        assert_eq!(result.diagnostics.len(), 1);
        assert!(
            result.diagnostics.iter().next().unwrap().message().contains("cannot resolve"),
            "{}",
            result.diagnostics.iter().next().unwrap().message()
        );
    }

    #[test]
    fn repeatable_fields_absorb_every_occurrence() {
        let result = parse_ok(repeatable_label(), r"\section{A}\label{x}\label{y}z");
        let section = section_node(&result);
        let content: Vec<_> = section.argument_content_nodes(1).unwrap().iter().collect();
        assert_eq!(content.len(), 2);

        let fields = split_tack_on_fields_drop_annotations(section.argument_content_nodes(1).unwrap()).unwrap();
        assert_eq!(fields.len(), 2);
        let keys: Vec<_> = fields.iter().map(|entry| entry.key().to_string()).collect();
        assert_eq!(keys, ["label", "label"]);
        // Last-wins lookup and collect-all composition, straight from the KeyVals
        // grammar.
        assert_eq!(
            content_as_chars(fields.get("label").unwrap().value().unwrap()).unwrap(),
            "y"
        );
        let combined = fields.get_combined_with("label", ",").unwrap().unwrap();
        assert_eq!(content_as_chars(combined.root().children()).unwrap(), "x,y");
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("z"));
    }

    #[test]
    fn non_repeatable_duplicates_are_diagnosed_and_kept() {
        let once = || {
            TackOnFieldsArgumentParser::new(CallableType::Macro)
                .with_field("label", label_spec())
        };

        let err =
            language(once(), Recovery::Strict).parse(r"\section{A}\label{x}\label{y}").unwrap_err();
        assert!(
            err.to_string().contains("cannot be specified more than once"),
            "{err}"
        );

        let result =
            language(once(), Recovery::Tolerant).parse(r"\section{A}\label{x}\label{y}").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), RepeatedTackOnField::IDENTIFIER);
        let condition = diagnostic.data().downcast_ref::<RepeatedTackOnField>().unwrap();
        assert_eq!(condition.name, "label");
        assert_eq!(condition.escape_char, '\\');
        // The repeated field stays parsed and attached (bytes are never dropped).
        let section = section_node(&result);
        let content: Vec<_> = section.argument_content_nodes(1).unwrap().iter().collect();
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn noise_between_fields_keeps_the_association() {
        let result = parse_ok(repeatable_label(), "\\section{A} % note\n\\label{x}!");
        let section = section_node(&result);
        let region: Vec<_> = section.argument_nodes(1).unwrap().iter().collect();
        assert!(region.iter().any(|node| node.comment().is_some()));
        let fields = split_tack_on_fields_drop_annotations(section.argument_content_nodes(1).unwrap()).unwrap();
        assert_eq!(
            content_as_chars(fields.get("label").unwrap().value().unwrap()).unwrap(),
            "x"
        );
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("!"));
    }

    #[test]
    fn absorption_stops_at_anything_else_silently() {
        let result = parse_ok(repeatable_label(), r"\section{A} text");
        let section = section_node(&result);
        assert_eq!(section.span().range(), 0..11);
        assert!(!section.arguments().unwrap().get(1).unwrap().is_provided());
        assert_eq!(result.tree.root().child(1).unwrap().chars(), Some(" text"));
    }

    #[test]
    fn zero_argument_fields_record_no_value() {
        let tack_on = TackOnFieldsArgumentParser::new(CallableType::Macro)
            .with_repeatable_field("label", label_spec())
            .with_field("nonumber", Arc::new(MacroSpec::new(vec![])));
        let result = parse_ok(tack_on, r"\section{A}\nonumber\label{x}");
        let section = section_node(&result);
        let fields = split_tack_on_fields_drop_annotations(section.argument_content_nodes(1).unwrap()).unwrap();
        assert_eq!(fields.len(), 2);
        assert!(fields.get("nonumber").unwrap().value().is_none());
        assert_eq!(
            content_as_chars(fields.get("label").unwrap().value().unwrap()).unwrap(),
            "x"
        );
    }

    // --- spec-author contract violations are implementation errors ([§dd-dr:panic-policy]) --

    #[test]
    fn duplicate_field_names_are_an_implementation_error() {
        let tack_on = TackOnFieldsArgumentParser::new(CallableType::Macro)
            .with_field("label", label_spec())
            .with_repeatable_field("label", label_spec());
        // Tolerant recovery, deliberately: an implementation error bypasses the
        // recover funnel and aborts even there.
        let err = language(tack_on, Recovery::Tolerant)
            .parse(r"\section{A}\label{x}")
            .unwrap_err();
        assert_eq!(
            err.identifier(),
            crate::constructs::ImplementationError::IDENTIFIER
        );
    }
}
