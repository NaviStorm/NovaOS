#!/bin/sh
set -eu
racine=$(cd "$(dirname "$0")/../../.." && pwd -P)
: "${TMPDIR:?}"
export RUSTC="$(rustup which --toolchain stable rustc)"
lib=$(rustup run stable cargo build --release --locked --offline --manifest-path "$racine/novaFS/Cargo.toml" --lib --target aarch64-apple-darwin --message-format=json | python3 -c 'import json,sys; print(next(f for l in sys.stdin for f in json.loads(l).get("filenames",[]) if f.endswith(".rlib") and "/libnovafs" in f))')
"$RUSTC" --edition=2024 -O --extern "novafs=$lib" -L "dependency=$(dirname "$lib")/deps" "$racine/components/sort/lab/volume_etroit.rs" -o "$TMPDIR/arc027-etat-sort-p9"
echo "$TMPDIR/arc027-etat-sort-p9"
