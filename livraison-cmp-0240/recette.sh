#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
root=$(CDPATH= cd "$ici/.." && pwd)
: "${NOVAOS_TMPDIR:=$(cd "$root/.." && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
case "${1:-}" in
    aarch64) target=aarch64-unknown-none-softfloat ;;
    x86_64) target=x86_64-unknown-none ;;
    *) echo "Usage : $0 aarch64|x86_64 512|4096 répertoire-neuf [--mesures]" >&2; exit 2 ;;
esac
case "${2:-}" in 512|4096) ;; *) exit 2 ;; esac
arch=$1
sector=$2
out=$3
shift 3
mkdir -p "$TMPDIR"
lab=$(mktemp -d "$TMPDIR/cmp-recette.XXXXXXXX")
git -C "$root" archive HEAD components novaOS novaFS | tar -x -C "$lab"
gzip -dc "$ici/admission-recette.patch.gz" > "$lab/admission.patch"
git -C "$lab" apply --check "$lab/admission.patch"
git -C "$lab" apply "$lab/admission.patch"
export RUSTC=$(rustup which --toolchain stable rustc)
export NOVA_CMP_LAB="$ici/bin/cmp-$arch.elf"
export CARGO_TARGET_DIR="$lab/target"
(cd "$lab/novaOS" && rustup run stable cargo build --offline --locked --release --target "$target")
python3 "$ici/statut-invite.py" "$arch" "$sector" "$lab/target/$target/release/novaos" "$out" "$@"
