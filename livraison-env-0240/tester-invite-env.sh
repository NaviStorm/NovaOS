#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
. "$here/lab/environnement.sh"
export RUSTC="$(rustup which --toolchain stable rustc)"
exec python3 -B "$here/lab/invite.py" "$@"
