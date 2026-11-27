#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
nova=$(cd "$here/../.." && pwd)
: "${NOVAOS_TMPDIR:?indiquer le répertoire temporaire privé NovaOS}"
export TMPDIR="$NOVAOS_TMPDIR/"
export RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTDOC="$(rustup which --toolchain stable rustdoc)"
logdir=${1:?nouveau répertoire pour les journaux}
mkdir "$logdir"
scratch=$(mktemp -d "$NOVAOS_TMPDIR/modeles-5e.XXXXXXXX")
printf '#[path = "%s/kernel/src/hardware/acpi.rs"] pub mod acpi;\n' "$nova" >"$scratch/modele.rs"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    "$RUSTC" --edition 2024 --test --target "$target" "$scratch/modele.rs" -o "$scratch/acpi-$target"
    "$scratch/acpi-$target" > "$logdir/acpi-$target.log"
    rustup run stable cargo test --release --offline --locked --target "$target" \
        --target-dir "$nova/build/phase5e-regression" --manifest-path "$nova/libs/catalogue-pilotes/Cargo.toml" \
        --test admission --test politique --test plans > "$logdir/catalogue-$target.log"
    rustup run stable cargo test --release --offline --locked --target "$target" \
        --target-dir "$nova/build/phase5e-regression" --manifest-path "$nova/user/novaauth/Cargo.toml" \
        --features distribution-lab --test distribution paquet_entier_atteste_avant_publication \
        > "$logdir/paquet-$target.log"
    rustup run stable cargo test --release --offline --locked --target "$target" \
        --target-dir "$nova/build/phase5e-regression" --manifest-path "$here/Cargo.toml" --test passif \
        > "$logdir/diagnostic-$target.log"
done
