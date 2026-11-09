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
cp -R licenses "$stage/root/"
cp "target/$rust_target/release/novaos" "$stage/root/boot/novaos.elf"
xorriso -as mkisofs -R -J -V NOVAOS -e boot/limine-uefi-cd.bin \
    -no-emul-boot -efi-boot-part --efi-boot-image --protective-msdos-label \
    -o "$stage/novaos.iso" "$stage/root"
mv "$stage/novaos.iso" "$iso"
