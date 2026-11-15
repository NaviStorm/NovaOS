#!/bin/sh
# Deux Mach-O indépendants, même moteur P9 et même module de service que dans l’invité.
set -eu
root=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
: "${NOVAOS_TMPDIR:?définir NOVAOS_TMPDIR conformément à AGENTS.md}"
export TMPDIR="$NOVAOS_TMPDIR/"
arch=${1:?usage : services_config_stockage.sh aarch64|x86_64}
case "$arch" in aarch64|x86_64) ;; *) exit 2 ;; esac
target="$arch-apple-darwin"
export NOVAFS_TARGET="$target"
"$root/novaFS/lab/run" build --offline --lib
lib="${CARGO_TARGET_DIR:-$root/novaFS/target}/$target/release"
work=$(mktemp -d "$NOVAOS_TMPDIR/config-store.XXXXXXXX")
trap 'rm -f "$work/banc" && rmdir "$work"' EXIT
rustup run stable rustc --edition=2024 --target "$target" -C opt-level=2 \
    -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" \
    "$root/novaOS/tests/services_config_stockage.rs" -o "$work/banc"
"$work/banc"
