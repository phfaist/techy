#!/bin/sh
# API-stability guard (dev-docs/DESIGN_RATIONALE.md [§dd-dr:stability-rubric]):
# compare techy's public surface against the frozen semver baseline.
#
# Baseline: the `api-baseline` git branch — pointed at the commit where the
# API-review Phase 3 restructuring landed; move it deliberately at each version
# bump (0.x discipline: breaking -> 0.(x+1).0, then
# `git branch -f api-baseline <release-commit>`). A movable branch, not a tag
# (user ruling, 2026-08-05): the baseline follows deliberate API adjustments
# during the soft freeze. techy is unpublished, so the baseline is a git
# revision, not a registry version. Override with BASELINE_REV=<rev> for
# ad-hoc comparisons.
#
# Requires cargo-semver-checks (`cargo install cargo-semver-checks --locked`).
#
# RUSTDOCFLAGS is cleared for this run only: the workspace's .cargo/config.toml
# injects docs/rustdoc-header.html by a workspace-root-relative path, which does
# not resolve inside cargo-semver-checks' scratch builds (the injection is a
# doc-presentation concern with no bearing on the API surface being compared).
set -e
BASELINE_REV="${BASELINE_REV:-api-baseline}"
exec env RUSTDOCFLAGS="" cargo semver-checks check-release -p techy --baseline-rev "$BASELINE_REV"
