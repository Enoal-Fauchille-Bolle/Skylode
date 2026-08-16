#!/usr/bin/env bash
# Ask the current terminal whether it speaks the kitty keyboard protocol.
#
# How it works: we write the query sequence CSI ? u  (bytes: 0x1b 0x5b 0x3f 0x75)
# to the terminal. A terminal that implements the protocol answers by injecting
# CSI ? <flags> u back into our *input*, exactly as if the user had typed it.
# A terminal that does not implement it stays silent, so we time out.
#
# We must put the terminal in raw mode first, otherwise the line discipline
# waits for a newline that is never coming, and -echo so the reply is not
# painted on screen.

exec < /dev/tty

old_stty=$(stty -g)
# min 0 / time 3 = read returns after at most 0.3s even with zero bytes.
stty raw -echo min 0 time 3

printf '\033[?u' > /dev/tty
reply=$(dd bs=1 count=32 2>/dev/null)

stty "$old_stty"

printf '\n'
printf 'TERM          = %s\n' "${TERM:-<unset>}"
printf 'TERM_PROGRAM  = %s\n' "${TERM_PROGRAM:-<unset>}"
if [ -n "$TMUX" ]; then
  printf 'tmux          = yes (this can swallow the reply)\n'
fi
printf '\n'

if [ -z "$reply" ]; then
  printf 'Reply         : (none)\n'
  printf 'Verdict       : NO kitty keyboard protocol.\n'
  printf '                Space release is NOT reportable here.\n'
else
  printf 'Reply         : %s\n' "$(printf '%s' "$reply" | cat -v)"
  printf 'Verdict       : kitty keyboard protocol SUPPORTED.\n'
  printf '                (^[[?<n>u means: current flags = <n>, usually 0.)\n'
fi
