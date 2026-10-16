#!/bin/sh
# API-stability guard (dev-docs/DESIGN_RATIONALE.md [§dd-dr:stability-rubric]):
# compare techy's public surface against the frozen semver baseline.
#
# Baseline: the `api-baseline` git branch — pointed at the commit where the
# API-review Phase 3 restructuring landed; move it deliberately at each version
# bump (0.x discipline: breaking -> 0.(x+1).0, additions -> 0.x.(z+1), then
# `git branch -f api-baseline <release-commit>`). A movable branch, not a tag
# (user ruling, 2026-08-05): the baseline follows deliberate API adjustments
# during the soft freeze. The baseline is a git revision, not the registry
# version, so the comparison also covers API work between releases. Override
# with BASELINE_REV=<rev> for ad-hoc comparisons.
#
# Requires cargo-semver-checks (`cargo install cargo-semver-checks --locked`).
#
# The baseline is checked out as a git worktree at target/semver-baseline and
# passed as --baseline-root rather than as --baseline-rev: the tool's own export
# of a git revision does not reproduce the `techy/docs -> ../docs` symlink, so
# the baseline's rustdoc build fails on the `include_str!`'d guide pages. A real
# checkout keeps the symlink. The worktree is created and removed only through
# `git worktree`, which refuses to touch a directory that is not one of its own
# worktrees; a leftover from an interrupted run is removed on the next run.
#
# RUSTDOCFLAGS is cleared for this run only: the workspace's .cargo/config.toml
# injects docs/rustdoc-header.html by a workspace-root-relative path, which does
# not resolve inside cargo-semver-checks' scratch builds (the injection is a
# doc-presentation concern with no bearing on the API surface being compared).
set -e
BASELINE_REV="${BASELINE_REV:-api-baseline}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
BASELINE_DIR="$REPO_ROOT/target/semver-baseline"
git -C "$REPO_ROOT" worktree remove --force "$BASELINE_DIR" 2>/dev/null || true
# --force: re-register the path if a previous worktree there was deleted by
# hand (e.g. by `cargo clean`); an existing directory is still refused.
git -C "$REPO_ROOT" worktree add --quiet --force --detach "$BASELINE_DIR" "$BASELINE_REV"
trap 'git -C "$REPO_ROOT" worktree remove --force "$BASELINE_DIR"' EXIT
env RUSTDOCFLAGS="" cargo semver-checks check-release -p techy --baseline-root "$BASELINE_DIR"
