#!/usr/bin/env sh
#
# The workspace's coverage, as a browsable report.
#
# `cargo tarpaulin --out Stdout` already prints the figure, and `ci.yml` gates on it at
# 94 %. What this adds is the per-line view: which lines of which file are uncovered,
# which is the only form that answers *"what should I test next?"*.
#
# **No flags beyond the output format**, deliberately. The engine, the skipped clean and
# the separate build directory all live in `tarpaulin.toml`, so that a hand-run
# `cargo tarpaulin` reports the number this does — two coverage figures for one
# repository is a bug report filed against nothing. This script passed
# `--ignore-tests --exclude-files "*/tests/*"` until 2026-08-16; measured, they changed
# the figure by nothing (98.60 %, 5917/6001, either way), but they restated settings the
# config owns and would have gone stale the first time the config moved.
#
# The gate is a floor, not a target. It sits at 94 against a workspace near 99 so that
# it refuses a collapse rather than a wobble — a gate that trips on the commit adding a
# function, before the commit testing it, is a gate that gets worked around.
#
# Usage:  scripts/coverage.sh
# See also `scripts/diff-coverage.sh`, which scores only what the branch changed.

set -eu

cd "$(git rev-parse --show-toplevel)"

if ! command -v cargo-tarpaulin > /dev/null; then
    echo "cargo-tarpaulin is not installed: cargo install cargo-tarpaulin" >&2
    exit 1
fi

cargo tarpaulin --out Html --output-dir ./coverage

# `xdg-open` is a Linux desktop convention and absent on plenty of machines this could
# run on, so a missing one prints the path rather than failing a report that succeeded.
if command -v xdg-open > /dev/null; then
    xdg-open coverage/tarpaulin-report.html > /dev/null 2>&1 || true
else
    echo "Report written to coverage/tarpaulin-report.html"
fi
