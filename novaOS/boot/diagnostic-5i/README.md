# Reproduire l’observation 5I

Le chemin qualifié utilise le médiateur de QEMU sur vCPU suspendu au premier RIP EFI. Le banc contrôle la fenêtre de registres dans le FlatView du CPU puis termine QEMU en gardant le débogueur connecté. Le PE terminal peut être exercé séparément jusqu’à HLT. Le profil `5i-temoin` exerce les rappels UEFI pour conserver la contre-épreuve. La reprise d’une VM entre ces deux bornes a son propre verdict. [Résultats et conditions de 5J](../../docs/diagnostic-uefi-5i.md)

## Relire les octets publiés

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/" PYTHONDONTWRITEBYTECODE=1
python3 -B novaOS/boot/diagnostic-5i/verifier.py
```

## Rejouer le laboratoire x86_64

Depuis la racine du worktree. Le banc ouvre des fichiers ordinaires et réserve lui-même le verrou partagé pour chaque série. QEMU 11.1.3, OVMF x86_64 SMM archivé en 5E, Python 3.11, pyfatfs et virt-firmware sont utilisés dans les versions du laboratoire 5H. Les clés privées de préparation ont été détruites par le constructeur. Les images et les magasins publics archivés permettent de retrouver exactement les octets mesurés. Le témoin VT-d actif utilise le modèle, le firmware, le magasin et le FAT d’origine du banc VirtIO historique. L’outil en vérifie les SHA avant exécution. [Environnement 5H](../diagnostic-5h/README.md)

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/" PYTHONDONTWRITEBYTECODE=1
mkdir -p novaOS/build
lot=$(mktemp -d "$PWD/novaOS/build/rejeu-5i.XXXXXXXX")
d=novaOS/boot/diagnostic-5i
p=novaOS/docs/validation
t="$PWD/../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools"
py="$t/venv/bin/python"
(cd "$p/diagnostic-uefi-5e" && shasum -a 256 -c SHA256SUMS)
(cd "$p/diagnostic-uefi-5h" && shasum -a 256 -c SHA256SUMS)
(cd "$p/diagnostic-uefi-5i" && shasum -a 256 -c SHA256SUMS)
tar -xzf "$p/diagnostic-uefi-5e/laboratoire.tar.gz" -C "$lot" ovmf
tar -xzf "$p/diagnostic-uefi-5i/laboratoire.tar.gz" -C "$lot"
rustup run stable rustc --edition 2024 -O "$d/relire.rs" -o "$lot/relire"
# Les adresses de calibration publiées permettent la capture sans aucune reprise de l’EFI.
tar -xzf "$p/diagnostic-uefi-5i/observations.tar.gz" -C "$lot" terminal
"$py" -B "$d/suspendu.py" "$lot/supports" "$lot/ovmf/code.fd" "$lot/terminal" "$lot/captures" .. "$lot/relire"
python3 -B "$d/actif.py" ../archives-virtio-net/finale-20261125 "$lot/domaines" ..
```

Pour étudier le comportement **après reprise**, exécuter séparément les deux commandes suivantes. Chaque campagne conserve les cellules et quitte en échec si une écriture apparaît entre l’entrée et HLT. Le verdict négatif fait partie des résultats à conserver. Les magasins UEFI et le support SATA des essais sont des copies privées du laboratoire.

```sh
"$py" -B "$d/strict.py" "$lot/supports" "$lot/ovmf/code.fd" "$lot/efi" .. "$lot/relire"
```

```sh
"$py" -B "$d/temoin.py" "$lot/temoin" "$lot/ovmf/code.fd" "$lot/rappels" ..
```

Pour les trois paires de mesures par topologie, utiliser les calibrations publiées et les mêmes médias. Le chronomètre cumule les deux périodes de reprise du débogueur. La capture de la topologie, les arrêts du vCPU et les vérifications hors VM sont exclus de cette mesure.

```sh
mkdir "$lot/5h"
tar -xzf "$p/diagnostic-uefi-5h/supports.tar.gz" -C "$lot/5h"
python3 -B "$d/comparer.py" "$lot/5h" "$lot/supports" "$lot/ovmf/code.fd" "$lot/terminal" "$lot/temps" ..
```

## Reconstruire et tester le code

```sh
src=novaOS/boot/diagnostic-5e
for profil in 5i 5i-temoin; do
  NOVA_DIAGNOSTIC_PROFILE="$profil" NOVA5E_BUILD="$lot/target-$profil" sh "$src/construire.sh"
done
export RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)"
for arch in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup run stable cargo test --locked --offline --release --target "$arch" --test passif \
    --features diagnostic-5i --manifest-path "$src/Cargo.toml" --target-dir "$lot/tests"
done
python3 -B "$d/test_domaines.py" -v
make -C novaOS abi-check
```

Une nouvelle signature de laboratoire utilise `preparer.py --phase5h` selon le protocole 5H. Elle produit un autre lot de clés et d’empreintes. Le PE terminal non signé publié a été reproduit à l’identique. Le banc compare le RIP, le RVA et douze octets de code dans le PE du FAT et la RAM de la VM.
