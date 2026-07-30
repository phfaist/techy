//! Task 5 (stretch): a custom ConstructParser the standard parsers don't provide —
//! `@title` takes THE REST OF THE LINE as raw content (no tokenization: `[`, `@`,
//! `#` inert until newline), staged as a slot named "line" on the callable node.

use std::sync::Arc;

use techy::constructs::{ConstructParser, ConstructParserResult, Invocation, ParseContext};
use techy::node::{
    BuildId, CallableData, ChildRegion, ContentNodes, NodeKind, ParsedArguments, ParsedSlot,
};
use techy::scopes::Package;
use techy::source::{SourceSpan, Span};
use techy::spec::CallableSpec;
use techy::state::{ParsingStateDelta, TokenRulesOverrides};
use techy::token::TokenKind;

use crate::lang::{Notely, NotelyCallable};

/// The `@title` spec: no declared arguments, full-takeover invocation parser.
#[derive(Debug)]
pub struct TitleSpec;

impl CallableSpec<Notely> for TitleSpec {
    fn requires_content(&self) -> bool {
        true // takes material despite declaring no arguments (the documented channel)
    }

    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, Notely>,
    ) -> Box<dyn ConstructParser<Notely, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        Box::new(TitleLineParser { invocation })
    }
}

/// Tier-2 temporary: reads raw chars to end of line, stages chars + callable node.
struct TitleLineParser<'a, 's> {
    invocation: Invocation<'a, 's, Notely>,
}

impl ConstructParser<Notely> for TitleLineParser<'_, '_> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, Notely>,
    ) -> ConstructParserResult<Notely, (BuildId, Option<ParsingStateDelta<Notely>>)> {
        let trigger = self.invocation.token;

        // A raw sub-state: every syntax gate off, so each byte reads as a Char token
        // (the verbatim idiom, hand-built because `verbatim_state_delta` wants a
        // terminator GroupRule and rest-of-line has none).
        let raw = cx.derived_state(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_whitespace: Some(false),
            enable_multi_newline_paragraphs: Some(false),
            enable_groups: Some(false),
            enable_commands: Some(false),
            enable_comments: Some(false),
            enable_specials: Some(false),
            ..TokenRulesOverrides::default()
        }))?;

        let start = cx.tokens.pos();
        let mut end = start;
        loop {
            let token = match cx.tokens.peek(&raw) {
                Ok(token) => token,
                Err(_) => break, // unreadable byte: end the title here (demo policy)
            };
            match token.kind {
                TokenKind::Char('\n') | TokenKind::EndOfStream => break,
                TokenKind::Char(_) => {
                    end = token.span.end();
                    cx.tokens.move_past(&token, false);
                }
                _ => break, // unreachable under the raw state
            }
        }

        // Stage the line's chars node (if any) and mint the slot region over it.
        let mut children: Vec<BuildId> = Vec::new();
        let region = if end > start {
            let content_span = Span::new(start, end);
            let chars = cx
                .session
                .builder
                .add(
                    NodeKind::chars(content_span),
                    SourceSpan::new(&cx.source, content_span),
                    Arc::clone(&raw),
                    Vec::new(),
                )
                .map_err(|error| cx.implementation_error(error, content_span))?;
            children.push(chars);
            ChildRegion::new(0..1, ContentNodes::InRegion(0..1))
        } else {
            ChildRegion::new(0..0, ContentNodes::InRegion(0..0))
        };

        let node_span = Span::new(trigger.span.start(), end.max(trigger.span.end()));
        let data = CallableData {
            callable_type: self.invocation.callable_type,
            name: self.invocation.name.into(),
            spec: Arc::clone(self.invocation.spec),
            arguments: ParsedArguments::empty(),
            slots: vec![ParsedSlot::named("line", region)].into(),
            post_space: trigger.post_space().into(),
            ext: (),
        };
        let id = cx
            .session
            .builder
            .add(
                NodeKind::callable(data),
                SourceSpan::new(&cx.source, node_span),
                Arc::clone(&cx.state),
                children,
            )
            .map_err(|error| cx.implementation_error(error, node_span))?;
        Ok((id, None))
    }
}

/// The extension package registering `@title`.
pub fn notely_ext() -> Package<Notely> {
    let mut package = Package::new("notely-ext");
    package.insert(NotelyCallable::Command, "title", Arc::new(TitleSpec));
    package
}
