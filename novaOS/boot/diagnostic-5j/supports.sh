#!/bin/sh
# Exécuter depuis la racine. Le répertoire des supports doit être neuf.
set -eu
: "${NOVAOS_TMPDIR:?répertoire privé requis}"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
export RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)"
export RUSTFLAGS="--remap-path-prefix=$PWD=/nova --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C link-arg=/timestamp:0 -C link-arg=/debug:none"
d=novaOS/boot/diagnostic-5e
rustup run stable cargo build --bin diagnostic-5j --features diagnostic-5j --release --locked --offline \
  --target x86_64-unknown-uefi --manifest-path "$d/Cargo.toml" --target-dir novaOS/build/target5j
t="$PWD/../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools"
"$t/venv/bin/python" -B "$d/preparer.py" --phase5h \
  novaOS/build/target5j/x86_64-unknown-uefi/release/diagnostic-5j.efi "$1" "$2" \
  --signer "$t/sign-build/osslsigncode" --var-tool "$t/venv/bin/virt-fw-vars"
