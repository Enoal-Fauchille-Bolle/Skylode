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
# draft, or a private working directory can never fail someone else's build — and it
# is why the contents of `organization/` are not checked here and need no exemption:
# gitignored is already out of scope. (Checking whether a tracked file *points at*
# that directory is a different question, and check 7 is the one that asks it.) It
# also means the checks describe the repository as a stranger cloning it sees it,
# which is the audience this whole exercise is aimed at.
mapfile -t DOCS < <(git ls-files '*.md')

# Checks 6 and 7 read the rustdoc as well, because the crates cite the design
# documents as heavily as `docs/` cites itself — 416 section references against 150,
# measured on 2026-08-17 — and `cargo doc -D warnings` validates intra-doc links to
# *Rust items* only. A `§` and a file path in a doc comment are prose to it.
mapfile -t SRC < <(git ls-files 'crates/*.rs')

printf '%sChecking %d tracked markdown files and %d source files…%s\n\n' \
  "$BLUE" "${#DOCS[@]}" "${#SRC[@]}" "$RESET"

# ---------------------------------------------------------------------------
# 1. Relative links that resolve to nothing.
# ---------------------------------------------------------------------------
#
# The cheapest check and the one with the best record: run by hand on 2026-08-16 it
# found ten broken links, all of them in the gitignored working documents and none in
# `docs/` — which is itself the evidence that an unwatched corpus rots and a watched
# one does not.

printf '%s[1/8]%s relative links\n' "$BLUE" "$RESET"

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

printf '%s[2/8]%s heading anchors\n' "$BLUE" "$RESET"

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
# Three citation shapes, and the third is the one that had to be argued for. A path
# (`Foo::bar`) and a SCREAMING_CASE constant were the original two. A bare function
# named in prose is the shape that let `loot_for_level` live in `UI.md` describing a
# function nobody ever wrote, so `snake_case(` now counts as well — but **only when
# the name carries an underscore**. Without that restriction the pattern cannot tell
# a call from the language itself: `pub(crate)`, `#[serde(…)]`, `expect(dead_code, …)`
# and the `feat(core):` of a commit example all have the shape *identifier, paren*,
# and they were seventeen of the eighteen matches when this was measured. Requiring
# a compound name costs the single-word methods, which the corpus writes as
# `GameState::tick` anyway, and it left zero false positives across all 179 files.
#
# What counts as the name existing is the other half, and it is deliberately *not*
# "a definition". The index below accepts four positions: a declaration keyword, a
# name opening a line before `,` `:` `(` or `{` — an enum variant, a struct field, a
# call — and a `use` binding. The looser three are sound for the same reason the
# strict one is: **none of them survives the definition being renamed, because the
# crate would stop compiling.** What does survive a rename is a mention in a comment
# or a string, and those are exactly what this excludes — a comment line opens with
# `//`, never with an identifier. Requiring a keyword instead would have rejected
# `Action::CursorUp` and `Session::next_frame`, since `enum Action` declares `Action`
# and nothing else; a linter that cries wolf gets deleted.
#
# The allowlist holds names that are real but that `crates/*.rs` cannot vouch for,
# in three groups: items of dependencies we cite but do not define, two file names
# the SCREAMING_CASE pattern cannot tell from a constant, and `PRESTIGE`, which is
# the word the player types rather than the constant `CONFIRM_WORD` holding it.
# Measured on 2026-08-18 across the whole tracked corpus: those are the only false
# positives, which is what makes this check cheap enough to keep.

printf '%s[3/8]%s Rust identifiers\n' "$BLUE" "$RESET"

ALLOWLIST='E0599|CARGO_REGISTRY_TOKEN|KDSKBMODE|MASK|SKYLODE_DEV|RUSTDOCFLAGS'
ALLOWLIST+='|rustdoc::private_intra_doc_links|GITHUB_STEP_SUMMARY|GITHUB_OUTPUT'
# Items of dependencies: crossterm, directories, ratatui, and one std associated const.
ALLOWLIST+='|event::poll|ProjectDirs::data_dir|Modifier::DIM|u64::MAX'
ALLOWLIST+='|REPORT_EVENT_TYPES|REPORT_ALL_KEYS_AS_ESCAPE_CODES'
# File names, and the word the prestige overlay makes the player type by hand.
ALLOWLIST+='|LICENSE|SHA256SUMS|PRESTIGE'

# One pass over the sources rather than one per identifier: a few hundred `grep -r`
# invocations become a single index and a `grep -qxF` membership test each.
{
  grep -rhoE '\b(fn|struct|enum|trait|union|type|const|static|mod)[[:space:]]+[A-Za-z_][A-Za-z0-9_]*' \
       --include='*.rs' crates/ | awk '{ print $2 }'
  grep -rhE '^[[:space:]]*(pub[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*[[:space:]]*[,:({]' \
       --include='*.rs' crates/ \
    | sed -E 's/^[[:space:]]*(pub[[:space:]]+)?([A-Za-z_][A-Za-z0-9_]*).*/\2/'
  grep -rhE '^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]' --include='*.rs' crates/ \
    | grep -oE '[A-Za-z_][A-Za-z0-9_]*'
} | sort -u > /tmp/check-docs-names.$$

for f in "${DOCS[@]}"; do
  grep -ohE '`[A-Za-z_][A-Za-z0-9_]*::[A-Za-z_][A-Za-z0-9_]*`|`[A-Z][A-Z0-9_]{3,}`' "$f" \
    | tr -d '`' | sort -u | while read -r id; do
    printf '%s\t%s\n' "$f" "$id"
  done
  grep -ohE '`[a-z][a-z0-9_]*_[a-z0-9_]*\(' "$f" \
    | tr -d '`(' | sort -u | while read -r id; do
    printf '%s\t%s\n' "$f" "$id"
  done
done | sort -u -k2 | while IFS=$'\t' read -r f id; do
  printf '%s' "$id" | grep -qE "^($ALLOWLIST)$" && continue
  last=${id##*::}
  grep -qxF "$last" /tmp/check-docs-names.$$ || printf '%s cites %s\n' "$f" "$id"
done > /tmp/check-docs-ids.$$ || true

rm -f /tmp/check-docs-names.$$

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

printf '%s[4/8]%s prose line width\n' "$BLUE" "$RESET"

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

printf '%s[5/8]%s decision records\n' "$BLUE" "$RESET"

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

  # Every `docs/decisions/NNNN` cited anywhere must name a record that exists.
  #
  # This is the rustdoc's only way to cite a decision. A doc comment cannot use a
  # relative markdown link — the generated HTML lives under `target/doc`, so the path
  # would resolve from the wrong place — so the citation is plain text, and plain text
  # is what check 1 cannot follow. The form deliberately omits the slug: a slug is
  # derived from the record's title, so embedding one would make rewording a title
  # break every citation of it, which is the failure this whole check family exists to
  # prevent.
  grep -rhoE 'docs/decisions/[0-9]{4}' "${DOCS[@]}" "${SRC[@]}" Cargo.toml \
    crates/*/Cargo.toml .coderabbit.yaml 2>/dev/null \
    | sed 's|.*/||' | sort -u | while read -r cited; do
    ls "$ADR_DIR/$cited"-*.md > /dev/null 2>&1 \
      || printf 'a citation names record %s, which does not exist\n' "$cited"
  done > /tmp/check-docs-cited.$$ || true

  while read -r line; do
    [ -n "$line" ] && report "$line"
  done < /tmp/check-docs-cited.$$
  rm -f /tmp/check-docs-cited.$$

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

# ---------------------------------------------------------------------------
# 6. `§` references that name no numbered heading.
# ---------------------------------------------------------------------------
#
# This repository cites sections by number 566 times — `docs/UI.md §5.2`, `(UI.md
# §6.11)`, a bare `§8.3` in a doc comment — and until 2026-08-17 nothing checked one.
# Check 2 does the same job for markdown `#anchor` links, but a `§` is prose: it
# renders as text, so a wrong one is invisible in exactly the way a wrong anchor is.
#
# The cost of leaving it unchecked was paid in full when `UI-EN.md` became
# `docs/UI.md` and the overlays were promoted from a sub-section (§5.7) to a chapter
# (§6). Every number below §5.7 shifted by one, and everything above it moved further:
# §5.9 became §7, §6.4 became §8.4. The old numbers did not stop resolving — several
# of them still name a real section, a *different* one — so a reader following
# `game.rs`'s §5.3 lands on Inventory where the sentence means Mines. Both crates ended
# up carrying a mix of the two schemes with nothing to tell them apart, which is the
# failure this check exists to make impossible.
#
# Two resolution rules, and they are the whole of the convention:
#
#   - the target is the **last `*.md` named at or before the `§` in the same
#     paragraph**, matched by basename so `UI.md §5.2` and `docs/UI.md §5.2` mean the
#     same thing;
#   - with no file named in the paragraph, the reference points **inside its own
#     file** for markdown, and at `docs/UI.md` for a doc comment — which is what a bare
#     `§` has always meant in `crates/`, both crates included.
#
# "At or before" reaches exactly one line back, and both halves of that were measured
# rather than chosen. A strictly line-scoped first draft reported two decision records
# as unanchored when both name `UI.md` on the line directly above the `§` — everything
# here is hard-wrapped, so a citation and its file land on either side of a break
# often. Widening it to the whole paragraph then mis-resolved three of `UI.md`'s own
# bare references to `MECHANICS.md`, named six lines earlier in the same bullet. One
# line back is what a wrap can separate; more than that is a different sentence.
#
# The second rule has an edge the first pass got wrong, and it is worth stating: a
# file with no numbered headings of its own — a decision record, say — cannot be the
# target of its own bare `§`. Two records were citing `§6.8` and `§7` meaning
# `docs/UI.md`, which a reader of the record has no way to know. Those are reported as
# *unanchored* rather than dead: the number is fine, the sentence just never says
# which document numbers it that way.
#
# A target that is not tracked is skipped rather than reported: that is check 7's
# finding, and one mistake should not be counted twice.

printf '%s[6/8]%s section references\n' "$BLUE" "$RESET"

# `file<TAB>number` for every numbered heading, the trailing dot of `## 5. Foo`
# stripped so it compares equal to the `§5` that cites it.
#
# `docs/decisions/` is skipped, and not as an optimisation. A record opens with
# `# 0068 — The prestige multiplier …`, which is a heading whose first word is a run
# of digits — indistinguishable, to a regex, from `## 5. The screens`. Indexing it
# would tell this check that every record carries a section numbered 0068, and the
# first thing that goes wrong is the unanchored rule below: a record would be judged
# capable of being the target of its own `§`. Records have no internal numbering at
# all; their subheadings are `## Decision` and `## Why`.
for f in "${DOCS[@]}"; do
  case "$f" in "$ADR_DIR"/*) continue ;; esac
  grep -oE '^#{1,6} [0-9]+(\.[0-9]+)*\.?[[:space:]]' "$f" \
    | sed -E 's/^#+ //; s/\.?[[:space:]]$//' \
    | while read -r n; do printf '%s\t%s\n' "$f" "$n"; done
done > /tmp/check-docs-headings.$$ || true

for f in "${DOCS[@]}"; do
  printf '%s\t%s\n' "${f##*/}" "$f"
done > /tmp/check-docs-basenames.$$

awk '
  FILENAME == ARGV[1] { heading[$1 "#" $2] = 1; numbered[$1] = 1; next }
  FILENAME == ARGV[2] { path[$1] = $2; next }

  FNR == 1 { carried = ""; carried_from = 0 }

  {
    line = $0
    # Only the line directly above may lend its document to a bare reference.
    inherited = (FNR == carried_from + 1) ? carried : ""
    pos = 0
    while (match(substr(line, pos + 1), /§[0-9]+(\.[0-9]+)*/)) {
      # Save the match immediately. The filename scan below calls match() in turn,
      # and match() writes RSTART/RLENGTH globally — reading them afterwards walks
      # this loop backwards and never terminates.
      start = pos + RSTART
      len   = RLENGTH
      pos   = start + len - 1

      # Take the number out of the match itself rather than by offset. `§` is one
      # character and two bytes, and the two awks disagree about which of those
      # match() counts: mawk counts bytes, gawk in a UTF-8 locale counts characters.
      # Any fixed `+2` is therefore correct on exactly one of them — the first draft
      # hard-coded mawk, passed on Debian, and reported 592 findings on a runner
      # where every `§5.2` had been read as `§.2` and every `§6` as `§`.
      num = substr(line, start, len)
      sub(/^§/, "", num)
      sub(/\.$/, "", num)

      target = inherited
      tail = substr(line, 1, start - 1)
      while (match(tail, /[A-Za-z0-9_.\/-]+\.md/)) {
        target = substr(tail, RSTART, RLENGTH)
        tail = substr(tail, RSTART + RLENGTH)
      }

      if (target != "") {
        base = target
        sub(/^.*\//, "", base)
        if (!(base in path)) continue   # untracked: check 7 owns it
        target = path[base]
      } else if (FILENAME !~ /\.md$/) {
        target = "docs/UI.md"
      } else if (FILENAME in numbered) {
        target = FILENAME
      } else {
        printf "%s:%d cites §%s and names no document; %s has no sections of its own\n", \
               FILENAME, FNR, num, FILENAME
        continue
      }

      if (!((target "#" num) in heading))
        printf "%s:%d cites §%s, which %s has no heading for\n", \
               FILENAME, FNR, num, target
    }

    # Carry the last document this line named into the rest of the paragraph, so that
    # a bare section reference on the next line resolves against it.
    tail = line
    while (match(tail, /[A-Za-z0-9_.\/-]+\.md/)) {
      carried = substr(tail, RSTART, RLENGTH)
      carried_from = FNR
      tail = substr(tail, RSTART + RLENGTH)
    }
  }
' /tmp/check-docs-headings.$$ /tmp/check-docs-basenames.$$ \
  "${DOCS[@]}" "${SRC[@]}" > /tmp/check-docs-sections.$$ || true

while read -r line; do
  [ -n "$line" ] && report "section reference: $line"
done < /tmp/check-docs-sections.$$
rm -f /tmp/check-docs-headings.$$ /tmp/check-docs-basenames.$$ /tmp/check-docs-sections.$$

# ---------------------------------------------------------------------------
# 7. Tracked files pointing at a gitignored working document.
# ---------------------------------------------------------------------------
#
# `organization/` holds the working documents this project was drafted in. They are
# gitignored, so a stranger who clones this repository does not get them, and every
# tracked sentence that cites one is a dead end for the only reader who matters.
#
# The names are listed literally rather than read from the directory, because the
# directory is precisely what a fresh clone lacks — a check that silently passes when
# its input is missing is the shape of check that CI green-lights forever.
#
# The list is by *basename*, and that is the load-bearing detail. Auditing this on
# 2026-08-17 with `git grep 'organization/'` found 22 references; the real number was
# 51, because 29 of them wrote `UI-EN.md` with no directory in front. A rule keyed on
# the prefix would have been satisfied by a corpus that still pointed, twenty-nine
# times, at a file nobody has.

printf '%s[7/8]%s references to working documents\n' "$BLUE" "$RESET"

WORKING_DOCS='organization/|UI-EN\.md|PRICES-FR\.md|TODO-(CORE|TUI|CI|REPO)(-[A-Z]+)?\.md|PROMPT-[A-Z-]+\.md'

for f in "${DOCS[@]}" "${SRC[@]}"; do
  # This script names them all in the comment above, which is the one place the names
  # have to appear for the rule to be readable at all.
  [ "$f" = "scripts/check-docs.sh" ] && continue
  grep -nE "$WORKING_DOCS" "$f" | while IFS=: read -r n _; do
    printf '%s:%s\n' "$f" "$n"
  done
done > /tmp/check-docs-working.$$ || true

while read -r line; do
  [ -n "$line" ] && report "points at a gitignored working document: $line"
done < /tmp/check-docs-working.$$
rm -f /tmp/check-docs-working.$$

# ---------------------------------------------------------------------------
# 8. A decision count written in prose that the directory contradicts.
# ---------------------------------------------------------------------------
#
# Four tracked files quote the size of `docs/decisions/` as a number in a sentence,
# and on 2026-08-17 two of them said 157 and two said 154 against a directory holding
# 157. Nothing distinguished the stale pair from the current one by reading.
#
# This is the smallest possible instance of the rule `docs/BALANCE.md` is built on: a
# figure derivable from the tree is not a fact to maintain by hand. `BALANCE.md` earns
# a generator; one integer earns a check.
#
# Deliberately *not* matched: `DECISIONS.md`'s "a single table of 154 rows", which is
# a true statement about what the ledger used to be. The pattern requires the words
# "numbered records", which is the phrase only a claim about the present uses.

printf '%s[8/8]%s decision count\n' "$BLUE" "$RESET"

if [ -d "$ADR_DIR" ]; then
  actual=$(find "$ADR_DIR" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-*.md' | wc -l)

  for f in "${DOCS[@]}"; do
    grep -noE '[0-9]+ numbered records' "$f" | while IFS=: read -r n claim; do
      claimed=${claim%% *}
      [ "$claimed" = "$actual" ] \
        || printf '%s:%s says %s numbered records; there are %s\n' "$f" "$n" "$claimed" "$actual"
    done
  done > /tmp/check-docs-count.$$ || true

  while read -r line; do
    [ -n "$line" ] && report "stale decision count: $line"
  done < /tmp/check-docs-count.$$
  rm -f /tmp/check-docs-count.$$
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
