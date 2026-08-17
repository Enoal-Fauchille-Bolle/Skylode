#!/usr/bin/env sh
#
# Coverage of *what this branch changed*, rather than of the whole workspace.
#
# The workspace figure sits near 99 %, which makes it useless as a review signal: a
# hundred new uncovered lines move it by less than a point. This answers the question a
# reviewer actually has — "is the code in this diff tested?" — and it is the number to
# look at before a large commit, since the 94 % gate in `ci.yml` only refuses a
# collapse.
#
# Needs `diff-cover` (Python). Installed here as a user-space package:
#     pip install --user --break-system-packages diff-cover
# Debian 13 marks its Python externally-managed, so a plain `pip install` is refused;
# `pipx install diff-cover` is the tidier route if pipx is available.
#
# No tarpaulin flags beyond the output format. The engine, the skipped clean and the
# separate build directory live in `tarpaulin.toml` precisely so every invocation
# reports the same number — two coverage figures for one repository is a bug report
# filed against nothing. This script passed `--ignore-tests --exclude-files "*/tests/*"`
# until 2026-08-16, which measured the same 98.60 % but restated settings the config
# already owns, and would have gone stale the moment the config moved.

set -eu

cd "$(git rev-parse --show-toplevel)"

# The branch to compare against. `main` is the default; override it for a stacked
# branch, e.g. `COMPARE_BRANCH=origin/main scripts/diff-coverage.sh`. It used to be
# hardcoded to `dev/trunk`, a branch that no longer exists — so this script had been
# failing on its first line rather than reporting anything.
COMPARE_BRANCH="${COMPARE_BRANCH:-main}"

if ! git rev-parse --verify --quiet "$COMPARE_BRANCH" > /dev/null; then
    echo "No such branch: $COMPARE_BRANCH" >&2
    echo "Set COMPARE_BRANCH to one that exists." >&2
    exit 1
fi

if ! command -v diff-cover > /dev/null; then
    echo "diff-cover is not installed — see the header of this script." >&2
    exit 1
fi

cargo tarpaulin --out Xml --output-dir ./coverage

# `--format html:PATH` and not `--html-report`, which diff-cover 10 deprecates and
# warns about on every run.
diff-cover coverage/cobertura.xml \
    --compare-branch="$COMPARE_BRANCH" \
    --format "html:coverage/diff-cover.html"

# Same guard as `coverage.sh`: xdg-open is a Linux desktop convention, and a machine
# without it should still be told where the report went.
if command -v xdg-open > /dev/null; then
    xdg-open coverage/diff-cover.html > /dev/null 2>&1 || true
else
    echo "Report written to coverage/diff-cover.html"
fi
