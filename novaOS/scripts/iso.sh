#!/bin/sh
set -eu
root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/target.sh"

(cd "$root/third_party/limine" && shasum -a 256 -c SHA256SUMS)
iso="$root/$iso"
mkdir -p "$root/build"
stage=$(mktemp -d "${NOVAOS_TMPDIR:-$root/build}/iso.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/root/boot" "$stage/root/EFI/BOOT"
limine="$root/third_party/limine"
esp="$stage/root/EFI/BOOT"
if [ "$efi_loader" = "BOOTAA64.EFI" ]; then
    sh "$root/scripts/construire-relais-bootid.sh" "$esp/$efi_loader"
    cp "$limine/$efi_loader" "$esp/LIMINE.EFI"
    python3 -B "$root/scripts/efi_relais_cd.py" "$limine/limine-uefi-cd.bin" \
        "$esp/$efi_loader" "$stage/root/boot/limine-uefi-cd.bin"
else
    cp "$limine/limine-uefi-cd.bin" "$stage/root/boot/"
    cp "$limine/$efi_loader" "$esp/"
fi
cp "$root/boot/limine.conf" "$stage/root/"
if [ "$#" -ne 0 ]; then
    [ "$#" -eq 2 ] || exit 2
    case "$1" in
        cache-pression|cache-propriete-invalide|cache-test|cache-reprise|cache-coupure|cache-restaure|cache-reinstalle|cache-actualise|cache-reprise-purge|journal-admin|services-config-race|services-config-admin|services-admin|services-test|services-perte|instances-reboot|tmp-test|tmp-coupure|tmp-reprise|tmp-attentes|cli-p8|cli-p8-test) ;;
        instances-reboot=*|instances-coupure=*)
            identity=${1#*=}
            [ "${#identity}" -eq 64 ] || exit 2
            case "$identity" in *[!0123456789abcdef]*) exit 2 ;; esac
            ;;
        nvme-rw-revoke-before|nvme-rw-revoke-after|nvme-rw-timeout-flush|nvme-rw-reuse|nvme-rw-init|nvme-rw-update|nvme-rw-recover|nvme-rw-rights|nvme-rw-race|nvme-rw-revoke-write|nvme-rw-revoke-flush|nvme-rw-timeout|nvme-rw-driver-death|nvme-rw-client-death|nvme-rw-dma-fault|nvme-rw-qi-error|nvme-rw-qi-delay|nvme-rw-cut-before-write|nvme-rw-cut-during-write|nvme-rw-cut-after-write|nvme-rw-cut-during-f1|nvme-rw-cut-after-f1|nvme-rw-cut-during-publish|nvme-rw-cut-during-f2|nvme-rw-cut-after-f2|nvme-lecture|nvme-permission|nvme-permission-read|nvme-mort|nvme-client|nvme-retrait|nvme-delai|vtd-edu-test|vtd-edu-quarantaine|vtd-edu-erreur-qi|vtd-edu-jamais-soumise|vtd-edu-fin-incertaine|materiel-pilote-qemu|materiel-inventaire|materiel-inventaire-qemu|ipc-smp-equite|ipc-smp-libre|ipc-smp-local|ipc-smp-distant|ipc-smp-paires|ipc-smp-paires-sonde|ipc-smp-paires-cpu0|ipc-smp-paires-libres|ipc-smp-calcul|ipc-smp-memoire|ipc-smp-vfs|ipc-smp-sonde|threads-test|sdk-example|smp-test|smp-arret|smp-panique|smp-ap-absent|smp-cpu-muet|smp-fp|cli|cli-test|namespace-f1-inflight|namespace-f2-inflight|namespace-disconnect|namespace-depth|namespace-inspect|namespace-write|namespace-read|namespace-rename|namespace-f1-before|namespace-f1-after|namespace-f2-before|namespace-f2-after|files-recover|files-f1-before|files-f1-inflight|files-f1-after|files-f2-before|files-f2-inflight|files-f2-after|files-data|files-meta|files-close|files-remount|files-inject|files-info|files-abandon|files-restart|files-perf|files-write|files-read|elf-test|storage-page-write|storage-page-read|storage-live|storage-write|storage-read|storage-perf|storage-eio|dma-flight|dma-fence|dma-user|dma-qual|storage-test|ipc-test|ipc-endurance|ipc-perf|ipc-races|abi-test|instances-test|proc-lifecycle|proc-test|proc-boot-fault|mem-test|mem-ro|mem-alias|mem-page-ro|mem-nx|mem-absent|heap-test|heap-oom|heap-corrupt|heap-reentrant|heap-metadata|heap-guard|heap-infallible|sched-test|sched-stress|sched-endurance|sched-guard|sched-fpu) ;;
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
