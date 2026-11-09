#!/bin/sh
set -eu
. scripts/target.sh

case "${NOVA_ARCH:-x86_64}" in
    x86_64)
        code=edk2-x86_64-code.fd
        vars=edk2-i386-vars.fd
        emulator=qemu-system-x86_64
        set -- -machine q35,i8042=on,pic=on -cpu max -vga "${NOVA_VGA:-std}" \
            -boot order=d -cdrom "$iso" "$@"
        ;;
    aarch64)
        code=edk2-aarch64-code.fd
        vars=edk2-arm-vars.fd
        emulator=qemu-system-aarch64
        set -- -machine virt -cpu cortex-a72 -device ramfb \
            -device virtio-scsi-pci -drive "if=none,id=cd,file=$iso,format=raw,readonly=on,media=cdrom" \
            -device scsi-cd,drive=cd,bootindex=1 "$@"
        ;;
esac

if [ -z "${OVMF_CODE:-}" ]; then
    OVMF_CODE="$(brew --prefix qemu)/share/qemu/$code"
fi
if [ -z "${OVMF_VARS:-}" ]; then
    OVMF_VARS="$(dirname "$OVMF_CODE")/$vars"
fi
for firmware in "$OVMF_CODE" "$OVMF_VARS"; do
    if [ ! -r "$firmware" ]; then
        echo "Firmware UEFI introuvable : $firmware" >&2
        exit 1
    fi
done

# La NVRAM du paquet reste en lecture seule ; QEMU garde les écritures en mémoire.
exec "$emulator" -name NovaOS -accel tcg -smp 1 -m 256M -nic none -no-reboot \
    -drive "if=pflash,unit=0,format=raw,readonly=on,file=$OVMF_CODE" \
    -drive "if=pflash,unit=1,format=raw,snapshot=on,file=$OVMF_VARS" "$@"
