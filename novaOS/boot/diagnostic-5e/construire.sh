#!/bin/sh
# Depuis tout emplacement. Le répertoire de sortie appartient au worktree actif.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
nova=$(cd "$here/../.." && pwd)
repo=$(cd "$nova/.." && pwd)
: "${NOVAOS_TMPDIR:?indiquer le répertoire temporaire privé NovaOS}"
export TMPDIR="$NOVAOS_TMPDIR/"
export RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTDOC="$(rustup which --toolchain stable rustdoc)"
# Sous UEFI, seules les dépendances invitées reçoivent ces options de liaison.
export RUSTFLAGS="--remap-path-prefix=$repo=/nova --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C link-arg=/timestamp:0 -C link-arg=/debug:none"
rustup run stable cargo build --features firmware --release --locked --offline --manifest-path "$here/Cargo.toml" \
    --target x86_64-unknown-uefi --target-dir "${NOVA5E_BUILD:-$nova/build/phase5e-target}"
