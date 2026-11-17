#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
root=$(dirname "$ici")
export PYTHONDONTWRITEBYTECODE=1 CARGO_NET_OFFLINE=true
export RUSTC="$(rustup which --toolchain stable rustc)"
mkdir -p "$1" "$root/novaOS/build"
out=$(cd "$1" && pwd)
"$ici/reconstruire.sh" elf
(cd "$root/novaFS" && rustup run stable cargo build --release --locked --offline --target aarch64-apple-darwin --bin novafs)
for arch in aarch64 x86_64; do
  for profil in ${2:-sdk nsh}; do
    if [ "$profil" = nsh ]; then make -C "$root/novaOS" ARCH="$arch" kernel; fi
    python3 "$ici/lab/invite.py" "$arch" "$profil" --out "$out"
  done
done
