# Reproduire le diagnostic 5H sur deux Q35 de laboratoire

Depuis la racine du worktree dédié, utiliser QEMU x86_64 11.1.3, TCG/SMM, Python 3.11 et virt-firmware 26.10. La relecture des preuves archivées utilise seulement la bibliothèque standard Python. Le firmware et ses notices sont conservés avec la phase 5E. La livraison 5H contient les supports signés et altérés, les trois magasins OVMF de laboratoire, les certificats publics et les notices applicables. L’EFI et les clés admises par ce laboratoire n’ont vocation à être déployés sur aucun poste physique.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/"
h=novaOS/docs/validation/diagnostic-uefi-5h
(cd "$h" && shasum -a 256 -c SHA256SUMS)
python3.11 -B novaOS/boot/diagnostic-5h/relire.py
```

Pour rejouer les octets exacts, utiliser le même environnement que 5F et un répertoire neuf. Le banc réserve le verrou commun avant chaque série et ne raccorde à QEMU que des périphériques simulés et des copies de fichiers ordinaires. Les deux magasins nominal et inactif contiennent les mêmes PK, KEK, db et dbx. L’unique différence de valeur en entrée est le choix OVMF `SecureBootEnable`. Le diagnostic n’écrit pas cette variable. Le magasin révoqué sert de témoin séparé.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/"
export PYTHONDONTWRITEBYTECODE=1
t="$PWD/../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools"
py="${PY5H:-$t/venv/bin/python}"
h=novaOS/docs/validation/diagnostic-uefi-5h
e=novaOS/docs/validation/diagnostic-uefi-5e
(cd "$h" && shasum -a 256 -c SHA256SUMS)
(cd "$e" && shasum -a 256 -c SHA256SUMS)
mkdir -p novaOS/build
lot=$(mktemp -d "$PWD/novaOS/build/rejeu-5h.XXXXXXXX")
mkdir "$lot/supports"
tar -xzf "$e/laboratoire.tar.gz" -C "$lot" ovmf
tar -xzf "$h/supports.tar.gz" -C "$lot/supports"
"$py" -B novaOS/boot/diagnostic-5h/qualifier.py "$lot/supports" "$lot/ovmf/code.fd" "$lot/serie" --worktrees ..
```

Pour reconstruire le PE Rust et constituer un **autre** jeu de clés de laboratoire, conserver les variables et les chemins de la commande précédente. Rust stable, la cible `x86_64-unknown-uefi`, les dépendances verrouillées en cache, cryptography, pyfatfs, virt-fw-vars et osslsigncode sont nécessaires. Le choix explicite de 5H préserve la construction 5E par défaut. La génération des clés et signatures ne reproduit pas le SHA du média déjà signé. La construction par défaut conserve le comportement 5E. Son PE reconstruit depuis les sources modifiées de cette branche diffère des octets historiques. Pour un rejeu ou une reconstruction exacte de 5E, utiliser les archives et la révision 5E indiquées dans sa procédure.

```sh
src=novaOS/boot/diagnostic-5e
NOVA_DIAGNOSTIC_PROFILE=5h NOVA5E_BUILD="$lot/target" sh "$src/construire.sh"
pe="$lot/target/x86_64-unknown-uefi/release/diagnostic.efi"
shasum -a 256 "$pe"
"$py" -B "$src/preparer.py" --phase5h "$pe" "$lot/ovmf/gabarit.fd" "$lot/nouveaux-supports" \
  --signer "${SIGN5H:-$t/sign-build/osslsigncode}" --var-tool "${VARS5H:-$t/venv/bin/virt-fw-vars}"
```

Tests ciblés du lecteur et du catalogue sur les deux cibles Darwin de la toolchain stable :

```sh
export RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)"
for arch in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup run stable cargo test --locked --offline --release --target "$arch" --test passif \
    --features diagnostic-5h --manifest-path novaOS/boot/diagnostic-5e/Cargo.toml --target-dir "$lot/tests"
done
```

[Résultats et demandes A7](../../docs/diagnostic-uefi-5h.md)
