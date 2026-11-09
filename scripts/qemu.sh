#!/bin/sh
set -eu

if [ -z "${OVMF_CODE:-}" ]; then
    OVMF_CODE="$(brew --prefix qemu)/share/qemu/edk2-x86_64-code.fd"
fi
if [ -z "${OVMF_VARS:-}" ]; then
    OVMF_VARS="$(dirname "$OVMF_CODE")/edk2-i386-vars.fd"
fi
for firmware in "$OVMF_CODE" "$OVMF_VARS"; do
    if [ ! -r "$firmware" ]; then
        echo "Firmware UEFI introuvable : $firmware" >&2
        exit 1
    fi
done

# La NVRAM du paquet reste en lecture seule ; QEMU garde les écritures en mémoire.
exec qemu-system-x86_64 -machine q35 -accel tcg -cpu max -smp 1 -m 256M \
    -drive "if=pflash,unit=0,format=raw,readonly=on,file=$OVMF_CODE" \
    -drive "if=pflash,unit=1,format=raw,snapshot=on,file=$OVMF_VARS" \
    -vga "${NOVA_VGA:-std}" -nic none -boot order=d -cdrom build/novaos.iso -no-reboot "$@"
