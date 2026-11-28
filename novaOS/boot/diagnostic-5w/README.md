# Reproduire le diagnostic STANDARD 5W

Depuis la racine du worktree dédié sous macOS Apple Silicon, utiliser Rust stable avec les cibles Darwin ARM64, Darwin x86_64 et x86_64-unknown-uefi. Les dépendances Rust sont verrouillées. La construction ne lance aucune VM. [Contrat des deux profils et résultats](../../docs/diagnostic-uefi-5w.md)

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/"
export PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5w
v=novaOS/docs/validation/diagnostic-uefi-5w
b=novaOS/build/rejeu-5w
mkdir -p novaOS/build
mkdir "$b"
sh "$d/construire.sh" "$b/rust"
pe="$b/rust/x86_64-unknown-uefi/release/standard.efi"
(cd "$v" && shasum -a 256 -c SHA256SUMS)
gzip -dc "$v/standard.efi.gz" > "$b/standard-reference.efi"
cmp "$pe" "$b/standard-reference.efi"
gzip -dc "$v/novaos-standard-68mio.img.gz" > "$b/reference.img"
python3 -B "$d/image.py" verifier "$pe" "$b/reference.img" --octets 71303168
export RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)"
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup run stable cargo test --offline --locked --release --test standard --target "$cible" \
        --manifest-path "$d/Cargo.toml" --target-dir "$b/hotes"
done
sh novaOS/boot/diagnostic-5u/qualifier.sh
for mode in arm64 x86_64; do
    /usr/bin/arch "-$mode" /usr/bin/python3 -B "$d/verifier.py"
done
```

## Démarrages de laboratoire

Le constructeur de supports réutilise l’environnement Python de 5E (virt-firmware et virt-fw-vars). Il prépare deux magasins virtuels neufs depuis le gabarit distribué et une image altérée pour les refus. Aucun certificat privé n’est requis. Le catalogue embarqué est le catalogue expérimental 5E. Le PE publié est autorisé dans la base db de laboratoire par son empreinte Authenticode.

```sh
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
fw=/opt/homebrew/share/qemu/edk2-x86_64-secure-code.fd
vars=/opt/homebrew/share/qemu/edk2-i386-vars.fd
"$py" -B "$d/supports.py" "$pe" "$vars" "$b/supports"
python3 -B "$d/banc.py" "$fw" "$b/supports" "$b/qemu"
```

Le banc prend le verrou partagé avant le premier lancement. Il utilise le code OVMF distribué, un disque interne vierge privé et un support USB virtuel en lecture seule. L’observateur externe calibre les points d’arrêt, lit la mémoire et les compteurs et capture les pixels. Seule la cellule TES actif fournit un stimulus après LoadImage. Sa racine sans droit réside dans une table ACPI de test, son emplacement est retrouvé via le XSDT et sa page est vérifiée vide. Le banc refuse d’intervenir si une racine ou un état actif préexiste. Aucun octet du PE ni aucune preuve de confiance n’est injecté. Tous les fichiers de preuve bruts restent dans le répertoire de série neuf.

## Témoin RENFORCÉ

Utiliser le PE historique exact (SHA-256 `7f80bf3c42c89162d67fb229b9080e9706c0f7f6a3574978051c1c0c555477a5`) et le magasin actif 5R pour le rejeu QEMU. La commande `temoin.py` prend elle-même le verrou commun. L’archive 5R et l’archive des deux refus rejoués en 5W se relisent sans VM.

```sh
r=../diagnostic-uefi-phase5r/novaOS/build/5r
pr="$r/rust/x86_64-unknown-uefi/release/diagnostic-5r.efi"
for mode in arm64 x86_64; do
    /usr/bin/arch "-$mode" /usr/bin/python3 -B -m unittest discover -s novaOS/boot/diagnostic-5r
    /usr/bin/arch "-$mode" /usr/bin/python3 -B novaOS/boot/diagnostic-5r/qualification.py verifier \
        novaOS/docs/validation/diagnostic-uefi-5r/observations.zip "$pr"
    /usr/bin/arch "-$mode" /usr/bin/python3 -B novaOS/docs/qualification-live-5s/controler.py "$pr" --archive "$v/renforce-5s.zip"
done
"$py" -B novaOS/docs/qualification-live-5s/temoin.py "$pr" "$r/magasin/actif.fd" rejeu5w
```
