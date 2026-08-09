// Normalization prototype for the LangHas* subtrait spelling (PLAN.md M2).
// Question: with `trait LangHasGroups: Lang where Self::Features:
// LangFeatures<Groups = FeaturePresent> {}`, does `L: LangHasGroups` let
// generic code write PLAIN struct literals where the field type is
// `<<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<T>`
// (i.e. does the equality propagate + normalize Store<T> to T)?

trait FeaturePresence: 'static {
    const PRESENT: bool;
    type Store<T: Clone + std::fmt::Debug>: Clone + std::fmt::Debug;
}

struct FeaturePresent;
impl FeaturePresence for FeaturePresent {
    const PRESENT: bool = true;
    type Store<T: Clone + std::fmt::Debug> = T;
}

struct FeatureAbsent;
impl FeaturePresence for FeatureAbsent {
    const PRESENT: bool = false;
    type Store<T: Clone + std::fmt::Debug> = std::marker::PhantomData<T>;
}

trait LangFeatures: 'static {
    type Groups: FeaturePresence;
    type Commands: FeaturePresence;
}

trait Lang: 'static {
    type Features: LangFeatures;
}

// --- preferred spelling under test ---
trait LangHasGroups: Lang<Features: LangFeatures<Groups = FeaturePresent>> {}
impl<L: Lang> LangHasGroups for L where L::Features: LangFeatures<Groups = FeaturePresent> {}

// M3-shaped storage: the field type routes through the GAT.
struct GroupRules<L: Lang> {
    rules: <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<Vec<u32>>,
}

// Load-bearing check 1: bounded generic code writes a PLAIN literal.
fn make_rules<L: LangHasGroups>() -> GroupRules<L> {
    GroupRules { rules: vec![1, 2, 3] }
}

// Load-bearing check 2: bounded generic code READS through the equality.
fn read_rules<L: LangHasGroups>(g: &GroupRules<L>) -> usize {
    g.rules.len()
}

// Load-bearing check 3: const guard usable in unbounded generic code.
fn guarded<L: Lang>() -> bool {
    <L::Features as LangFeatures>::Groups::PRESENT
}

// Concrete all-present bundle + language, exercising the plain-literal path
// for concrete code too (transparent Present store).
struct AllLangFeatures;
impl LangFeatures for AllLangFeatures {
    type Groups = FeaturePresent;
    type Commands = FeaturePresent;
}
struct TestLang;
impl Lang for TestLang {
    type Features = AllLangFeatures;
}

fn main() {
    let concrete = GroupRules::<TestLang> { rules: vec![7] };
    let generic = make_rules::<TestLang>();
    assert_eq!(read_rules(&concrete), 1);
    assert_eq!(read_rules(&generic), 3);
    assert!(guarded::<TestLang>());
    println!("normalization OK: supertrait where-clause equality propagates");
}
