#!/bin/sh
set -eu
: "${NOVAOS_TMPDIR:?répertoire temporaire NovaOS requis}"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1 CARGO_NET_OFFLINE=true
export RUSTC="$(rustup which --toolchain stable rustc)"
# Appeler depuis la racine du worktree. La copie des composants est privée à chaque construction.
d="$PWD/novaOS/boot/live-5z"
b=${1:?répertoire de sortie}
archive=${2:?archive source limine-10.8.5.tar.xz}
mkdir -p "$b"
b=$(cd "$b" && pwd)
archive=$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")
sh "$d/chargeur.sh" "$archive" "$b/limine" "$d/limine-usb.patch"
outil="$b/limine/bin/limine"
cd novaOS
export CARGO_TARGET_DIR="$b/rust"
export RUSTFLAGS="-C no-redzone=yes -C relocation-model=static --remap-path-prefix=$PWD/..=/nova --remap-path-prefix=$PWD=/nova/novaOS --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup"
make ARCH=x86_64 kernel
python3 -B "$d/image.py" preparer "$b/stage" "$CARGO_TARGET_DIR/x86_64-unknown-none/release/novaos" "$outil" "$b/limine/bin/BOOTX64.EFI"
export NOVA5Z_PLAN="$b/stage/PLAN.BIN"
export RUSTFLAGS="--remap-path-prefix=$PWD=/nova/novaOS --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C link-arg=/timestamp:0 -C link-arg=/debug:none"
rustup run stable cargo build --offline --locked --manifest-path "$d/Cargo.toml" --target x86_64-unknown-uefi --release
cp "$CARGO_TARGET_DIR/x86_64-unknown-uefi/release/live.efi" "$b/stage/BOOTX64.EFI"
python3 -B "$d/image.py" assembler "$b/stage" "$b/novaos-live.img"
