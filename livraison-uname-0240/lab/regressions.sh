#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
root=$(dirname "$ici")
out=$1
mkdir -p "$out"
out=$(cd "$out" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/uname-arc027/" CARGO_NET_OFFLINE=true
export PYTHONDONTWRITEBYTECODE=1 NOVA_RAM=256 NOVA_VCPUS=1
mkdir -p "$TMPDIR" "$root/novaOS/build"
export CARGO_PKG_VERSION=0.20.0
cd "$root"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup run stable rustc --edition 2024 --test --target "$target" novaOS/tests/documentation_catalogue.rs -o "$TMPDIR/catalogue-$target"
    "$TMPDIR/catalogue-$target" > "$out/catalogue-$target.txt"
done
python3 -B novaOS/tests/abi_registre.py > "$out/abi.txt" 2>&1
for arch in aarch64 x86_64; do
    export NOVA_ARCH="$arch"
    make -C novaOS ARCH="$arch" nsh-test nsh-scripts-test > "$out/$arch.log" 2>&1
    for sector in 512 4096; do
        for name in nsh scripts; do cp "novaOS/build/$name-$arch-$sector.json" "$out/"; done
        for suffix in nsh nsh-remontage scripts scripts-reprise; do
            src="novaOS/build/cli-$arch-$sector-$suffix.serial"
            if [ -f "$src" ]; then gzip -c "$src" > "$out/$(basename "$src").gz"; fi
        done
    done
done
