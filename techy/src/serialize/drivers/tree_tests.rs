//! Tests of the tree driver (M4): round-trips of hand-built and parsed trees covering
//! every node kind and region shape, the D21 argument-spec index rule and its
//! override, multi-tree sharing, the fresh layout tag, a hostile-input battery, and —
//! under the `serde` feature — a pinned JSON snapshot.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use crate::node::{
    CallableData, ChildRegion, ContentNodes, GroupData, NodeBuildError, NodeKind, NodeTree,
    NodeTreeBuilder, ParsedArgument, ParsedArguments, ParsedSlot, ParsedSlots, SlotRole,
};
use crate::scopes::{CallableQuery, ProviderError, SpecsProvider};
use crate::source::{Source, SourceSpan, Span, TextContent};
use crate::spec::{ArgumentParser, ArgumentSpec, CallableSpec, ParsedArgumentNodes};
use crate::state::{InvocationSyntax, Lang, ParsingState, TrivialLang};

use crate::constructs::{ConstructParserResult, ParseContext};
use crate::engine::{Language, ScopesCommandResolver, StdParseDriver};
use crate::error::Recovery;
use crate::serialize::tree_support::{ignore_annotations, round_trip_tree};
use crate::serialize::{
    DeserializableObject, DeserializableValue, DeserializeContext, DeserializeError, SerdeSession,
    Segment, SerialEntry, SerialIndex, SerialValue, SerialValueError, SerializableLang, SerializableObject,
    SerializableValue, SerializeContext, SerializeError, TreeSerialization,
};

// --- the toy lang for hand-built trees ------------------------------------------------

/// Every feature present, every type defaulted: the empty `SerializableLang` impl.
#[derive(Debug, Clone, Copy)]
struct ToyLang;
impl TrivialLang for ToyLang {}
impl SerializableLang for ToyLang {}

const GT_BRACE: u32 = 0;
const CT_MACRO: u32 = 10;
const CT_ENV: u32 = 12;

type ToySource = Arc<Source<Option<String>>>;
type ToyState = Arc<ParsingState<ToyLang>>;

fn source(text: &str) -> ToySource {
    Arc::new(Source::new(text).with_origin(Some(text.to_string())))
}

fn state() -> ToyState {
    Arc::new(ParsingState::<ToyLang>::lang_initial().expect("seed state"))
}

fn span(src: &ToySource, range: core::ops::Range<usize>) -> SourceSpan<Option<String>> {
    SourceSpan::new(src, range)
}

// --- the toy argument parser and specs ------------------------------------------------

/// A stand-in argument parser: never invoked in hand-built trees, and irrelevant on
/// read (argument parsers are not serialized — the rebuilt spec's declared argument
/// specs use this one uniformly).
#[derive(Debug)]
struct ToyArgParser;

impl<L: Lang> ArgumentParser<L> for ToyArgParser {
    fn parse_argument(
        &self,
        _cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        Ok(None)
    }
}

/// A self-describing callable spec: its name and its argument names. Its declared
/// argument specs are the ones parsed arguments point at (the D21 index rule); the
/// argument parser is a stub (parsers are not serialized).
struct ToySpec<L: Lang> {
    name: String,
    arguments: Vec<Arc<ArgumentSpec<L>>>,
    arg_names: Vec<String>,
}

impl<L: Lang> ToySpec<L> {
    fn new(name: &str, arg_names: &[&str]) -> ToySpec<L> {
        let arguments = arg_names
            .iter()
            .map(|argument| Arc::new(ArgumentSpec::new(ToyArgParser, *argument)))
            .collect();
        ToySpec {
            name: name.to_string(),
            arguments,
            arg_names: arg_names.iter().map(|argument| argument.to_string()).collect(),
        }
    }

    fn shared(name: &str, arg_names: &[&str]) -> Arc<ToySpec<L>> {
        Arc::new(ToySpec::new(name, arg_names))
    }
}

// A stable Debug that shows the name and argument names only (not the stub parser
// Arcs), so the deep-compare of a rebuilt spec matches the original.
impl<L: Lang> fmt::Debug for ToySpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToySpec").field("name", &self.name).field("args", &self.arg_names).finish()
    }
}

impl<L: Lang> CallableSpec<L> for ToySpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }
}

impl<L: Lang> SerializableObject<L> for ToySpec<L> {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let args = SerialValue::List(self.arg_names.iter().map(|argument| SerialValue::Str(argument.clone())).collect());
        Ok(SerialEntry {
            identifier: "toy.spec".into(),
            data: SerialValue::Map(Vec::from([
                (String::from("name"), SerialValue::Str(self.name.clone())),
                (String::from("args"), args),
            ])),
        })
    }
}

impl<L: SerializableLang> DeserializableObject<L> for ToySpec<L> {
    type Output = ToySpec<L>;

    fn deserialize_object(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self::Output, DeserializeError> {
        let SerialValue::Map(fields) = value else {
            return Err(DeserializeError::failed("a toy spec is a map"));
        };
        let name = match fields.iter().find(|(key, _)| key == "name").map(|(_, value)| value) {
            Some(SerialValue::Str(name)) => name.clone(),
            _ => return Err(DeserializeError::failed("a toy spec has a string name")),
        };
        let args = match fields.iter().find(|(key, _)| key == "args").map(|(_, value)| value) {
            Some(SerialValue::List(items)) => items
                .iter()
                .map(|item| match item {
                    SerialValue::Str(argument) => Ok(argument.clone()),
                    _ => Err(DeserializeError::failed("a toy spec argument name is a string")),
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(DeserializeError::failed("a toy spec has an argument-name list")),
        };
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        Ok(ToySpec::new(&name, &arg_refs))
    }
}

/// A callable spec whose parsed arguments carry an *out-of-band* argument spec (one it
/// does not declare): it overrides the D21 pair, serializing each argument spec's name
/// and rebuilding it on read.
#[derive(Debug)]
struct OobSpec {
    name: String,
}

impl OobSpec {
    fn shared(name: &str) -> Arc<OobSpec> {
        Arc::new(OobSpec { name: name.to_string() })
    }
}

impl<L: Lang> CallableSpec<L> for OobSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &[]
    }

    fn serialize_argument_spec(
        &self,
        _index: usize,
        argument_spec: &Arc<ArgumentSpec<L>>,
        _cx: &mut SerializeContext<'_, L>,
    ) -> Result<Option<SerialValue>, SerializeError>
    where
        L: SerializableLang,
    {
        let name = argument_spec.name.as_deref().unwrap_or("");
        Ok(Some(SerialValue::Str(name.to_string())))
    }

    fn deserialize_argument_spec(
        &self,
        _index: usize,
        value: Option<&SerialValue>,
        _cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<ArgumentSpec<L>>, DeserializeError>
    where
        L: SerializableLang,
    {
        let Some(SerialValue::Str(name)) = value else {
            return Err(DeserializeError::failed("an out-of-band argument spec is its name"));
        };
        Ok(Arc::new(ArgumentSpec::new(ToyArgParser, name.as_str())))
    }
}

impl<L: Lang> SerializableObject<L> for OobSpec {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialEntry { identifier: "toy.oob-spec".into(), data: SerialValue::Str(self.name.clone()) })
    }
}

impl<L: SerializableLang> DeserializableObject<L> for OobSpec {
    type Output = OobSpec;

    fn deserialize_object(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self::Output, DeserializeError> {
        match value {
            SerialValue::Str(name) => Ok(OobSpec { name: name.clone() }),
            _ => Err(DeserializeError::failed("an oob spec is its name")),
        }
    }
}

// --- session setup --------------------------------------------------------------------

/// A session with the standard tables, the toy spec readers registered, and (for
/// custom annotation types) the annotation codec registered.
fn setup<L: SerializableLang>() -> SerdeSession<L> {
    let mut session = SerdeSession::<L>::new();
    let specs = session.standard_tables().unwrap().specs;
    specs.register_type::<ToySpec<L>>(&mut session, "toy.spec", |spec| Arc::new(spec) as Arc<dyn CallableSpec<L>>).unwrap();
    specs.register_type::<OobSpec>(&mut session, "toy.oob-spec", |spec| Arc::new(spec) as Arc<dyn CallableSpec<L>>).unwrap();
    session
}

// --- hand-built trees -----------------------------------------------------------------

/// A tree over `x{y}z`: chars, a typed group with an inner chars child, chars — the
/// non-callable kinds, one shared state and source.
fn chars_group_tree() -> NodeTree<ToyLang, ()> {
    let src = source("x{y}z");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let x = builder.add(NodeKind::chars(Span::new(0, 1)), span(&src, 0..1), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let y = builder.add(NodeKind::chars(Span::new(2, 3)), span(&src, 2..3), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let group = builder
        .add(
            NodeKind::group(GroupData::new(GT_BRACE, Span::new(1, 2), Span::new(3, 4))),
            span(&src, 1..4),
            Arc::clone(&st),
            Vec::from([y]),
            (),
            (),
        )
        .unwrap();
    let z = builder.add(NodeKind::chars(Span::new(4, 5)), span(&src, 4..5), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let comment = builder
        .add(
            NodeKind::comment(TextContent::from("%"), TextContent::from(" c"), TextContent::empty()),
            span(&src, 0..0),
            Arc::clone(&st),
            Vec::new(),
            (),
            (),
        )
        .unwrap();
    let root = builder
        .add(NodeKind::list(), span(&src, 0..5), st, Vec::from([x, group, z, comment]), (), ())
        .unwrap();
    builder.finish(root).unwrap()
}

/// A `\frac{a}{b}` callable: two group arguments whose content is inside the groups
/// (`InChildrenOf`), plus a synthesized `List` root — exercises groups-as-argument
/// regions and `InChildrenOf` content.
fn callable_group_args_tree() -> NodeTree<ToyLang, ()> {
    let src = source("\\frac{a}{b}");
    let st = state();
    let spec: Arc<ToySpec<ToyLang>> = ToySpec::shared("frac", &["num", "den"]);
    let arg0_spec = Arc::clone(&spec.arguments()[0]);
    let arg1_spec = Arc::clone(&spec.arguments()[1]);
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();

    // {a}: group with an inner chars child.
    let a = builder.add(NodeKind::chars(Span::new(6, 7)), span(&src, 6..7), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let g0 = builder
        .add(
            NodeKind::group(GroupData::new(GT_BRACE, Span::new(5, 6), Span::new(7, 8))),
            span(&src, 5..8),
            Arc::clone(&st),
            Vec::from([a]),
            (),
            (),
        )
        .unwrap();
    // {b}
    let b = builder.add(NodeKind::chars(Span::new(9, 10)), span(&src, 9..10), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let g1 = builder
        .add(
            NodeKind::group(GroupData::new(GT_BRACE, Span::new(8, 9), Span::new(10, 11))),
            span(&src, 8..11),
            Arc::clone(&st),
            Vec::from([b]),
            (),
            (),
        )
        .unwrap();

    let arguments = ParsedArguments::new(Vec::from([
        ParsedArgument::provided(arg0_spec, ChildRegion::new(0..1, ContentNodes::InChildrenOf(g0, 0..1)), ()),
        ParsedArgument::provided(arg1_spec, ChildRegion::new(1..2, ContentNodes::InChildrenOf(g1, 0..1)), ()),
    ]));
    let callable = CallableData {
        callable_type: CT_MACRO,
        name: "frac".into(),
        spec: spec as Arc<dyn CallableSpec<ToyLang>>,
        arguments,
        slots: ParsedSlots::empty(),
        invocation_syntax: (),
    };
    let node = builder
        .add(NodeKind::callable(callable), span(&src, 0..11), Arc::clone(&st), Vec::from([g0, g1]), (), ())
        .unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..11), st, Vec::from([node]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

/// A `\frac 1 2` callable: two single-token arguments that are the content directly
/// (`InRegion` via `ChildRegion::single`), plus one absent optional argument.
fn callable_token_args_tree() -> NodeTree<ToyLang, ()> {
    let src = source("\\frac 1 2");
    let st = state();
    let spec: Arc<ToySpec<ToyLang>> = ToySpec::shared("frac", &["num", "den", "opt"]);
    let arg0_spec = Arc::clone(&spec.arguments()[0]);
    let arg1_spec = Arc::clone(&spec.arguments()[1]);
    let arg2_spec = Arc::clone(&spec.arguments()[2]);
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let one = builder.add(NodeKind::chars(Span::new(6, 7)), span(&src, 6..7), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let two = builder.add(NodeKind::chars(Span::new(8, 9)), span(&src, 8..9), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let arguments = ParsedArguments::new(Vec::from([
        ParsedArgument::provided(arg0_spec, ChildRegion::single(0), ()),
        ParsedArgument::provided(arg1_spec, ChildRegion::single(1), ()),
        ParsedArgument::absent(arg2_spec),
    ]));
    let callable = CallableData {
        callable_type: CT_MACRO,
        name: "frac".into(),
        spec: spec as Arc<dyn CallableSpec<ToyLang>>,
        arguments,
        slots: ParsedSlots::empty(),
        invocation_syntax: (),
    };
    let node = builder
        .add(NodeKind::callable(callable), span(&src, 0..9), Arc::clone(&st), Vec::from([one, two]), (), ())
        .unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..9), st, Vec::from([node]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

/// An environment-shaped callable with three slots (one per role): a body `List`
/// (`Content`, `InChildrenOf`), an attached region-level slot (`Attached`, `InRegion`),
/// and a hidden region-level slot (`Hidden`, `InRegion`).
fn callable_slots_tree() -> NodeTree<ToyLang, ()> {
    let src = source("BODYAH");
    let st = state();
    let spec: Arc<ToySpec<ToyLang>> = ToySpec::shared("env", &[]);
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    // body content: one chars node inside a body List.
    let body_chars = builder.add(NodeKind::chars(Span::new(0, 4)), span(&src, 0..4), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let body = builder.add(NodeKind::list(), span(&src, 0..4), Arc::clone(&st), Vec::from([body_chars]), (), ()).unwrap();
    // attached and hidden region-level chars.
    let attached = builder.add(NodeKind::chars(Span::new(4, 5)), span(&src, 4..5), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let hidden = builder.add(NodeKind::chars(Span::new(5, 6)), span(&src, 5..6), Arc::clone(&st), Vec::new(), (), ()).unwrap();

    let slots = ParsedSlots::new(Vec::from([
        ParsedSlot::new(ChildRegion::new(0..1, ContentNodes::InChildrenOf(body, 0..1)), "body", SlotRole::Content, ()),
        ParsedSlot::new(ChildRegion::new(1..2, ContentNodes::InRegion(0..1)), "attached", SlotRole::Attached, ()),
        ParsedSlot::new_unnamed(ChildRegion::new(2..3, ContentNodes::InRegion(0..1)), SlotRole::Hidden, ()),
    ]));
    let callable = CallableData {
        callable_type: CT_ENV,
        name: "env".into(),
        spec: spec as Arc<dyn CallableSpec<ToyLang>>,
        arguments: ParsedArguments::empty(),
        slots,
        invocation_syntax: (),
    };
    let node = builder
        .add(NodeKind::callable(callable), span(&src, 0..6), Arc::clone(&st), Vec::from([body, attached, hidden]), (), ())
        .unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..6), st, Vec::from([node]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

/// A multi-source tree with `Owned` text: the root's child spans a second source, and
/// carries an owned (normalized) chars payload.
fn multi_source_tree() -> NodeTree<ToyLang, ()> {
    let main = source("AB");
    let other = source("CD");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let a = builder.add(NodeKind::chars(Span::new(0, 1)), span(&main, 0..1), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    // An owned-text chars node whose span lies in the other source.
    let owned = builder
        .add(NodeKind::chars(TextContent::from("normalized")), span(&other, 0..2), Arc::clone(&st), Vec::new(), (), ())
        .unwrap();
    let root = builder.add(NodeKind::list(), span(&main, 0..2), st, Vec::from([a, owned]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

// --- round-trip tests -----------------------------------------------------------------

#[test]
fn chars_group_and_comment_round_trip() {
    round_trip_tree(setup::<ToyLang>, &chars_group_tree(), ignore_annotations);
}

#[test]
fn callable_with_group_arguments_round_trips() {
    round_trip_tree(setup::<ToyLang>, &callable_group_args_tree(), ignore_annotations);
}

#[test]
fn callable_with_token_and_absent_arguments_round_trips() {
    round_trip_tree(setup::<ToyLang>, &callable_token_args_tree(), ignore_annotations);
}

#[test]
fn callable_with_slots_of_every_role_round_trips() {
    round_trip_tree(setup::<ToyLang>, &callable_slots_tree(), ignore_annotations);
}

#[test]
fn multi_source_owned_text_round_trips() {
    let back = round_trip_tree(setup::<ToyLang>, &multi_source_tree(), ignore_annotations);
    // Not single-source, and the two sources stay distinct after the round trip.
    assert!(!back.is_single_source());
}

// --- annotations ----------------------------------------------------------------------

type SpanAnnotation = SourceSpan<Option<String>>;

fn span_annotation_eq(a: &SpanAnnotation, b: &SpanAnnotation) -> bool {
    a.start() == b.start() && a.end() == b.end() && a.source().content() == b.source().content()
}

/// A setup whose trees table also has the `SourceSpan` annotation codec registered.
fn setup_span_annotations() -> SerdeSession<ToyLang> {
    let mut session = setup::<ToyLang>();
    let trees = session.standard_tables().unwrap().trees;
    trees.register_annotation::<SpanAnnotation>(&mut session, "test.span").unwrap();
    session
}

#[test]
fn a_tree_of_source_span_annotations_round_trips() {
    // Each node's annotation is a span into the tree's own source (an interned table
    // object riding on the value conversion).
    let src = source("hello");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, SpanAnnotation>::new();
    let child = builder
        .add(NodeKind::chars(Span::new(0, 5)), span(&src, 0..5), Arc::clone(&st), Vec::new(), (), span(&src, 0..3))
        .unwrap();
    let root =
        builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([child]), (), span(&src, 0..5)).unwrap();
    let tree = builder.finish(root).unwrap();
    round_trip_tree(setup_span_annotations, &tree, span_annotation_eq);
}

// --- D21: the argument-spec index rule and its override -------------------------------

/// A callable whose one provided argument carries an *out-of-band* argument spec,
/// handled by [`OobSpec`]'s override of the D21 pair.
fn oob_tree() -> NodeTree<ToyLang, ()> {
    let src = source("\\x{q}");
    let st = state();
    let spec = OobSpec::shared("x");
    let oob_arg = Arc::new(ArgumentSpec::new(ToyArgParser, "oob"));
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let q = builder.add(NodeKind::chars(Span::new(3, 4)), span(&src, 3..4), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let group = builder
        .add(
            NodeKind::group(GroupData::new(GT_BRACE, Span::new(2, 3), Span::new(4, 5))),
            span(&src, 2..5),
            Arc::clone(&st),
            Vec::from([q]),
            (),
            (),
        )
        .unwrap();
    let arguments = ParsedArguments::new(Vec::from([ParsedArgument::provided(
        oob_arg,
        ChildRegion::new(0..1, ContentNodes::InChildrenOf(group, 0..1)),
        (),
    )]));
    let callable = CallableData {
        callable_type: CT_MACRO,
        name: "x".into(),
        spec: spec as Arc<dyn CallableSpec<ToyLang>>,
        arguments,
        slots: ParsedSlots::empty(),
        invocation_syntax: (),
    };
    let node =
        builder.add(NodeKind::callable(callable), span(&src, 0..5), Arc::clone(&st), Vec::from([group]), (), ()).unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([node]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

#[test]
fn an_overriding_spec_round_trips_its_out_of_band_argument_spec() {
    round_trip_tree(setup::<ToyLang>, &oob_tree(), ignore_annotations);
}

#[test]
fn an_out_of_band_argument_spec_without_an_override_is_a_write_error() {
    // A default-D21 spec (ToySpec) declaring one argument, but the parsed argument
    // points at a different argument-spec Arc: the write fails naming node and callable.
    let src = source("\\x{q}");
    let st = state();
    let spec: Arc<ToySpec<ToyLang>> = ToySpec::shared("x", &["declared"]);
    let unrelated = Arc::new(ArgumentSpec::new(ToyArgParser, "out-of-band"));
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let q = builder.add(NodeKind::chars(Span::new(3, 4)), span(&src, 3..4), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let group = builder
        .add(
            NodeKind::group(GroupData::new(GT_BRACE, Span::new(2, 3), Span::new(4, 5))),
            span(&src, 2..5),
            Arc::clone(&st),
            Vec::from([q]),
            (),
            (),
        )
        .unwrap();
    let arguments = ParsedArguments::new(Vec::from([ParsedArgument::provided(
        unrelated,
        ChildRegion::new(0..1, ContentNodes::InChildrenOf(group, 0..1)),
        (),
    )]));
    let callable = CallableData {
        callable_type: CT_MACRO,
        name: "x".into(),
        spec: spec as Arc<dyn CallableSpec<ToyLang>>,
        arguments,
        slots: ParsedSlots::empty(),
        invocation_syntax: (),
    };
    let node =
        builder.add(NodeKind::callable(callable), span(&src, 0..5), Arc::clone(&st), Vec::from([group]), (), ()).unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([node]), (), ()).unwrap();
    let tree = builder.finish(root).unwrap();

    let mut writer = setup::<ToyLang>();
    let error = writer.serialize_tree(&tree).unwrap_err();
    // The failure is located in the trees table, at node #1 (the callable `x` — the
    // root is node #0), and is the out-of-band diagnosis.
    assert!(matches!(&error, SerializeError::InTable { table: "trees", .. }), "{error}");
    assert_eq!(write_node_location(&error), Some((1, Some("x"))), "{error}");
    assert!(matches!(innermost_write(&error), SerializeError::ArgumentSpecOutOfBand { index: 0, count: 1 }), "{error}");
    // The rendering reads location-outermost-first.
    let rendered = error.to_string();
    let expected_prefix = "while serializing an object into table `trees`: while serializing node #1 \
                           (callable `x`) of the tree: argument #1";
    assert!(rendered.starts_with(expected_prefix), "{rendered}");
}

// --- fresh tag ------------------------------------------------------------------------

#[test]
fn the_rebuilt_tree_mints_a_fresh_layout_tag() {
    let tree = chars_group_tree();
    let original_root = tree.root().id();
    let back = round_trip_tree(setup::<ToyLang>, &tree, ignore_annotations);
    // A node id from the original tree does not belong to the rebuilt one.
    assert!(back.get(original_root).is_none());
    assert_ne!(tree.tree_tag(), back.tree_tag());
}

// --- multi-tree sharing ---------------------------------------------------------------

fn simple_callable_tree(src: &ToySource, st: &ToyState, spec: &Arc<ToySpec<ToyLang>>, name: &str) -> NodeTree<ToyLang, ()> {
    // A zero-argument macro: the callable has no children (nothing to tile).
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let callable = CallableData {
        callable_type: CT_MACRO,
        name: name.into(),
        spec: Arc::clone(spec) as Arc<dyn CallableSpec<ToyLang>>,
        arguments: ParsedArguments::empty(),
        slots: ParsedSlots::empty(),
        invocation_syntax: (),
    };
    let node = builder.add(NodeKind::callable(callable), span(src, 0..2), Arc::clone(st), Vec::new(), (), ()).unwrap();
    let root = builder.add(NodeKind::list(), span(src, 0..2), Arc::clone(st), Vec::from([node]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

#[test]
fn multiple_trees_share_states_sources_and_specs() {
    let src = source("ab");
    let st = state();
    let spec: Arc<ToySpec<ToyLang>> = ToySpec::shared("m", &[]);
    let tree_a = simple_callable_tree(&src, &st, &spec, "m");
    let tree_b = simple_callable_tree(&src, &st, &spec, "m");

    let mut writer = setup::<ToyLang>();
    let pa = writer.serialize_tree(&tree_a).unwrap();
    let pb = writer.serialize_tree(&tree_b).unwrap();
    let segment = writer.take_segment();
    // The shared state, source, and spec were each written once; two tree entries.
    assert_eq!(count_entries(&segment, "states"), 1);
    assert_eq!(count_entries(&segment, "sources"), 1);
    assert_eq!(count_entries(&segment, "specs"), 1);
    assert_eq!(count_entries(&segment, "trees"), 2);

    let mut reader = setup::<ToyLang>();
    reader.push_segment(segment).unwrap();
    let trees = reader.standard_tables().unwrap().trees;
    let back_a: NodeTree<ToyLang, ()> = reader.tree(trees.position(pa.index())).unwrap();
    let back_b: NodeTree<ToyLang, ()> = reader.tree(trees.position(pb.index())).unwrap();
    // The shared state, source, and spec survive as one Arc across the two trees.
    assert!(Arc::ptr_eq(&back_a.nodes()[0].parsing_state, &back_b.nodes()[0].parsing_state));
    assert!(Arc::ptr_eq(back_a.nodes()[0].span.source(), back_b.nodes()[0].span.source()));
    assert!(core::ptr::eq(callable_spec_ptr(&back_a), callable_spec_ptr(&back_b)));
}

fn callable_spec_ptr(tree: &NodeTree<ToyLang, ()>) -> *const () {
    for node in tree.nodes() {
        if let NodeKind::Callable(data) = &node.kind {
            return Arc::as_ptr(&data.spec) as *const ();
        }
    }
    panic!("no callable node");
}

// --- determinism ----------------------------------------------------------------------

#[test]
fn two_sessions_emit_identical_tree_segments() {
    let tree = callable_group_args_tree();
    let mut first = setup::<ToyLang>();
    first.serialize_tree(&tree).unwrap();
    let mut second = setup::<ToyLang>();
    second.serialize_tree(&tree).unwrap();
    assert_eq!(first.take_segment().to_serial_value(), second.take_segment().to_serial_value());
}

// --- hostile-input battery ------------------------------------------------------------

fn count_entries(segment: &Segment, table: &str) -> usize {
    segment.tables().iter().find(|t| t.name() == table).map_or(0, |t| t.entries().len())
}

/// The failure under every location wrapper (`InEntry`, `InNode`).
fn innermost(error: &DeserializeError) -> &DeserializeError {
    match error {
        DeserializeError::InEntry { cause, .. } | DeserializeError::InNode { cause, .. } => innermost(cause),
        other => other,
    }
}

/// The failure under every location wrapper (`InTable`, `InNode`).
fn innermost_write(error: &SerializeError) -> &SerializeError {
    match error {
        SerializeError::InTable { cause, .. } | SerializeError::InNode { cause, .. } => innermost_write(cause),
        other => other,
    }
}

/// The node location a read error carries (`InNode`'s node position and callable
/// name), searched through the wrappers.
fn read_node_location(error: &DeserializeError) -> Option<(u32, Option<&str>)> {
    match error {
        DeserializeError::InNode { node, callable, .. } => Some((*node, callable.as_deref())),
        DeserializeError::InEntry { cause, .. } => read_node_location(cause),
        _ => None,
    }
}

/// The node location a write error carries (`InNode`'s node position and callable
/// name), searched through the wrappers.
fn write_node_location(error: &SerializeError) -> Option<(u32, Option<&str>)> {
    match error {
        SerializeError::InNode { node, callable, .. } => Some((*node, callable.as_deref())),
        SerializeError::InTable { cause, .. } => write_node_location(cause),
        _ => None,
    }
}

fn map_field_mut<'a>(value: &'a mut SerialValue, key: &str) -> &'a mut SerialValue {
    match value {
        SerialValue::Map(fields) => {
            &mut fields.iter_mut().find(|(k, _)| k == key).unwrap_or_else(|| panic!("no key `{key}`")).1
        }
        _ => panic!("not a map (expected key `{key}`)"),
    }
}

fn list_mut(value: &mut SerialValue) -> &mut Vec<SerialValue> {
    match value {
        SerialValue::List(items) => items,
        _ => panic!("not a list"),
    }
}

fn find_table_mut<'a>(segment: &'a mut SerialValue, name: &str) -> &'a mut SerialValue {
    list_mut(map_field_mut(segment, "tables"))
        .iter_mut()
        .find(|table| matches!(table, SerialValue::Map(fields)
            if fields.iter().any(|(k, v)| k == "name" && matches!(v, SerialValue::Str(n) if n == name))))
        .unwrap()
}

/// The `data` map of the first entry of the trees table.
fn tree_entry_data_mut(segment: &mut SerialValue) -> &mut SerialValue {
    let trees = find_table_mut(segment, "trees");
    let entries = list_mut(map_field_mut(trees, "entries"));
    map_field_mut(&mut entries[0], "data")
}

fn tree_nodes_mut(segment: &mut SerialValue) -> &mut Vec<SerialValue> {
    list_mut(map_field_mut(tree_entry_data_mut(segment), "nodes"))
}

/// Serialize `base`, edit the whole segment value, and push it into a fresh reader,
/// returning the error.
fn push_edited(base: &NodeTree<ToyLang, ()>, edit: impl FnOnce(&mut SerialValue)) -> DeserializeError {
    push_edited_in(setup::<ToyLang>, base, edit)
}

/// [`push_edited`] for a tree of any language: `setup` builds both sessions.
fn push_edited_in<L: SerializableLang>(
    setup: impl Fn() -> SerdeSession<L>,
    base: &NodeTree<L, ()>,
    edit: impl FnOnce(&mut SerialValue),
) -> DeserializeError {
    let mut writer = setup();
    writer.serialize_tree(base).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    edit(&mut value);
    let edited = Segment::from_serial_value(&value).expect("edited segment still parses as a segment");
    let mut reader = setup();
    reader.push_segment(edited).unwrap_err()
}

#[test]
fn a_bad_source_index_is_rejected() {
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        if let SerialValue::Index { index, .. } = map_field_mut(map_field_mut(&mut nodes[0], "span"), "source") {
            *index = 999;
        } else {
            panic!("span source is a table position");
        }
    });
    assert!(matches!(innermost(&error), DeserializeError::IndexOutOfRange { table: "sources", .. }), "{error}");
}

#[test]
fn a_bad_state_index_is_rejected() {
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        if let SerialValue::Index { index, .. } = map_field_mut(&mut nodes[0], "state") {
            *index = 999;
        }
    });
    assert!(matches!(innermost(&error), DeserializeError::IndexOutOfRange { table: "states", .. }), "{error}");
    // Located: the tree's entry, then the node (the root, not a callable).
    assert!(matches!(&error, DeserializeError::InEntry { table: "trees", .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((0, None)), "{error}");
}

#[test]
fn a_wrong_table_index_is_rejected() {
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        // Point the state field at the sources table (ordinal 0).
        if let SerialValue::Index { table, .. } = map_field_mut(&mut nodes[0], "state") {
            *table = crate::serialize::TableId::new(0);
        }
    });
    assert!(matches!(innermost(&error), DeserializeError::WrongTable { expected: "states", .. }), "{error}");
}

#[test]
fn a_bad_spec_index_is_rejected() {
    let error = push_edited(&callable_group_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        // Node 1 is the callable (root is 0, the callable its only child).
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        if let SerialValue::Index { index, .. } = map_field_mut(callable, "spec") {
            *index = 999;
        }
    });
    assert!(matches!(innermost(&error), DeserializeError::IndexOutOfRange { table: "specs", .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, Some("frac"))), "{error}");
}

#[test]
fn a_span_out_of_bounds_is_rejected() {
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        *map_field_mut(map_field_mut(&mut nodes[0], "span"), "end") = SerialValue::Int(9999);
    });
    assert!(matches!(innermost(&error), DeserializeError::SpanOutOfBounds { .. }), "{error}");
}

#[test]
fn a_span_off_a_char_boundary_is_rejected() {
    // A tree over "café" (é is two bytes at 3..5); a node span ending mid-é.
    let src = source("café");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let child = builder
        .add(NodeKind::chars(TextContent::from("café")), span(&src, 0..5), Arc::clone(&st), Vec::new(), (), ())
        .unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([child]), (), ()).unwrap();
    let tree = builder.finish(root).unwrap();
    let error = push_edited(&tree, |segment| {
        let nodes = tree_nodes_mut(segment);
        *map_field_mut(map_field_mut(&mut nodes[1], "span"), "end") = SerialValue::Int(4);
    });
    assert!(matches!(innermost(&error), DeserializeError::SpanNotOnCharBoundary { .. }), "{error}");
}

#[test]
fn a_chars_content_span_out_of_range_is_rejected() {
    // The node's own span is fine; the chars payload's byte range runs past the
    // source. The builder's residency check catches it, named at the node.
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let chars = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "chars");
        *map_field_mut(chars, "content") = SerialValue::Map(Vec::from([(
            String::from("spanned"),
            SerialValue::Map(Vec::from([
                (String::from("start"), SerialValue::Int(0)),
                (String::from("end"), SerialValue::Int(9_999)),
            ])),
        )]));
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, None)), "{error}");
    assert!(error.to_string().contains("chars content span 0..9999"), "{error}");
}

#[test]
fn a_group_delimiter_span_off_a_char_boundary_is_rejected() {
    // "café": a group open delimiter ending mid-é (é is two bytes at 3..5).
    let src = source("café");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let inner = builder
        .add(NodeKind::chars(Span::new(0, 3)), span(&src, 0..3), Arc::clone(&st), Vec::new(), (), ())
        .unwrap();
    let group = builder
        .add(
            NodeKind::group(GroupData::new(GT_BRACE, Span::new(0, 1), Span::new(1, 2))),
            span(&src, 0..5),
            Arc::clone(&st),
            Vec::from([inner]),
            (),
            (),
        )
        .unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([group]), (), ()).unwrap();
    let tree = builder.finish(root).unwrap();
    let error = push_edited(&tree, |segment| {
        let nodes = tree_nodes_mut(segment);
        let group = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "group");
        *map_field_mut(group, "open") = SerialValue::Map(Vec::from([(
            String::from("spanned"),
            SerialValue::Map(Vec::from([
                (String::from("start"), SerialValue::Int(0)),
                (String::from("end"), SerialValue::Int(4)),
            ])),
        )]));
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, None)), "{error}");
    assert!(error.to_string().contains("group open delimiter span 0..4"), "{error}");
}

#[test]
fn children_out_of_bounds_and_backwards_are_rejected() {
    let out = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        *map_field_mut(map_field_mut(&mut nodes[0], "children"), "end") = SerialValue::Int(4_000_000_000i64);
    });
    assert!(matches!(innermost(&out), DeserializeError::Failed { .. }), "{out}");

    let backwards = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let children = map_field_mut(&mut nodes[0], "children");
        *map_field_mut(children, "start") = SerialValue::Int(3);
        *map_field_mut(children, "end") = SerialValue::Int(1);
    });
    assert!(matches!(innermost(&backwards), DeserializeError::Failed { .. }), "{backwards}");
}

#[test]
fn a_duplicate_child_claim_is_rejected() {
    // Root children are [1..5]; node 3 (chars `z`) is a leaf. Give node 3 the group's
    // child range so a child index is claimed twice.
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let children = map_field_mut(&mut nodes[2], "children");
        *map_field_mut(children, "start") = SerialValue::Int(4);
        *map_field_mut(children, "end") = SerialValue::Int(5);
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
}

#[test]
fn a_region_that_does_not_tile_is_rejected() {
    let error = push_edited(&callable_group_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        // Blank the first argument's region so the child list is no longer tiled.
        *map_field_mut(&mut arguments[0], "region") = SerialValue::Null;
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
}

#[test]
fn an_unknown_kind_is_rejected() {
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        *map_field_mut(&mut nodes[0], "kind") = SerialValue::Str("mystery".into());
    });
    assert!(matches!(innermost(&error), DeserializeError::Value(_)), "{error}");
}

#[test]
fn a_wrong_ext_shape_is_rejected() {
    // The toy node ext is `()` (null); anything else is a type mismatch.
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        *map_field_mut(&mut nodes[0], "ext") = SerialValue::Int(1);
    });
    assert!(matches!(innermost(&error), DeserializeError::Value(_)), "{error}");
}

#[test]
fn an_unregistered_annotation_identifier_is_rejected() {
    let mut writer = setup::<ToyLang>();
    writer.serialize_tree(&chars_group_tree()).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    let trees = find_table_mut(&mut value, "trees");
    let entries = list_mut(map_field_mut(trees, "entries"));
    // A heterogeneous entry carries its identifier under the wire key `id`.
    *map_field_mut(&mut entries[0], "identifier") = SerialValue::Str("mystery.tree".into());
    let edited = Segment::from_serial_value(&value).unwrap();
    let mut reader = setup::<ToyLang>();
    let error = reader.push_segment(edited).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::UnknownIdentifier { table: "trees", .. }), "{error}");
}

#[test]
fn reading_a_tree_with_the_wrong_annotation_type_is_rejected() {
    let mut writer = setup_span_annotations();
    let position = writer.serialize_tree(&chars_group_tree()).unwrap();
    let mut reader = setup_span_annotations();
    reader.push_segment(writer.take_segment()).unwrap();
    let trees = reader.standard_tables().unwrap().trees;
    // The tree was written with the unit annotation; reading it as `SourceSpan` fails,
    // naming the identifier it was written under and the annotation requested.
    let error = reader.tree::<SpanAnnotation>(trees.position(position.index())).unwrap_err();
    assert!(matches!(error, DeserializeError::Failed { .. }), "{error}");
    let rendered = error.to_string();
    assert!(rendered.contains("serialized under `core.tree`"), "{rendered}");
    assert!(rendered.contains("SourceSpan") && rendered.contains("(`test.span`)"), "{rendered}");
}

#[test]
fn an_unexpected_argument_spec_payload_at_the_default_is_rejected() {
    let error = push_edited(&callable_group_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        if let SerialValue::Map(fields) = &mut arguments[0] {
            fields.push((String::from("spec_payload"), SerialValue::Str("surprise".into())));
        }
    });
    assert!(matches!(innermost(&error), DeserializeError::UnexpectedArgumentSpecPayload { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, Some("frac"))), "{error}");
}

#[test]
fn an_argument_index_past_the_spec_is_rejected() {
    // Append a third (absent) argument to a two-argument callable: the rebuilt spec
    // declares only two, so the default read errors on the extra one.
    let error = push_edited(&callable_group_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        arguments.push(SerialValue::Map(Vec::new()));
    });
    assert!(matches!(innermost(&error), DeserializeError::ArgumentIndexOutOfRange { .. }), "{error}");
}

#[test]
fn nodes_no_node_claims_are_rejected_not_dropped() {
    // Shorten the root's children range: the last nodes are then claimed by nobody.
    // The builder would drop them silently; the reader must refuse the tree instead.
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        assert_eq!(nodes.len(), 6);
        *map_field_mut(map_field_mut(&mut nodes[0], "children"), "end") = SerialValue::Int(3);
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    // Node 3 (chars `z`) is the first unclaimed node.
    assert_eq!(read_node_location(&error), Some((3, None)), "{error}");
    assert!(error.to_string().contains("no node lists this node among its children"), "{error}");
}

#[test]
fn the_root_claimed_as_a_child_is_rejected() {
    // A leaf's children range that reaches back to node 0.
    let error = push_edited(&chars_group_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let children = map_field_mut(&mut nodes[3], "children");
        *map_field_mut(children, "start") = SerialValue::Int(0);
        *map_field_mut(children, "end") = SerialValue::Int(1);
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((3, None)), "{error}");
}

#[test]
fn a_content_parent_outside_its_region_is_rejected() {
    // `\frac{a}{b}`: point argument 0's content parent at argument 1's group (node 3).
    // The builder catches it when the tree is finished; the message names the wire
    // node, and the builder's own error is kept as the cause.
    let error = push_edited(&callable_group_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        let region = map_field_mut(&mut arguments[0], "region");
        *map_field_mut(map_field_mut(map_field_mut(region, "content"), "in_children_of"), "node") = SerialValue::Int(3);
    });
    match innermost(&error) {
        DeserializeError::Failed { detail, cause } => {
            assert!(detail.contains("content parent node #3 lies outside its own argument/slot region"), "{detail}");
            let cause = cause.as_ref().expect("the builder error is kept as the cause");
            assert!(
                matches!(cause.downcast_ref::<NodeBuildError>(), Some(NodeBuildError::ContentParentOutsideRegion { .. })),
                "{cause}"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn a_content_parent_stored_before_its_callable_is_rejected() {
    // Point a region's `in_children_of` content parent at the root (an ancestor,
    // stored before the callable — a content parent is a descendant, stored after it).
    let content_parent_edit = |segment: &mut SerialValue, node: i64| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        let region = map_field_mut(&mut arguments[0], "region");
        *map_field_mut(map_field_mut(map_field_mut(region, "content"), "in_children_of"), "node") = SerialValue::Int(node);
    };
    let error = push_edited(&callable_group_args_tree(), |segment| content_parent_edit(segment, 0));
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, Some("frac"))), "{error}");
    assert!(error.to_string().contains("node #0, is not stored after its callable"), "{error}");

    // The callable itself (`in_children_of` naming the node whose region it is): the
    // same error — the wire form for content among the callable's own children is
    // `in_region`, and the callable is not a node inside its own region.
    let error = push_edited(&callable_group_args_tree(), |segment| content_parent_edit(segment, 1));
    assert!(error.to_string().contains("node #1, is not stored after its callable"), "{error}");

    // And one out of range altogether.
    let error = push_edited(&callable_group_args_tree(), |segment| content_parent_edit(segment, 99));
    assert!(error.to_string().contains("node #99, is out of range"), "{error}");
}

#[test]
fn a_region_content_of_an_unknown_form_is_rejected() {
    // The content designation is one of two variants, `in_region` / `in_children_of`.
    let error = push_edited(&callable_token_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        let region = map_field_mut(&mut arguments[0], "region");
        *map_field_mut(region, "content") =
            SerialValue::Map(Vec::from([(String::from("somewhere"), SerialValue::Map(Vec::new()))]));
    });
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::UnknownVariant { .. })), "{error}");
    // A shape error is caught while the whole entry is read, before any node is staged.
    assert!(matches!(error, DeserializeError::InEntry { table: "trees", .. }), "{error}");
}

#[test]
fn a_region_content_range_out_of_bounds_is_rejected() {
    let error = push_edited(&callable_token_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        let region = map_field_mut(&mut arguments[0], "region");
        *map_field_mut(map_field_mut(map_field_mut(region, "content"), "in_region"), "end") = SerialValue::Int(9);
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, Some("frac"))), "{error}");
}

#[test]
fn overlapping_regions_are_rejected() {
    // Two argument regions over the same child.
    let error = push_edited(&callable_token_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        let region = map_field_mut(&mut arguments[1], "region");
        *map_field_mut(map_field_mut(region, "children"), "start") = SerialValue::Int(0);
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, Some("frac"))), "{error}");
}

#[test]
fn an_ext_on_an_absent_argument_is_rejected() {
    // The third argument of `\frac 1 2` is absent; give it an ext anyway.
    let error = push_edited(&callable_token_args_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let arguments = list_mut(map_field_mut(callable, "arguments"));
        if let SerialValue::Map(fields) = &mut arguments[2] {
            fields.push((String::from("ext"), SerialValue::Int(7)));
        }
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert!(error.to_string().contains("argument #3 is absent"), "{error}");
}

#[test]
fn an_annotation_count_mismatch_is_rejected() {
    let src = source("hello");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, SpanAnnotation>::new();
    let child = builder
        .add(NodeKind::chars(Span::new(0, 5)), span(&src, 0..5), Arc::clone(&st), Vec::new(), (), span(&src, 0..3))
        .unwrap();
    let root =
        builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([child]), (), span(&src, 0..5)).unwrap();
    let tree = builder.finish(root).unwrap();
    let mut writer = setup_span_annotations();
    writer.serialize_tree(&tree).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    list_mut(map_field_mut(tree_entry_data_mut(&mut value), "annotations")).pop();
    let edited = Segment::from_serial_value(&value).unwrap();
    let mut reader = setup_span_annotations();
    let error = reader.push_segment(edited).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert!(error.to_string().contains("2 nodes but 1 annotations"), "{error}");
}

#[test]
fn a_non_tree_object_in_the_trees_table_is_a_write_error() {
    let mut session = setup::<ToyLang>();
    let trees = session.standard_tables().unwrap().trees;
    let object: Arc<dyn Any + Send + Sync> = Arc::new(42u32);
    let error = session.intern(trees, &object).unwrap_err();
    assert!(matches!(&error, SerializeError::InTable { table: "trees", .. }), "{error}");
    assert!(error.to_string().contains("not a NodeTree of a registered annotation type"), "{error}");
}

#[test]
fn a_deep_chain_round_trips_without_recursion() {
    // A 60_000-deep chain of `List` nodes: the writer and the reader must not recurse
    // per level.
    let src = source("d");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let mut current =
        builder.add(NodeKind::chars(Span::new(0, 1)), span(&src, 0..1), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    for _ in 0..60_000 {
        current = builder.add(NodeKind::list(), span(&src, 0..1), Arc::clone(&st), Vec::from([current]), (), ()).unwrap();
    }
    let tree = builder.finish(current).unwrap();
    let back = round_trip_tree(setup::<ToyLang>, &tree, ignore_annotations);
    assert_eq!(back.node_count(), 60_001);
}

#[test]
fn a_session_composed_from_empty_has_the_unit_annotation() {
    // The trees table registers `core.tree` itself: a session built from `empty()`
    // needs no further registration to serialize and read a `NodeTree<L>`.
    let compose = || {
        let mut session = SerdeSession::<ToyLang>::empty();
        session.register_table(crate::serialize::SourceSerdeDriver::new()).unwrap();
        session.register_table(crate::serialize::StateSerdeDriver::new()).unwrap();
        session.register_table(crate::serialize::SpecSerdeDriver::<ToyLang>::new("specs")).unwrap();
        session.register_table(crate::serialize::ProviderSerdeDriver::<ToyLang>::new("providers")).unwrap();
        session.register_table(crate::serialize::TreeSerdeDriver::<ToyLang>::new()).unwrap();
        session
    };
    round_trip_tree(compose, &chars_group_tree(), ignore_annotations);
}

// --- a toy-lang parse round trip ------------------------------------------------------

/// A provider resolving `\hi` (a zero-argument macro) and `\emph` (one group argument)
/// to shared toy specs; identity-resolved on read against the environment.
#[derive(Debug)]
struct ParseProvider {
    hi: Arc<ToySpec<ParseLang>>,
    emph: Arc<ToySpec<ParseLang>>,
}

impl ParseProvider {
    fn new() -> Arc<ParseProvider> {
        Arc::new(ParseProvider { hi: ToySpec::shared("hi", &[]), emph: Arc::new(emph_spec()) })
    }
}

fn emph_spec() -> ToySpec<ParseLang> {
    // One group argument; the parser is a real group parser so `\emph{x}` parses.
    let parser = crate::constructs::GroupArgumentParser::<ParseLang>::new(GT_BRACE);
    ToySpec {
        name: "emph".to_string(),
        arguments: Vec::from([Arc::new(ArgumentSpec::new(parser, "arg"))]),
        arg_names: Vec::from([String::from("arg")]),
    }
}

impl SpecsProvider<ParseLang> for ParseProvider {
    fn name(&self) -> &str {
        "parse"
    }

    fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, '_, ParseLang>,
        _state: &ParsingState<ParseLang>,
    ) -> Result<Option<Arc<dyn CallableSpec<ParseLang>>>, ProviderError> {
        Ok(match query.name {
            "hi" => Some(Arc::clone(&self.hi) as Arc<dyn CallableSpec<ParseLang>>),
            "emph" => Some(Arc::clone(&self.emph) as Arc<dyn CallableSpec<ParseLang>>),
            _ => None,
        })
    }
}

impl SerializableObject<ParseLang> for ParseProvider {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, ParseLang>) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry { identifier: "toy.parse-provider".into(), data: SerialValue::Str("parse".into()) })
    }
}

struct ParseEnvironment {
    provider: Arc<ParseProvider>,
}

impl DeserializableObject<ParseLang> for ParseProvider {
    type Output = Arc<dyn SpecsProvider<ParseLang>>;

    fn deserialize_object(
        _value: &SerialValue,
        cx: &mut DeserializeContext<'_, ParseLang>,
    ) -> Result<Self::Output, DeserializeError> {
        let env = cx.user_data::<ParseEnvironment>().ok_or_else(|| DeserializeError::failed("no parse environment"))?;
        Ok(Arc::clone(&env.provider) as Arc<dyn SpecsProvider<ParseLang>>)
    }
}

/// A language with `{}` groups, `\` commands, `%` comments, and whitespace, resolving
/// commands to the toy specs through its seed's scope stack.
#[derive(Debug, Clone, Copy)]
struct ParseLang;

impl Lang for ParseLang {
    type Features = crate::state::AllLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = StdParseDriver<ScopesCommandResolver<ParseLang>>;

    fn initial_state_data() -> Result<crate::state::StateData<Self>, crate::state::FinalizeError> {
        use crate::token::{
            CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
            GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
        };
        let rules = TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: Vec::from([Arc::new(GroupRule { group_type: GT_BRACE, open: "{".into(), close: "}".into() })]),
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: Vec::from([Arc::new(CommandRule {
                    escape_char: '\\',
                    name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
                })]),
            },
            comments: CommentRules { enabled: true, rules: Vec::from([Arc::new(CommentRule { start: "%".into() })]) },
            specials: SpecialsRules { enabled: false },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        };
        let mut scopes = crate::scopes::ScopeStack::new();
        scopes.push(ParseProvider::new() as Arc<dyn SpecsProvider<ParseLang>>);
        Ok(crate::state::StateData { rules, scopes, mode: (), ext: () })
    }

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Option<String>>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) -> Result<(), crate::node::NodeBuildError> {
        Ok(())
    }
}

impl SerializableLang for ParseLang {}

fn setup_parse() -> SerdeSession<ParseLang> {
    let mut session = SerdeSession::<ParseLang>::new();
    let tables = session.standard_tables().unwrap();
    tables
        .specs
        .register_type::<ToySpec<ParseLang>>(&mut session, "toy.spec", |spec| Arc::new(spec) as Arc<dyn CallableSpec<ParseLang>>)
        .unwrap();
    tables
        .providers
        .register_type::<ParseProvider>(&mut session, "toy.parse-provider", |provider| provider)
        .unwrap();
    session.set_user_data(ParseEnvironment { provider: ParseProvider::new() });
    session
}

#[test]
fn a_parsed_tree_round_trips() {
    let language = Language::new(
        StdParseDriver::new(Recovery::Tolerant, ScopesCommandResolver::<ParseLang> { command_type: CT_MACRO }),
        ParsingState::lang_initial().expect("seed"),
    );
    let result = language.parse("a{b}c\\hi \\emph{x}%comment\n").expect("parse");
    round_trip_tree(setup_parse, &result.tree, ignore_annotations);
}

// --- text inside language payloads is owned on the wire -----------------------------

/// A toy invocation syntax carrying one text field, span-backed when it comes from a
/// parse (like a macro's post-space); its value conversion is `TextContent`'s.
#[derive(Debug, Clone)]
struct ToySyntax {
    post_space: TextContent,
}

impl<L: Lang> InvocationSyntax<L> for ToySyntax {
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self {
        ToySyntax { post_space: self.post_space.materialized(source) }
    }
}

impl<L: Lang> SerializableValue<L> for ToySyntax {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        self.post_space.serialize_value(cx)
    }
}

impl<L: Lang> DeserializableValue<L> for ToySyntax {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        Ok(ToySyntax { post_space: <TextContent as DeserializableValue<L>>::deserialize_value(value, cx)? })
    }
}

/// The toy lang whose invocation syntax is [`ToySyntax`] (everything else defaulted).
#[derive(Debug, Clone, Copy)]
struct SyntaxLang;

impl Lang for SyntaxLang {
    type Features = crate::state::AllLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type InvocationSyntax = ToySyntax;
    type Driver = StdParseDriver;

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Option<String>>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
    }
}

impl SerializableLang for SyntaxLang {}

/// `\x  y`: a zero-argument macro whose invocation syntax records its post-space
/// (`2..4`) as a span into the node's source, then a chars node.
fn spanned_syntax_tree() -> NodeTree<SyntaxLang, ()> {
    let src = source("\\x  y");
    let st = Arc::new(ParsingState::<SyntaxLang>::lang_initial().expect("seed state"));
    let spec: Arc<ToySpec<SyntaxLang>> = ToySpec::shared("x", &[]);
    let mut builder = NodeTreeBuilder::<SyntaxLang, ()>::new();
    let callable = CallableData {
        callable_type: CT_MACRO,
        name: "x".into(),
        spec: spec as Arc<dyn CallableSpec<SyntaxLang>>,
        arguments: ParsedArguments::empty(),
        slots: ParsedSlots::empty(),
        invocation_syntax: ToySyntax { post_space: TextContent::Spanned(Span::new(2, 4)) },
    };
    let node =
        builder.add(NodeKind::callable(callable), span(&src, 0..4), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let y = builder.add(NodeKind::chars(Span::new(4, 5)), span(&src, 4..5), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([node, y]), (), ()).unwrap();
    builder.finish(root).unwrap()
}

#[test]
fn a_spanned_invocation_syntax_round_trips_as_owned_text() {
    // The writer materializes the invocation syntax against the node's source: the
    // rebuilt syntax carries the same text, owned; the deep-compare (materialized
    // invocation syntaxes) passes.
    let tree = spanned_syntax_tree();
    let back = round_trip_tree(setup::<SyntaxLang>, &tree, ignore_annotations);
    let NodeKind::Callable(data) = &back.nodes()[1].kind else { panic!("node 1 is the callable") };
    assert!(data.invocation_syntax.post_space.is_owned());
    assert_eq!(data.invocation_syntax.post_space.resolve(back.nodes()[1].span.source()), "  ");

    // On the wire: owned text.
    let mut writer = setup::<SyntaxLang>();
    writer.serialize_tree(&tree).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    let nodes = tree_nodes_mut(&mut value);
    let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
    assert_eq!(
        map_field_mut(callable, "invocation_syntax"),
        &SerialValue::Map(Vec::from([(String::from("owned"), SerialValue::Str("  ".into()))]))
    );
}

#[test]
fn spanned_text_inside_an_invocation_syntax_is_rejected_on_read() {
    // A hostile segment puts `{spanned: {start, end}}` where the payload's text goes —
    // with a range no source could hold. The value conversion has no node to validate
    // the range against, so it refuses: a typed error naming the node, no panic (the
    // range would panic later, in `TextContent::resolve`, if it were accepted).
    let error = push_edited_in(setup::<SyntaxLang>, &spanned_syntax_tree(), |segment| {
        let nodes = tree_nodes_mut(segment);
        let callable = map_field_mut(map_field_mut(&mut nodes[1], "kind"), "callable");
        let range = SerialValue::Map(Vec::from([
            (String::from("start"), SerialValue::Int(0)),
            (String::from("end"), SerialValue::Int(9_999)),
        ]));
        *map_field_mut(callable, "invocation_syntax") = SerialValue::Map(Vec::from([(String::from("spanned"), range)]));
    });
    assert!(matches!(innermost(&error), DeserializeError::Failed { .. }), "{error}");
    assert_eq!(read_node_location(&error), Some((1, Some("x"))), "{error}");
    assert!(error.to_string().contains("span-backed text"), "{error}");
}

#[test]
fn a_spanned_text_value_is_a_write_error() {
    // `TextContent`'s value conversion refuses span-backed text on the write side too:
    // a tree annotated with `TextContent` values (the annotation codec is the value
    // conversion) fails to serialize at the node whose annotation is `Spanned`.
    let src = source("hello");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, TextContent>::new();
    let leaf = TextContent::from("leaf");
    let child =
        builder.add(NodeKind::chars(Span::new(0, 5)), span(&src, 0..5), Arc::clone(&st), Vec::new(), (), leaf).unwrap();
    let spanned = TextContent::Spanned(Span::new(0, 5));
    let root = builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([child]), (), spanned).unwrap();
    let tree = builder.finish(root).unwrap();

    let mut writer = setup::<ToyLang>();
    let trees = writer.standard_tables().unwrap().trees;
    trees.register_annotation::<TextContent>(&mut writer, "test.text").unwrap();
    let error = writer.serialize_tree(&tree).unwrap_err();
    assert!(matches!(innermost_write(&error), SerializeError::Failed { .. }), "{error}");
    assert_eq!(write_node_location(&error), Some((0, None)), "{error}");
    assert!(error.to_string().contains("span-backed text"), "{error}");
}

// --- JSON snapshot (feature) ----------------------------------------------------------

#[cfg(feature = "serde")]
#[test]
fn a_small_tree_has_a_pinned_json_rendering() {
    let src = source("hi");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, ()>::new();
    let child = builder.add(NodeKind::chars(Span::new(0, 2)), span(&src, 0..2), Arc::clone(&st), Vec::new(), (), ()).unwrap();
    let root = builder.add(NodeKind::list(), span(&src, 0..2), st, Vec::from([child]), (), ()).unwrap();
    let tree = builder.finish(root).unwrap();

    let mut writer = setup::<ToyLang>();
    writer.serialize_tree(&tree).unwrap();
    let segment = writer.take_segment();
    let entry = &segment.tables().iter().find(|t| t.name() == "trees").unwrap().entries()[0];
    let json = serde_json::to_string(&crate::serialize::to_value(entry).unwrap()).unwrap();
    assert_eq!(
        json,
        r#"{"identifier":"core.tree","data":{"nodes":[{"kind":"list","span":{"source":{"$index":[0,0]},"start":0,"end":2},"state":{"$index":[1,0]},"ext":null,"children":{"start":1,"end":2}},{"kind":{"chars":{"content":{"spanned":{"start":0,"end":2}}}},"span":{"source":{"$index":[0,0]},"start":0,"end":2},"state":{"$index":[1,0]},"ext":null,"children":{"start":2,"end":2}}]}}"#
    );
}

// --- serde-bridge annotations (feature) ----------------------------------------------

#[cfg(feature = "serde")]
#[test]
fn a_tree_of_plain_data_annotations_round_trips_through_the_bridge() {
    // A plain-data annotation (`String`) registered through the serde bridge.
    let src = source("hello");
    let st = state();
    let mut builder = NodeTreeBuilder::<ToyLang, String>::new();
    let child = builder
        .add(NodeKind::chars(Span::new(0, 5)), span(&src, 0..5), Arc::clone(&st), Vec::new(), (), String::from("leaf"))
        .unwrap();
    let root =
        builder.add(NodeKind::list(), span(&src, 0..5), st, Vec::from([child]), (), String::from("root")).unwrap();
    let tree = builder.finish(root).unwrap();

    let setup_bridge = || {
        let mut session = setup::<ToyLang>();
        let trees = session.standard_tables().unwrap().trees;
        trees.register_serde_annotation::<String>(&mut session, "test.label").unwrap();
        session
    };
    round_trip_tree(setup_bridge, &tree, |a: &String, b: &String| a == b);
}
