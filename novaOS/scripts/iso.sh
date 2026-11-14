#!/bin/sh
set -eu
root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/target.sh"

(cd "$root/third_party/limine" && shasum -a 256 -c SHA256SUMS)
iso="$root/$iso"
mkdir -p "$root/build"
stage=$(mktemp -d "$root/build/iso.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/root/boot" "$stage/root/EFI/BOOT"
cp "$root/third_party/limine/limine-uefi-cd.bin" "$stage/root/boot/"
cp "$root/third_party/limine/$efi_loader" "$stage/root/EFI/BOOT/"
cp "$root/boot/limine.conf" "$stage/root/"
if [ "$#" -ne 0 ]; then
    [ "$#" -eq 2 ] || exit 2
    case "$1" in
        ipc-smp-libre|ipc-smp-local|ipc-smp-distant|ipc-smp-paires|ipc-smp-paires-sonde|ipc-smp-calcul|ipc-smp-memoire|ipc-smp-vfs|ipc-smp-sonde|smp-test|smp-arret|smp-panique|smp-ap-absent|smp-cpu-muet|smp-fp|cli|cli-test|namespace-f1-inflight|namespace-f2-inflight|namespace-disconnect|namespace-depth|namespace-inspect|namespace-write|namespace-read|namespace-rename|namespace-f1-before|namespace-f1-after|namespace-f2-before|namespace-f2-after|files-recover|files-f1-before|files-f1-inflight|files-f1-after|files-f2-before|files-f2-inflight|files-f2-after|files-data|files-meta|files-close|files-remount|files-inject|files-info|files-abandon|files-restart|files-perf|files-write|files-read|elf-test|storage-page-write|storage-page-read|storage-live|storage-write|storage-read|storage-perf|storage-eio|dma-flight|dma-fence|dma-user|dma-qual|storage-test|ipc-test|ipc-endurance|ipc-perf|ipc-races|abi-test|proc-lifecycle|proc-test|proc-boot-fault|mem-test|mem-ro|mem-alias|mem-page-ro|mem-nx|mem-absent|heap-test|heap-oom|heap-corrupt|heap-reentrant|heap-metadata|heap-guard|heap-infallible|sched-test|sched-stress|sched-endurance|sched-guard|sched-fpu) ;;
        *) exit 2 ;;
    esac
    printf '    cmdline: %s\n' "$1" >> "$stage/root/limine.conf"
    iso=$2
fi
cp -R "$root/licenses" "$stage/root/"
cp "$root/../components/LICENSE" "$stage/root/licenses/cli-MIT.txt"
python3 "$root/scripts/licences_service.py" "$rust_target" "$stage/root/licenses/service-novafs.txt"
cp "${NOVA_KERNEL_ELF:-$root/target/$rust_target/release/novaos}" "$stage/root/boot/novaos.elf"
xorriso -as mkisofs -R -J -V NOVAOS -e boot/limine-uefi-cd.bin \
    -no-emul-boot -efi-boot-part --efi-boot-image --protective-msdos-label \
    -o "$stage/novaos.iso" "$stage/root"
mv "$stage/novaos.iso" "$iso"
