# Configuration partagée par la compilation, l’assemblage de l’ISO et le lanceur.
case "${NOVA_ARCH:-x86_64}" in
    x86_64)
        rust_target=x86_64-unknown-none
        efi_loader=BOOTX64.EFI
        iso=build/novaos.iso
        ;;
    aarch64)
        rust_target=aarch64-unknown-none-softfloat
        efi_loader=BOOTAA64.EFI
        iso=build/novaos-aarch64.iso
        ;;
    *)
        echo "Architecture inconnue : $NOVA_ARCH (x86_64 ou aarch64 attendu)" >&2
        exit 2
        ;;
esac
