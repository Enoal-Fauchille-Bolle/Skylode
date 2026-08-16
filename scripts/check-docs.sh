#!/usr/bin/env bash
#
# The documentation's linter.
#
# This exists because of an asymmetry the 2026-08-16 stock-take measured: the two
# documentation corpora in this repository have drifted by wildly different amounts,
# and the difference is structural rather than cultural. The rustdoc has a mechanical
# gate — `cargo doc -D warnings`, in both the pre-commit hook and `ci.yml` — and it is
# the corpus that stayed true. The markdown has none, and it is where a working
# document outlived its own migration, where a link pointed five times at a file one
# directory up, and where a roadmap still counted five screens against a `Screen::ALL`
# of six. A rule with no executor is a wish.
#
# What it can and cannot do bounds the ambition deliberately. Nothing here reads
# prose, so nothing here can catch a paragraph that has quietly stopped being true —
# that is the same limit `cargo doc` has, and CONTRIBUTING.md already names it. What
# it catches is the mechanical half: a pointer that resolves to nothing, and a name
# the code no longer answers to. That half is worth automating precisely because it
# is the half a human reader skims past.
#
# It runs in `ci.yml` and **not** in `.githooks/pre-commit`. The hook is already the
# slow link — it compiles four times — and this check's failures are never urgent in
# the way a broken build is: a stale link does not ship a bug. The same argument the
# hook's own header makes about `cargo-deny` and `actionlint` applies, with one
# addition in its favour: this one needs no tool a fresh clone lacks, so a contributor
# who wants it locally can simply run it.
#
# Usage:  bash scripts/check-docs.sh
# Exit:   0 if every check passes, 1 on the first category that has findings.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Colour only when a human is watching. A CI log is not a terminal, and escape codes
# in a GitHub Actions annotation are noise rather than emphasis.
if [ -t 1 ]; then
  RED=$'\033[1;31m'; GREEN=$'\033[1;32m'; BLUE=$'\033[1;34m'; DIM=$'\033[2m'; RESET=$'\033[0m'
else
  RED=''; GREEN=''; BLUE=''; DIM=''; RESET=''
fi

findings=0

report() {
  findings=$((findings + 1))
  printf '%s%s%s\n' "$RED" "  $1" "$RESET"
}

# `git ls-files` and not `find`, and this is the load-bearing choice in the whole
# script. It restricts every check to what is *tracked*, which means a scratch file, a
# draft, or anything a contributor keeps gitignored can never fail someone else's
# build. It also means the checks describe the repository as a stranger cloning it
# sees it — which is the audience the whole documentation restructure is aimed at.
#
# `docs/archive/` is the one exemption, and it is exempt by definition rather than by
# convenience: an archive is a document frozen at a date, and its links point into a
# world that no longer exists. Linting one would demand that a snapshot keep up with
# the tree it is a snapshot *of*, which is the opposite of what it is for. The
# directory is expected to be short-lived — it exists so the working checklists enter
# git once before being removed — so if `docs/archive/` is gone, this line should go
# with it.
mapfile -t DOCS < <(git ls-files '*.md' | grep -v '^docs/archive/')

printf '%sChecking %d tracked markdown files…%s\n\n' "$BLUE" "${#DOCS[@]}" "$RESET"

# ---------------------------------------------------------------------------
# 1. Relative links that resolve to nothing.
# ---------------------------------------------------------------------------
#
# The cheapest check and the one with the best record: run by hand on 2026-08-16 it
# found ten broken links, all of them in the gitignored working documents and none in
# `docs/` — which is itself the evidence that an unwatched corpus rots and a watched
# one does not.

printf '%s[1/5]%s relative links\n' "$BLUE" "$RESET"

for f in "${DOCS[@]}"; do
  dir=$(dirname "$f")
  # Strip the `#anchor` before testing the path: check 2 owns the anchor half, and a
  # link to `FOO.md#bar` must not be reported twice for one mistake.
  grep -oE '\]\([^)]+\)' "$f" | sed 's/^](//; s/)$//' | sed 's/#.*//' | while read -r target; do
    case "$target" in
      http*|mailto:*|'') continue ;;
    esac
    [ -e "$dir/$target" ] || printf '%s -> %s\n' "$f" "$target"
  done
done > /tmp/check-docs-links.$$ || true

while read -r line; do
  [ -n "$line" ] && report "broken link: $line"
done < /tmp/check-docs-links.$$
rm -f /tmp/check-docs-links.$$

# ---------------------------------------------------------------------------
# 2. Anchors that name no heading.
# ---------------------------------------------------------------------------
#
# Half of `docs/`'s cross-references carry an `#anchor`, and an anchor is the part
# that breaks silently: renaming a heading leaves every link to it *rendering*
# perfectly and landing at the top of the page. There is no way to notice by reading.
#
# The slug rules reproduced here are GitHub's: lowercase, drop everything that is not
# alphanumeric, space, underscore or hyphen, then spaces to hyphens. Verified against
# all 60 anchors this repository currently uses, including the two awkward ones —
# `Integrity (HMAC)` -> `integrity-hmac`, and `Phase 9 - Save (serialisation half
# only)` -> `phase-9---save-serialisation-half-only`, where the run of three hyphens
# is the dropped parenthesis leaving its space behind.

printf '%s[2/5]%s heading anchors\n' "$BLUE" "$RESET"

slugify() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed 's/[^a-z0-9 _-]//g; s/ /-/g'
}

# Precompute every heading slug of every tracked file, once. Recomputing per link
# would reread `UI.md`'s 56 headings for each of the links pointing into it.
declare -A HEADINGS
for f in "${DOCS[@]}"; do
  while IFS= read -r heading; do
    HEADINGS["$f#$(slugify "$heading")"]=1
  done < <(grep -E '^#{1,6} ' "$f" | sed 's/^#\{1,6\} //')
done

for f in "${DOCS[@]}"; do
  dir=$(dirname "$f")
  grep -oE '\]\([^)]*#[^)]+\)' "$f" | sed 's/^](//; s/)$//' | while read -r target; do
    case "$target" in http*|mailto:*) continue ;; esac
    anchor=${target#*#}
    path=${target%%#*}
    # A bare `#anchor` points inside the file that carries it.
    if [ -z "$path" ]; then
      resolved="$f"
    else
      resolved=$(realpath -m --relative-to=. "$dir/$path")
      # A path that does not exist is check 1's finding, not this one's.
      [ -e "$resolved" ] || continue
    fi
    [ -n "${HEADINGS["$resolved#$anchor"]:-}" ] \
      || printf '%s -> %s#%s\n' "$f" "$path" "$anchor"
  done
done > /tmp/check-docs-anchors.$$ || true

while read -r line; do
  [ -n "$line" ] && report "dead anchor: $line"
done < /tmp/check-docs-anchors.$$
rm -f /tmp/check-docs-anchors.$$

# ---------------------------------------------------------------------------
# 3. Rust names the code no longer answers to.
# ---------------------------------------------------------------------------
#
# The documentation quotes identifiers constantly — `GameState::tick`,
# `RAW_PER_COMPRESSED`, `Mine::refill_if_empty` — and a rename in `crates/` leaves
# every one of them behind without a single compiler warning, because markdown is not
# compiled. This is the markdown half of what `broken_intra_doc_links` already does
# for the rustdoc.
#
# It matches the *last* segment of a path rather than the whole of it: `Foo::bar`
# passes if `bar` exists anywhere in `crates/`. That is deliberately loose. A stricter
# check would have to resolve module paths, and the failure it would then report most
# often is its own — a linter that cries wolf gets deleted, and this one only needs to
# catch a name that has vanished entirely, which is what a rename produces.
#
# The allowlist holds names that are real but live outside `crates/`: two environment
# variables, a Linux kernel ioctl the keyboard section cites, one rustdoc lint, and
# the word MASK where `SYSTEMS.md` uses it as prose. Measured on 2026-08-16: those
# four were the *only* false positives across the whole tracked corpus, which is what
# makes this check cheap enough to keep.

printf '%s[3/5]%s Rust identifiers\n' "$BLUE" "$RESET"

ALLOWLIST='CARGO_REGISTRY_TOKEN|KDSKBMODE|MASK|SKYLODE_DEV|RUSTDOCFLAGS|rustdoc::private_intra_doc_links|GITHUB_STEP_SUMMARY|GITHUB_OUTPUT'

for f in "${DOCS[@]}"; do
  grep -ohE '`[A-Za-z_][A-Za-z0-9_]*::[A-Za-z_][A-Za-z0-9_]*`|`[A-Z][A-Z0-9_]{3,}`' "$f" \
    | tr -d '`' | sort -u | while read -r id; do
    printf '%s\t%s\n' "$f" "$id"
  done
done | sort -u -k2 | while IFS=$'\t' read -r f id; do
  printf '%s' "$id" | grep -qE "^($ALLOWLIST)$" && continue
  last=${id##*::}
  grep -rqE "\b${last}\b" crates/ || printf '%s cites %s\n' "$f" "$id"
done > /tmp/check-docs-ids.$$ || true

while read -r line; do
  [ -n "$line" ] && report "unknown identifier: $line"
done < /tmp/check-docs-ids.$$
rm -f /tmp/check-docs-ids.$$

# ---------------------------------------------------------------------------
# 4. Prose wider than 100 columns.
# ---------------------------------------------------------------------------
#
# Not a style preference — a diff-legibility rule, and the reason it is here is
# `DECISIONS.md`: 154 decisions in one table whose average line ran 524 characters and
# whose longest ran 2 028. At that width `git diff` reports "this line changed" about
# a paragraph, code review stops being possible, and the document becomes append-only
# by accident rather than by design.
#
# Three exemptions, and each one is a case where wrapping would destroy meaning rather
# than aid it:
#
#   - **Table rows** (`| … |`) cannot be wrapped at all in markdown. The right fix for
#     a wide table is a different container, which is what moving the ledger to
#     `docs/decisions/` does; a linter cannot make that judgement.
#   - **Fenced code blocks**, which in `UI.md` are 122 counted ASCII wireframes whose
#     whole purpose is to be exactly as wide as the terminal they describe. Wrapping
#     one would be vandalism.
#   - **Lines carrying a URL**, which have no legal break point.
#
# Measured against those three: the entire tracked corpus had two violations on
# 2026-08-16, both ordinary prose overshooting by under fifteen columns.

printf '%s[4/5]%s prose line width\n' "$BLUE" "$RESET"

LIMIT=100

for f in "${DOCS[@]}"; do
  awk -v file="$f" -v limit="$LIMIT" '
    # Track fenced blocks so their contents are exempt. Both ``` and ~~~ open a
    # fence; the toggle is deliberately dumb, because a nested fence is not
    # something this repository writes.
    /^[[:space:]]*(```|~~~)/ { fenced = !fenced; next }
    fenced { next }
    /^[[:space:]]*\|/ { next }
    /https?:\/\// { next }
    length > limit { printf "%s:%d (%d cols)\n", file, FNR, length }
  ' "$f"
done > /tmp/check-docs-width.$$ || true

while read -r line; do
  [ -n "$line" ] && report "line too long: $line"
done < /tmp/check-docs-width.$$
rm -f /tmp/check-docs-width.$$

# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# 5. The decision records' own rules.
# ---------------------------------------------------------------------------
#
# `docs/decisions/` is an append-only, numbered sequence, and the three properties
# below are the whole of what makes it one. None of them survives on discipline: a
# record added without an index row is invisible, a duplicated number silently
# reassigns an identity that other records and the rustdoc cite by number, and a
# `Status` outside the four legal values is a record whose standing nobody can read.
#
# What this cannot check is the property that matters most — that a revisited
# decision was *superseded* rather than quietly rewritten. Only the diff shows that,
# which is why the records are wrapped at 88 columns: to keep that diff readable by
# the one reviewer who can judge it.

printf '%s[5/5]%s decision records\n' "$BLUE" "$RESET"

ADR_DIR="docs/decisions"
if [ -d "$ADR_DIR" ]; then
  index="$ADR_DIR/README.md"

  seen=""
  for record in "$ADR_DIR"/[0-9][0-9][0-9][0-9]-*.md; do
    [ -e "$record" ] || continue
    base=$(basename "$record")
    number=${base%%-*}

    case "$seen" in
      *" $number "*) report "duplicate record number: $number" ;;
      *) seen="$seen $number " ;;
    esac

    status=$(grep -m1 '^\*\*Status:\*\* ' "$record" | sed 's/^\*\*Status:\*\* //')
    case "$status" in
      accepted|rejected|withdrawn|superseded) ;;
      *) report "record $number has an unusable Status: '${status:-<missing>}'" ;;
    esac

    grep -q "($base)" "$index" || report "record $number is missing from the index"
  done

  # Every `Supersedes:`/`Superseded by:` target must name a record that exists. The
  # link checker already proves the *path* resolves; this proves the field was filled
  # in with a record rather than with prose.
  grep -hoE '^\*\*(Supersedes|Superseded by):\*\* .*' "$ADR_DIR"/[0-9]*.md \
    | grep -v ':\*\* —$' \
    | grep -oE '\[[0-9]{4}\]' | tr -d '[]' | sort -u | while read -r target; do
    ls "$ADR_DIR/$target"-*.md > /dev/null 2>&1 \
      || printf 'supersession names record %s, which does not exist\n' "$target"
  done > /tmp/check-docs-adr.$$ || true

  while read -r line; do
    [ -n "$line" ] && report "$line"
  done < /tmp/check-docs-adr.$$
  rm -f /tmp/check-docs-adr.$$
fi

printf '\n'
if [ "$findings" -eq 0 ]; then
  printf '%sDocumentation checks passed.%s\n' "$GREEN" "$RESET"
  exit 0
fi

printf '%s%d finding(s).%s %sNothing here reads prose — a link that resolves is not a\n' \
  "$RED" "$findings" "$RESET" "$DIM"
printf 'sentence that is still true.%s\n' "$RESET"
exit 1
