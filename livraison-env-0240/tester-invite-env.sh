#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
export NOVAOS_TMPDIR="$(dirname "$root")/tmp" TMPDIR="$(dirname "$root")/tmp/" PYTHONDONTWRITEBYTECODE=1
export RUSTC="$(rustup which --toolchain stable rustc)"
exec python3 -B "$here/lab/invite.py" "$@"
