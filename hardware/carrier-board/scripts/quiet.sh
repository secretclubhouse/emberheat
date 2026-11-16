#!/bin/sh
# Run a command and drop the harmless Fontconfig and wxWidgets start-up
# chatter from its output, keeping the command's own exit status (a plain
# `cmd | grep -v ... || true` hides failures: make's shell has no pipefail).
# Usage: quiet.sh COMMAND [ARGS...]
out=$("$@" 2>&1)
status=$?
if [ -n "$out" ]; then
  printf '%s\n' "$out" | grep -v -e Fontconfig -e wxApp -e "image handler"
fi
exit $status
