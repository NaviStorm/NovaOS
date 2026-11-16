#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd -P)
: "${TMPDIR:?}"
image=${1:?chemin neuf requis}
sector=${2:?512 ou 4096}
case $sector in 512|4096) ;; *) exit 2 ;; esac
export RUSTC="$(rustup which --toolchain stable rustc)"
lib=$(rustup run stable cargo build --locked --offline --release --manifest-path "$root/novaFS/Cargo.toml" --lib --target aarch64-apple-darwin --message-format=json | python3 -c 'import json,sys; print(next(f for l in sys.stdin for f in json.loads(l).get("filenames",[]) if f.endswith(".rlib") and "/libnovafs" in f))')
"$RUSTC" --edition=2024 -O --extern "novafs=$lib" -L "dependency=$(dirname "$lib")/deps" "$root/components/sort/lab/p9.rs" -o "$TMPDIR/arc027-sort-p9"
"$TMPDIR/arc027-sort-p9" "$image" "$sector"
