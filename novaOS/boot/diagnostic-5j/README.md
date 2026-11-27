# Rejouer la protection DMA précoce 5J

Le processus hôte possède une seule VM TCG créée à froid et garde le vCPU arrêté jusqu'au verdict. Les scripts ne prennent aucun périphérique hôte en argument. Les fichiers de preuves, le modèle compilé et les nouveaux répertoires de construction sont distincts. [Contrat et qualification](../../docs/diagnostic-uefi-5j.md)

## Contre-lecture sans QEMU

Depuis la racine du worktree, Python 3 suffit. Le vérificateur contrôle les empreintes de livraison, les compteurs QMP, les traces de traduction, les copies de tables et les réponses RAM prises indépendamment du firmware.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5j
v=novaOS/docs/validation/diagnostic-uefi-5j
python3 -B "$d/verifier.py"
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "$d" -v
  /usr/bin/arch "-$isa" /usr/bin/python3 -B novaOS/boot/diagnostic-5i/test_domaines.py -v
done
make -C novaOS abi-check
```

## Rejeu des supports signés

Dans un **nouveau worktree** préparé avec le même outillage Darwin, vérifier les empreintes avant extraction. L'archive contient exclusivement les entrées publiques du banc et le correctif ACPI PIO d'origine. Le modèle QEMU est construit dans le répertoire privé du rejeu à partir de l'archive amont 11.1.3 (`5a8289c7940dc77d3873c81a6c29ba3afe1446fc9d36e77fa1658cdbecb84b22`). La recette vérifie aussi le SHA du correctif avant toute compilation. Un nouveau binaire QEMU a sa propre identité enregistrée dans chaque résultat. Avec le modèle exact déjà présent à l'emplacement de compilation 5J, omettre l'option de reconstruction. [Sources QEMU](https://download.qemu.org/qemu-11.1.3.tar.xz)

```sh
(cd "$v" && shasum -a 256 -c SHA256SUMS)
mkdir -p novaOS/build
tar -xzf "$v/laboratoire.tar.gz" -C novaOS/build
q=novaOS/build/source-modele
mkdir -p "$q/amont"
cp ../archives-virtio-net/finale-20261125/modele-acpi-pio/amont/qemu-11.1.3.tar.xz "$q/amont/"
python3 -B "$d/banc.py" novaOS/build/rejeu-5j --reconstruire-modele "$q"
```

Pour le témoin historique à 28 feuilles, utiliser les binaires d'origine. Le premier programme acquiert le verrou QEMU, vérifie les empreintes et prend deux clichés. Le second compare les tables au bail indépendant du courtier.

```sh
a=../archives-virtio-net/finale-20261125
python3 -B novaOS/boot/diagnostic-5i/actif.py "$a" novaOS/build/rejeu-herite-5j ..
python3 -B "$d/herite.py" "$a" novaOS/build/rejeu-herite-5j
```

## Reconstruction du firmware et du PE

La construction du firmware utilise une copie neuve de l'arbre EDK II PIO historique, révision `fbe0805b2091393406952e84724188f8c1941837`. La recette contrôle le diff historique des sources et le fichier BDS supplémentaire. Elle conserve SMM, la négociation des SMI et l'enregistrement du script S3. Le PE utilise le catalogue, les clés publiques de test et l'allocateur Rust du diagnostic existant. Le constructeur de supports 5H crée une nouvelle identité UEFI de laboratoire à chaque invocation. Conserver ces nouveaux supports à part pour une qualification distincte.

```sh
export PATH="$PWD/../tmp/ovmf-net-outils-20261125/acpica/20260408/bin:/opt/homebrew/opt/llvm/bin:$PATH"
bash "$d/firmware.sh" ../tmp/ovmf-q35-pio-20261125 novaOS/build/ovmf-reconstruit-5j
sh "$d/supports.sh" "$a/q35-pio-firmware-02/OVMF_VARS.fd" novaOS/build/nouveaux-supports-5j
export RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)"
for arch in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup run stable cargo test --release --locked --offline --target "$arch" --features diagnostic-5i \
    --test passif --manifest-path novaOS/boot/diagnostic-5e/Cargo.toml --target-dir "novaOS/build/tests-$arch"
done
```

La capture intégrale temporaire de RAM demande 256 Mio de place par invité actif. Le banc en extrait le reçu de 80 octets, vérifie l'adresse propre de la structure et supprime le fichier temporaire. Les traces complètes QEMU et les copies des tables destinées à la contre-lecture figurent dans l'archive d'observations.
