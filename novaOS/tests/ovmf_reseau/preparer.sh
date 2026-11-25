#!/bin/sh
# Prépare un support synthétique avec une seule empreinte d'image autorisée.
set -eu
[ "$#" -ge 3 ] && [ "$#" -le 4 ] || { echo "usage : preparer.sh arbre-edk2 repertoire-firmware dossier-neuf [--vtd]" >&2; exit 2; }
src=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
edk=$(CDPATH= cd -- "$1" && pwd)
fv=$(CDPATH= cd -- "$2" && pwd)
mkdir "$3"
out=$(CDPATH= cd -- "$3" && pwd)
mkdir -p "$out/esp/EFI/BOOT"
efi="$out/esp/EFI/BOOT"
compile() {
    clang --target=x86_64-pc-windows-msvc -ffreestanding -fno-stack-protector -fshort-wchar \
        -mno-red-zone -Werror -Wall -Os -I"$edk/MdePkg/Include" -I"$edk/MdePkg/Include/X64" "$@"
}
if [ "$#" -eq 4 ]; then
    [ "$4" = "--vtd" ] || exit 2
    compile -DNOVA_VTD_TERMINAL -c "$src/temoin.c" -o "$out/temoin.obj"
    compile -c "$src/vtd.c" -o "$out/vtd.obj"
    set -- "$out/temoin.obj" "$out/vtd.obj"
else
    compile -c "$src/temoin.c" -o "$out/temoin.obj"
    set -- "$out/temoin.obj"
fi
lld-link /entry:EfiMain /subsystem:efi_application /nodefaultlib /machine:x64 /Brepro \
    "/out:$efi/BOOTX64.EFI" "$@"
cp "$edk/Build/OvmfX64/RELEASE_CLANGDWARF/X64/Shell.efi" "$efi/REFUSE.EFI"
hash=$(python -c 'import pefile, sys; from virt.peutils.pesign import pe_authenticode_hash; print(pe_authenticode_hash(pefile.PE(sys.argv[1])).hex())' "$efi/BOOTX64.EFI")
virt-fw-vars -i "$fv/OVMF_VARS.fd" -o "$out/variables.fd" --enroll-generate "NovaOS Q35 laboratoire" \
   --microsoft-kek none --no-microsoft --add-db-hash 7a58904e-20f7-47ae-9f7c-674f87ef1104 "$hash" --sb
qemu-img convert -f raw -O raw "fat:$out/esp" "$out/support.raw"
