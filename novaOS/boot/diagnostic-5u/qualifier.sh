#!/bin/sh
# Depuis un worktree NovaOS sur un Mac Apple Silicon avec Rosetta et les cibles rustup.
set -eu
racine=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$racine"
: "${NOVAOS_TMPDIR:=$(cd ../tmp && pwd)}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5u
mkdir -p novaOS/build/qualification-5u
b=$(mktemp -d novaOS/build/qualification-5u/essai.XXXXXX)
rustup run stable rustc -vV
rustup run stable rustfmt --check --edition 2024 --config skip_children=true "$d/lib.rs" "$d/permissions.rs" "$d/tests.rs"
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    case "$cible" in aarch64-*) mode=arm64 ;; x86_64-*) mode=x86_64 ;; esac
    rustup run stable rustc --edition 2024 --test -D warnings --target "$cible" "$d/lib.rs" -o "$b/test-$cible"
    lipo -verify_arch "$mode" "$b/test-$cible"
    echo "Tests hôte $cible"
    /usr/bin/arch "-$mode" "$b/test-$cible" --test-threads=1
done
for cible in x86_64-unknown-uefi x86_64-unknown-none aarch64-unknown-none-softfloat; do
    RUSTC="$(rustup which --toolchain stable rustc)" RUSTFLAGS="-D warnings" rustup run stable cargo build --quiet --release --locked --offline \
        --manifest-path "$d/Cargo.toml" --target-dir "$b/cargo" --target "$cible"
    echo "Bibliothèque no_std compilée pour $cible"
done
make -C novaOS abi-check
test "$(rg -c '^\[call\.' novaOS/abi/syscalls.toml)" = 48
echo '48 sélecteurs ABI'
