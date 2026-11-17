#!/bin/sh
set -eu
delivery=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
root=$(dirname "$delivery")
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/" PYTHONDONTWRITEBYTECODE=1
export CARGO_NET_OFFLINE=true NOVA_VCPUS=2 NOVA_RAM=256
export RUSTC="$(rustup which --toolchain stable rustc)"
historique="$root/livraison-realpath-0240"
mkdir -p "$TMPDIR"
case "${1:-}" in
    --compiler)
        export CARGO_TARGET_DIR="$TMPDIR/cargo-perf"
        export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$root/novaOS/user/novafs/service.ld"
        for arch in aarch64 x86_64; do
            case "$arch" in aarch64) target=aarch64-unknown-none-softfloat;; *) target=x86_64-unknown-none;; esac
            rustup run stable cargo build --jobs 1 --manifest-path "$root/components/realpath/Cargo.toml" --bin realpath --release --offline --locked --target "$target"
            cp "$CARGO_TARGET_DIR/$target/release/realpath" "$delivery/bin/realpath-$arch.elf"
        done
        ;;
    --executer)
        out=$(mktemp -d "${TMPDIR}perf-qemu.XXXXXXXX")
        echo "$out"
        # Les scripts historiques retrouvent les trois binaires candidats dans cette vue privée.
        stage="$out/recette"
        mkdir -p "$stage/livraison-realpath-0240/lab"
        for component in novaOS novaFS components; do ln -s "$root/$component" "$stage/$component"; done
        for script in elf.py invite.py sonde.py options.py; do
            cp "$historique/lab/$script" "$stage/livraison-realpath-0240/lab/"
        done
        ln -s "$delivery/bin" "$stage/livraison-realpath-0240/bin"
        (cd "$root/novaFS" && rustup run stable cargo build --release --offline --locked --bin novafs --target aarch64-apple-darwin)
        python3 -B "$historique/lab/elf.py" preparer "$out/base-elf"
        for arch in aarch64 x86_64; do
            for sector in 512 4096; do
                python3 -B "$stage/livraison-realpath-0240/lab/sonde.py" "$arch" "$sector" "$out/moteur-$arch-$sector"
                python3 -B "$stage/livraison-realpath-0240/lab/elf.py" "$arch" "$sector" "$out/elf-$arch-$sector" "$out/base-elf"
            done
        done
        echo "Huit démarrages vérifiés dans $out"
        ;;
    *) echo "Usage : $0 --compiler | --executer"; exit 2;;
esac
