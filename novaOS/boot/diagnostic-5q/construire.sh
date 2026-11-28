#!/bin/bash
# Reconstruction isolée sur le poste de qualification Darwin ARM64.
set -euo pipefail
if [[ $# != 3 ]]; then
  echo "Usage: construire.sh COPIE_EDK_5J SOURCES_QEMU_5J NOM_BUILD_NEUF" >&2
  exit 2
fi
src=$(cd "$1" && pwd -P)
qemu=$(cd "$2" && pwd -P)
name=$3
[[ "$name" =~ ^[A-Za-z0-9][A-Za-z0-9_-]*$ ]]
d=$(cd "$(dirname "$0")" && pwd -P)
nova=$(cd "$d/../.." && pwd -P)
mkdir -p "$nova/build"
out="$nova/build/$name"
mkdir -m 700 "$out"
mkdir "$out/edk"
rsync -a --exclude=/Build --exclude=/Conf --exclude=.git --exclude=__pycache__ "$src/" "$out/edk/"
python3 -B "$d/preparer.py" "$out/edk"
make -C "$out/edk/BaseTools/Source/C/GenFw" > "$out/genfw.log" 2>&1
sh "$d/compiler-signature.sh" "$out/signature"
mkdir "$out/edk/Outils5Q"
cp "$out/signature/x86_64-unknown-none/release/libnovaos_transfert_usb_labo.a" "$out/edk/Outils5Q/libverification.a"
cc -std=c11 -O2 -Wall -Wextra -Werror -dynamiclib -undefined dynamic_lookup \
  $(pkg-config --cflags glib-2.0) -I "$qemu/include/plugins" "$d/controle_cpu.c" -o "$out/controle.dylib"
(cd "$out/edk" && bash "$nova/boot/diagnostic-5j/compiler-edk.sh") > "$out/compilation.log" 2>&1
firmware="$out/edk/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd"
(cd "$d" && python3 -B - "$firmware" <<'PY'
from pathlib import Path
import sys
from politique import FIRMWARE, empreinte, exiger
exiger(empreinte(Path(sys.argv[1]).read_bytes())==FIRMWARE, "Firmware différent du volume qualifié")
print(FIRMWARE)
PY
)
