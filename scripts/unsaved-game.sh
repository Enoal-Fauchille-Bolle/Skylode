#!/usr/bin/env sh
#
# Runs the game where it cannot find — or create — a save file.
#
# `persist` resolves the save's location through `ProjectDirs::from("", "", "skylode")`,
# and on Linux that whole function is guarded by a home directory: `directories`'
# `project_dirs_from_path` reads `$XDG_DATA_HOME` only *inside* an
# `if let Some(home_dir)`, so with no home it returns `None` outright and the XDG
# variables never get a say. Take the home away and the front-end takes the branch it
# otherwise only reaches on a broken machine: the title screen says *no persistence*,
# the run is playable, and every autosave fails on the edge without being fatal.
#
# That branch is worth being able to reach by hand. It is reachable from a test — the
# path is injected — but a test cannot tell you whether the standing banner is legible,
# whether the title's permanent line reads as a warning or as furniture, or what a save
# failure looks like three hours into a run.
#
# **Why `unshare` rather than `env -u HOME` alone.** Unsetting `HOME` is not enough.
# `dirs_sys::home_dir` reads `$HOME` and then falls back to `getpwuid_r(getuid())`, so
# the passwd database hands back the real home and the game writes a real save. Entering
# a user namespace mapped to an unused uid (12345) produces a user with no passwd entry,
# leaving the fallback nothing to find. That second half is the whole trick, and it is
# why this script looks heavier than the thing it does.
#
# **Linux only.** `unshare` is a Linux syscall with no macOS or Windows equivalent. On
# those platforms the same branch is easiest to reach by making the resolved save
# directory unwritable, rather than by hiding the home directory.
#
# Usage:  scripts/unsaved-game.sh
# The dev menu is enabled too, so a state past the first hour is one keypress away.

set -eu

cd "$(git rev-parse --show-toplevel)"

BINARY=./target/debug/skylode

if [ ! -x "$BINARY" ]; then
    echo "No debug binary at $BINARY — run 'cargo build -p skylode-tui' first." >&2
    exit 1
fi

# `-U` alone would map the current uid; the explicit map is what produces a uid with no
# passwd entry, which is the point. `--map-group` matters as little as it looks, and is
# passed so the namespace has a complete mapping rather than a partial one.
exec unshare -U --map-user=12345 --map-group=12345 \
    env -u HOME SKYLODE_DEV=1 "$BINARY"
