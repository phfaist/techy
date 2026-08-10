//! [`DescentGuard`]: the per-run limiter of recursion depth — for parses and for
//! the tree traversals.
//!
//! Parsing is recursive: a construct parser that needs a sub-construct parsed (a
//! group interior, an argument, an environment body) runs another construct parser
//! over the same input. One such step is a **descent**, and every descent goes
//! through the single entry point
//! [`ParseContext::parse_construct`](crate::constructs::ParseContext::parse_construct).
//! Each descent consumes call stack, so input nested deeply enough — pathological or
//! hostile — would otherwise crash the process by exhausting the stack.
//!
//! The guard is the defense: a small per-run object that is asked before every
//! descent whether the run may go one level deeper. A refusal aborts the parse
//! with an ordinary error
//! ([`DescentLimitExceeded`](crate::constructs::DescentLimitExceeded)) instead of
//! crashing — under any recovery policy, since past the limit there is no safe way
//! to continue.
//!
//! The consumer traversals are bounded the same way: trees built by hand through
//! a [`NodeTreeBuilder`](crate::node::NodeTreeBuilder) can be deeper than any
//! parse-side limit, so the traversal drivers
//! ([`TreeWalker`](crate::visit::TreeWalker),
//! [`TreeRestager`](crate::transform::TreeRestager),
//! [`TreeRecomposer`](crate::recompose::TreeRecomposer)) each create their own
//! per-run guard, configured through their own `with_descent_guard_init`. A
//! traversal refusal surfaces as the run's `DescentLimitExceeded`-style error
//! value, and the guard's early warning reaches the run's own visitor
//! (its defaulted `observe_descent_warning` hook). Traversal descents track the
//! tree directly: one descent per nesting level (a parse costs about two per
//! syntactic level).
//!
//! Every guarded run uses the standard implementation, [`StdDescentGuard`]; its
//! *configuration* ([`StdDescentGuardInit`]) travels on the run's long-lived
//! entry value — the [`Language`](super::Language)
//! ([`with_descent_guard_init`](super::Language::with_descent_guard_init)) or the
//! traversal driver — and a
//! fresh guard instance is created for every run, on the running thread, at
//! run entry. The [`DescentGuard`] trait states the contract the engine drives
//! the guard through; wiring in a different implementation is deliberately not
//! offered for now.

use alloc::format;
use alloc::string::String;

/// The per-run limiter of recursion depth — asked before every descent whether
/// the run may go one level deeper.
///
/// One guard value is created per run: a parse's lives on the
/// [`ParserSession`](super::ParserSession) and is driven from the single
/// descent entry point,
/// [`ParseContext::parse_construct`](crate::constructs::ParseContext::parse_construct);
/// a traversal's is created by its driver
/// ([`TreeWalker`](crate::visit::TreeWalker) and its transform/recompose
/// siblings) and wraps the per-node recursion. Either way:
/// [`try_enter`](DescentGuard::try_enter) before the descent runs,
/// [`exit`](DescentGuard::exit) after it returns (on the success and error paths
/// alike — errors are returned values, not unwinds). A refusal aborts the
/// run — for a parse, under any recovery policy — and the refused descent never
/// gets a matching `exit` call.
///
/// Implementations are plain values — no `Send`/`Sync` requirement on the guard
/// itself (it never leaves its run's thread); the configuration
/// ([`InitArg`](DescentGuard::InitArg)) is shared on the long-lived entry value
/// ([`Language`](super::Language), a traversal driver) and must be `Send + Sync`.
pub trait DescentGuard: Sized {
    /// The guard's configuration, stored on the run's entry value (set with its
    /// `with_descent_guard_init` method — on [`Language`](super::Language) and
    /// the traversal drivers alike).
    /// The `Default` value is the fallback used when no configuration was given —
    /// including on the hand-built-context path, where the session creates the
    /// guard lazily at the first descent.
    type InitArg: Default + Send + Sync;

    /// Create the per-run guard from its configuration. Runs at run entry, on
    /// the thread that runs — an implementation measuring the call stack takes
    /// its reference measurement here.
    fn init(arg: &Self::InitArg) -> Self;

    /// Ask whether the run may descend one level deeper.
    ///
    /// - `Ok(None)`: descend.
    /// - `Ok(Some(warning))`: descend, and surface `warning` — a parse records a
    ///   warning-severity diagnostic
    ///   ([`DescentLimitApproaching`](crate::constructs::DescentLimitApproaching)),
    ///   a traversal notifies its visitor (`observe_descent_warning`).
    /// - `Err(refusal)`: do not descend — a parse aborts with
    ///   [`DescentLimitExceeded`](crate::constructs::DescentLimitExceeded) under
    ///   any recovery policy, a traversal with its own
    ///   `DescentLimitExceeded`-style error value.
    ///   [`exit`](DescentGuard::exit) is **not** called for a refused descent.
    fn try_enter(&mut self) -> Result<Option<DescentWarning>, DescentRefusal>;

    /// The matching end of a granted [`try_enter`](DescentGuard::try_enter): the
    /// descent returned (successfully or with an error) and the run is one
    /// level shallower again.
    fn exit(&mut self);
}

/// A guard's answer "do not descend": the run aborts — a parse with
/// [`DescentLimitExceeded`](crate::constructs::DescentLimitExceeded), a traversal
/// with its own `DescentLimitExceeded`-style error value — carrying
/// `detail` as its message body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescentRefusal {
    /// Human-readable explanation of which limit was hit and how to configure it.
    pub detail: String,
}

/// A guard's early warning "descend, but the limit is getting close": a parse
/// records it as a warning-severity
/// [`DescentLimitApproaching`](crate::constructs::DescentLimitApproaching)
/// diagnostic, a traversal hands it to its visitor's `observe_descent_warning`
/// hook; the run continues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescentWarning {
    /// Human-readable explanation of the approaching limit and how to configure it.
    pub detail: String,
}

/// The configuration of [`StdDescentGuard`] — one of four modes, built with the
/// constructors below (the modes are named `FixedStackBudget`,
/// `ComputedStackBudget`, `DepthLimit`, and `Off`; construction goes through the
/// constructors because a default-constructed value additionally carries a private
/// "no configuration was given" mark that an openly constructable value could not).
///
/// The `Default` value is a [`FixedStackBudget`](StdDescentGuardInit::fixed_stack_budget)
/// of [`StdDescentGuard::DEFAULT_STACK_BUDGET`] bytes, marked as unconfigured:
/// under it the guard additionally emits the one-time
/// [`DescentLimitApproaching`](crate::constructs::DescentLimitApproaching) warning
/// at half the budget, and its refusal message points at the
/// `with_descent_guard_init` configuration method (on
/// [`Language`](super::Language) and the traversal drivers alike).
/// The default budget is deliberately tight — in unoptimized (debug) builds it
/// allows only on the order of ten parse nesting levels — so that a deep run
/// under the untuned default fails early, with a message naming the
/// configuration entry point, instead of consuming an unknown amount of stack.
///
/// # Which mode to choose
///
/// - [`fixed_stack_budget`](StdDescentGuardInit::fixed_stack_budget): cap the
///   *estimated stack use* of parsing at a byte count you choose. Robust across
///   build profiles in the sense that the cap is on actual consumption, not on a
///   level count — but the same input reaches the cap at different nesting depths
///   in debug and release builds.
/// - [`computed_stack_budget`](StdDescentGuardInit::computed_stack_budget): derive
///   the cap from the actually remaining stack space at parse entry, measured by a
///   probe function you supply (the crate is `no_std` and cannot ask the operating
///   system itself). This is the mode that uses the whole available stack safely.
/// - [`depth_limit`](StdDescentGuardInit::depth_limit): cap the *number* of
///   simultaneously open descents. In a parse the count is engine descents, not
///   syntactic nesting levels: one nesting level of the input costs about two
///   descents (the group's own descent plus the content run over its interior),
///   so a format policy ("no document nests deeper than N") needs a limit of
///   roughly `2 * N`. A tree traversal costs exactly one descent per tree
///   nesting level. Fully deterministic across platforms and build profiles —
///   the right mode for tests — but says nothing about actual stack use.
/// - [`off`](StdDescentGuardInit::off): no limit. Deeply nested input can then
///   crash the process by stack exhaustion; only for callers that bound their
///   input by other means.
///
/// # The `ComputedStackBudget` probe, and `HEADROOM`
///
/// The probe reports how many bytes of stack are still available on the current
/// thread, or `None` if it cannot tell (then the guard falls back to
/// [`DEFAULT_STACK_BUDGET`](StdDescentGuard::DEFAULT_STACK_BUDGET)). The budget is
/// resolved once, at parse entry: `probe() - HEADROOM` (saturating at zero) — the
/// [`HEADROOM`](StdDescentGuard::HEADROOM) subtraction reserves room for the work
/// that happens *between* two descent checks and for the error path after a
/// refusal, so the refusal itself still has stack to run on. The headroom is
/// library-owned: only the guard subtracts it, and **only in this mode** — a
/// [`fixed_stack_budget`](StdDescentGuardInit::fixed_stack_budget) is a
/// consumption cap chosen by the caller, not a physical-stack measurement, so no
/// headroom is subtracted from it.
///
/// The easiest probe is the `stacker` crate (std-side; copy-paste):
///
/// ```ignore
/// // [dependencies] stacker = "0.1"
/// let language = language.with_descent_guard_init(
///     StdDescentGuardInit::computed_stack_budget(|| stacker::remaining_stack()),
/// );
/// ```
///
/// Doing it yourself instead: on Linux, `pthread_getattr_np` +
/// `pthread_attr_getstack` give the current thread's stack base and size, from
/// which the remaining distance to the current stack pointer follows; on Windows,
/// `GetCurrentThreadStackLimits` returns the two bounds directly. The probe runs
/// on the parsing thread, once per parse.
#[derive(Debug, Clone, Copy)]
pub struct StdDescentGuardInit {
    mode: InitMode,
    /// True only for `Default`-constructed values: drives the one-time
    /// half-budget warning and the refusal text that points at the configuration
    /// entry point.
    unconfigured: bool,
}

/// The four configuration modes (private carrier; see the constructors on
/// [`StdDescentGuardInit`]).
#[derive(Debug, Clone, Copy)]
enum InitMode {
    FixedStackBudget { bytes: usize },
    ComputedStackBudget { probe: fn() -> Option<usize> },
    DepthLimit { levels: usize },
    Off,
}

impl StdDescentGuardInit {
    /// `FixedStackBudget`: refuse a descent once parsing's estimated stack use
    /// exceeds `bytes`. No headroom is subtracted — the number is used as given
    /// (see the type docs for the contrast with
    /// [`computed_stack_budget`](StdDescentGuardInit::computed_stack_budget)).
    pub fn fixed_stack_budget(bytes: usize) -> StdDescentGuardInit {
        StdDescentGuardInit {
            mode: InitMode::FixedStackBudget { bytes },
            unconfigured: false,
        }
    }

    /// `ComputedStackBudget`: resolve the budget at parse entry from the stack
    /// space `probe` reports as still available, minus
    /// [`StdDescentGuard::HEADROOM`] (saturating at zero); a probe answer of
    /// `None` falls back to [`StdDescentGuard::DEFAULT_STACK_BUDGET`]. See the
    /// type docs for a copy-paste probe.
    pub fn computed_stack_budget(probe: fn() -> Option<usize>) -> StdDescentGuardInit {
        StdDescentGuardInit {
            mode: InitMode::ComputedStackBudget { probe },
            unconfigured: false,
        }
    }

    /// `DepthLimit`: refuse the descent that would open more than `levels`
    /// simultaneously open descents. In a parse, `levels` counts engine
    /// descents, not syntactic nesting levels: one nesting level of the input
    /// costs about two descents (the group's own descent plus the content run
    /// over its interior), so a limit aimed at a syntactic nesting depth needs
    /// roughly twice that number; a tree traversal costs exactly one descent per
    /// tree nesting level. Deterministic across platforms and build profiles;
    /// counts descents, not bytes.
    pub fn depth_limit(levels: usize) -> StdDescentGuardInit {
        StdDescentGuardInit { mode: InitMode::DepthLimit { levels }, unconfigured: false }
    }

    /// `Off`: no limit — deeply nested input can crash the process by stack
    /// exhaustion. Only for callers that bound their input by other means.
    pub fn off() -> StdDescentGuardInit {
        StdDescentGuardInit { mode: InitMode::Off, unconfigured: false }
    }
}

/// The built-in default: a fixed stack budget of
/// [`StdDescentGuard::DEFAULT_STACK_BUDGET`] bytes, marked as unconfigured (the
/// mark drives the one-time half-budget warning and the self-describing refusal
/// text — see the type docs).
impl Default for StdDescentGuardInit {
    fn default() -> StdDescentGuardInit {
        StdDescentGuardInit {
            mode: InitMode::FixedStackBudget { bytes: StdDescentGuard::DEFAULT_STACK_BUDGET },
            unconfigured: true,
        }
    }
}

/// The standard [`DescentGuard`]: byte-budget, depth-limit, and off modes,
/// configured through [`StdDescentGuardInit`] (see its docs for choosing a mode).
///
/// The byte-budget modes **estimate** stack use by address distance: at
/// [`init`](DescentGuard::init) the guard records the address of a local variable
/// (the reference point), and each [`try_enter`](DescentGuard::try_enter) measures
/// the distance from that reference point to a fresh local's address. The estimate
/// tracks real consumption closely (the stack is contiguous on all supported
/// targets) without any operating-system calls, which the `no_std` crate could not
/// make.
#[derive(Debug)]
pub struct StdDescentGuard {
    mode: GuardMode,
    /// Carried over from the init's private mark (see [`StdDescentGuardInit`]).
    unconfigured: bool,
    /// Latch for the one-time half-budget warning.
    warned: bool,
}

/// The resolved per-parse mode (budget already computed, reference point taken).
#[derive(Debug)]
enum GuardMode {
    /// The two byte-budget configurations, resolved: refuse when the measured
    /// stack distance from `anchor` exceeds `budget`.
    Measured { anchor: usize, budget: usize },
    /// Refuse the descent that would exceed `limit` simultaneously open levels.
    Depth { limit: usize, current: usize },
    /// No limit.
    Off,
}

impl StdDescentGuard {
    /// The built-in default stack budget (bytes), used when nothing was configured
    /// and as the fallback when a
    /// [`computed_stack_budget`](StdDescentGuardInit::computed_stack_budget) probe
    /// answers `None`. Deliberately tight — in unoptimized (debug) builds it
    /// allows only on the order of ten nesting levels — so that untuned deep
    /// parses fail early, with a message that points at the configuration entry
    /// point
    /// ([`Language::with_descent_guard_init`](super::Language::with_descent_guard_init)),
    /// instead of consuming an unknown amount of stack.
    pub const DEFAULT_STACK_BUDGET: usize = 250 * 1024;

    /// The stack reserve (bytes) subtracted from a
    /// [`computed_stack_budget`](StdDescentGuardInit::computed_stack_budget)
    /// probe's answer — room for the work between two descent checks and for the
    /// error path after a refusal, so the refusal itself still has stack to run
    /// on. Library-owned, applied only in that mode; a
    /// [`fixed_stack_budget`](StdDescentGuardInit::fixed_stack_budget) is used as
    /// given (see [`StdDescentGuardInit`]).
    pub const HEADROOM: usize = 64 * 1024;
}

/// The address of a local variable in this function's own frame — the guard's
/// stack-position measurement.
#[inline(never)]
fn stack_position() -> usize {
    let marker = 0u8;
    core::hint::black_box(&marker as *const u8 as usize)
}

impl DescentGuard for StdDescentGuard {
    type InitArg = StdDescentGuardInit;

    fn init(arg: &StdDescentGuardInit) -> StdDescentGuard {
        let mode = match arg.mode {
            InitMode::FixedStackBudget { bytes } => {
                GuardMode::Measured { anchor: stack_position(), budget: bytes }
            }
            InitMode::ComputedStackBudget { probe } => {
                // Resolved once, at parse entry: the probe's answer minus the
                // library-owned headroom (only this mode — see the init docs).
                let budget = match probe() {
                    Some(remaining) => remaining.saturating_sub(StdDescentGuard::HEADROOM),
                    None => StdDescentGuard::DEFAULT_STACK_BUDGET,
                };
                GuardMode::Measured { anchor: stack_position(), budget }
            }
            InitMode::DepthLimit { levels } => GuardMode::Depth { limit: levels, current: 0 },
            InitMode::Off => GuardMode::Off,
        };
        StdDescentGuard { mode, unconfigured: arg.unconfigured, warned: false }
    }

    fn try_enter(&mut self) -> Result<Option<DescentWarning>, DescentRefusal> {
        match &mut self.mode {
            GuardMode::Off => Ok(None),
            GuardMode::Depth { limit, current } => {
                if *current >= *limit {
                    return Err(DescentRefusal {
                        detail: format!(
                            "the run is {} descent levels deep, the configured \
                             depth limit (StdDescentGuardInit::depth_limit)",
                            limit
                        ),
                    });
                }
                *current += 1;
                Ok(None)
            }
            GuardMode::Measured { anchor, budget } => {
                let consumed = anchor.abs_diff(stack_position());
                if consumed > *budget {
                    let detail = if self.unconfigured {
                        format!(
                            "the run's estimated stack use ({} bytes) exceeds the \
                             built-in default budget of {} bytes \
                             (StdDescentGuard::DEFAULT_STACK_BUDGET); no descent-guard \
                             configuration was given — choose a limit explicitly with \
                             with_descent_guard_init (on Language, TreeWalker, \
                             TreeRestager, or TreeRecomposer)",
                            consumed,
                            StdDescentGuard::DEFAULT_STACK_BUDGET
                        )
                    } else {
                        format!(
                            "the run's estimated stack use ({} bytes) exceeds the \
                             configured budget of {} bytes",
                            consumed, budget
                        )
                    };
                    return Err(DescentRefusal { detail });
                }
                // The one-time early warning: only under the unconfigured
                // built-in default, latched at the first descent past half the
                // budget.
                if self.unconfigured && !self.warned && consumed > *budget / 2 {
                    self.warned = true;
                    return Ok(Some(DescentWarning {
                        detail: format!(
                            "the run's estimated stack use ({} bytes) has passed half \
                             of the built-in default budget of {} bytes \
                             (StdDescentGuard::DEFAULT_STACK_BUDGET); no descent-guard \
                             configuration was given — deeper input will be refused; \
                             choose a limit explicitly with with_descent_guard_init \
                             (on Language, TreeWalker, TreeRestager, or \
                             TreeRecomposer)",
                            consumed,
                            StdDescentGuard::DEFAULT_STACK_BUDGET
                        ),
                    }));
                }
                Ok(None)
            }
        }
    }

    fn exit(&mut self) {
        if let GuardMode::Depth { current, .. } = &mut self.mode {
            *current = current.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recurse `levels` deep, calling `try_enter` at each level with an inflated
    /// frame (at least `PAD` bytes), counting warnings; unwind through `exit` on
    /// the way back. Returns the refusal, if one was hit, and how many levels were
    /// granted before it.
    const PAD: usize = 8 * 1024;

    fn descend(
        guard: &mut StdDescentGuard,
        levels: usize,
        granted: &mut usize,
        warnings: &mut usize,
    ) -> Result<(), DescentRefusal> {
        let pad = [0u8; PAD];
        core::hint::black_box(&pad);
        match guard.try_enter() {
            Ok(Some(_)) => *warnings += 1,
            Ok(None) => {}
            Err(refusal) => return Err(refusal),
        }
        *granted += 1;
        let result = if levels > 1 {
            descend(guard, levels - 1, granted, warnings)
        } else {
            Ok(())
        };
        guard.exit();
        result
    }

    #[test]
    fn the_unconfigured_default_warns_once_at_half_budget_then_refuses() {
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
        let (mut granted, mut warnings) = (0, 0);
        let refusal = descend(&mut guard, 200, &mut granted, &mut warnings).unwrap_err();
        // With >= 8 KiB per level the 250 KiB default is exhausted well before 200
        // levels, and the half-budget warning latched exactly once on the way.
        assert_eq!(warnings, 1, "the half-budget warning latches once");
        assert!(granted < 200);
        assert!(refusal.detail.contains("DEFAULT_STACK_BUDGET"));
        assert!(refusal.detail.contains("with_descent_guard_init"));
    }

    #[test]
    fn a_configured_fixed_budget_never_warns_and_names_its_budget() {
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::fixed_stack_budget(
            StdDescentGuard::DEFAULT_STACK_BUDGET,
        ));
        let (mut granted, mut warnings) = (0, 0);
        let refusal = descend(&mut guard, 200, &mut granted, &mut warnings).unwrap_err();
        assert_eq!(warnings, 0, "the warning fires only under the unconfigured default");
        assert!(refusal.detail.contains("configured budget"));
        assert!(!refusal.detail.contains("with_descent_guard_init"));
    }

    #[test]
    fn a_computed_budget_subtracts_the_headroom_from_the_probe_answer() {
        // The probe answers HEADROOM + 1000: the resolved budget is 1000 bytes,
        // which the first padded level already exceeds — and the refusal names it.
        fn probe() -> Option<usize> {
            Some(StdDescentGuard::HEADROOM + 1000)
        }
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::computed_stack_budget(probe));
        let (mut granted, mut warnings) = (0, 0);
        let refusal = descend(&mut guard, 10, &mut granted, &mut warnings).unwrap_err();
        assert!(
            refusal.detail.contains("configured budget of 1000 bytes"),
            "refusal names the headroom-subtracted budget: {}",
            refusal.detail
        );
        assert_eq!(warnings, 0);
    }

    #[test]
    fn a_computed_budget_saturates_at_zero_and_falls_back_on_none() {
        // A probe answering less than the headroom saturates to a zero budget:
        // the very first descent is refused.
        fn tiny() -> Option<usize> {
            Some(StdDescentGuard::HEADROOM - 1)
        }
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::computed_stack_budget(tiny));
        let (mut granted, mut warnings) = (0, 0);
        let refusal = descend(&mut guard, 2, &mut granted, &mut warnings).unwrap_err();
        assert!(refusal.detail.contains("configured budget of 0 bytes"));
        assert_eq!(granted, 0);

        // A probe answering None falls back to the built-in default budget —
        // configured (no warning, no pointer at the configuration entry point).
        fn unknown() -> Option<usize> {
            None
        }
        let mut guard =
            StdDescentGuard::init(&StdDescentGuardInit::computed_stack_budget(unknown));
        let (mut granted, mut warnings) = (0, 0);
        let refusal = descend(&mut guard, 200, &mut granted, &mut warnings).unwrap_err();
        assert!(refusal.detail.contains(&format!(
            "configured budget of {} bytes",
            StdDescentGuard::DEFAULT_STACK_BUDGET
        )));
        assert_eq!(warnings, 0);
    }

    #[test]
    fn depth_limit_counts_open_levels_and_exit_rebalances() {
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::depth_limit(3));
        // Three nested levels pass, the fourth is refused.
        let (mut granted, mut warnings) = (0, 0);
        assert!(descend(&mut guard, 3, &mut granted, &mut warnings).is_ok());
        assert_eq!(granted, 3);
        let refusal = descend(&mut guard, 4, &mut granted, &mut warnings).unwrap_err();
        assert!(refusal.detail.contains("3 descent levels deep"));
        assert!(refusal.detail.contains("depth_limit"));
        // The unwound exits rebalanced the count: three levels pass again.
        let (mut granted, mut warnings) = (0, 0);
        assert!(descend(&mut guard, 3, &mut granted, &mut warnings).is_ok());
        assert_eq!(warnings, 0);
    }

    #[test]
    fn off_grants_everything() {
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::off());
        let (mut granted, mut warnings) = (0, 0);
        assert!(descend(&mut guard, 64, &mut granted, &mut warnings).is_ok());
        assert_eq!((granted, warnings), (64, 0));
    }
}
