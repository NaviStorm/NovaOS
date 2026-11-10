#!/bin/sh
set -eu
. scripts/target.sh

(cd third_party/limine && shasum -a 256 -c SHA256SUMS)
mkdir -p build
stage=$(mktemp -d build/iso.XXXXXXXX)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/root/boot" "$stage/root/EFI/BOOT"
cp third_party/limine/limine-uefi-cd.bin "$stage/root/boot/"
cp "third_party/limine/$efi_loader" "$stage/root/EFI/BOOT/"
cp boot/limine.conf "$stage/root/"
if [ "$#" -ne 0 ]; then
    [ "$#" -eq 2 ] || exit 2
    case "$1" in
        proc-test|proc-boot-fault|mem-test|mem-ro|mem-alias|mem-page-ro|mem-nx|mem-absent|heap-test|heap-oom|heap-corrupt|heap-reentrant|heap-metadata|heap-guard|heap-infallible|sched-test|sched-stress|sched-endurance|sched-guard|sched-fpu) ;;
        *) exit 2 ;;
    esac
    printf '    cmdline: %s\n' "$1" >> "$stage/root/limine.conf"
    iso=$2
fi
cp -R licenses "$stage/root/"
cp "target/$rust_target/release/novaos" "$stage/root/boot/novaos.elf"
xorriso -as mkisofs -R -J -V NOVAOS -e boot/limine-uefi-cd.bin \
    -no-emul-boot -efi-boot-part --efi-boot-image --protective-msdos-label \
    -o "$stage/novaos.iso" "$stage/root"
mv "$stage/novaos.iso" "$iso"
