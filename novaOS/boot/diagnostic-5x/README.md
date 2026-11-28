# Reproduire le candidat STANDARD 5X sur fichiers ordinaires

Depuis la racine du worktree, utiliser la chaîne figée par la [qualification STANDARD 5W](../diagnostic-5w/README.md) et le répertoire temporaire partagé NovaOS. Ces commandes produisent uniquement des fichiers de laboratoire. Chaque série QEMU exige un dossier de sortie neuf. [Rapport](../../docs/diagnostic-uefi-5x.md) · [Fiche A8](../../docs/qualification-live-5x/a8-msi.md)

## PE, disques et copie simulée

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/"
export PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5x
w=novaOS/boot/diagnostic-5w
v=novaOS/docs/validation/diagnostic-uefi-5x
b=novaOS/build/rejeu-5x
mkdir -p novaOS/build
mkdir "$b"
sh "$w/construire.sh" "$b/rust"
sh "$w/construire.sh" "$b/repro"
rel=x86_64-unknown-uefi/release/standard.efi
cmp "$b/rust/$rel" "$b/repro/$rel"
python3 -B "$d/images.py" "$b/rust/$rel" > "$b/images.json"
cmp "$b/images.json" "$v/images.json"
(cd "$v" && shasum -a 256 -c SHA256SUMS)
gzip -dc novaOS/docs/validation/diagnostic-uefi-5w/novaos-standard-68mio.img.gz > "$b/novaos-standard-68mio.img"
python3 -B "$w/image.py" verifier "$b/rust/$rel" "$b/novaos-standard-68mio.img" --octets 71303168
shasum -a 256 "$b/novaos-standard-68mio.img"
```

Le test crée ses propres fichiers ordinaires exclusifs dans un répertoire temporaire de `novaOS/build`. Il passe les descripteurs ouverts à `dd` par entrée et sortie standard. Les trois géométries produisent successivement des fichiers temporaires, avec environ 1,1 Gio de place libre nécessaire pour la plus grande copie. La génération à une autre capacité utilise le même `image.py` et un nombre exact d’octets multiple de 512.

## Tests hôtes et relecture des preuves

```sh
export RUSTC="$(rustup which --toolchain stable rustc)"
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup run stable cargo test --offline --locked --release --target "$cible" \
        --manifest-path "$w/Cargo.toml" --target-dir "$b/hotes" --test standard --test console
    rustup run stable cargo test --offline --locked --release --target "$cible" \
        --manifest-path novaOS/boot/diagnostic-5e/Cargo.toml --target-dir "$b/hotes" \
        --features diagnostic-5i --test passif
done
for mode in arm64 x86_64; do
    /usr/bin/arch "-$mode" /usr/bin/python3 -B "$w/verifier.py"
    /usr/bin/arch "-$mode" /usr/bin/python3 -B "$d/relire.py"
done
```

La contre-lecture RENFORCÉ utilise le PE historique exact conservé avec les preuves 5R. Les commandes suivantes relisent les archives et les deux refus sur USB ordinaire sans démarrer de VM.

```sh
pr=../diagnostic-uefi-phase5r/novaOS/build/5r/rust/x86_64-unknown-uefi/release/diagnostic-5r.efi
for mode in arm64 x86_64; do
    /usr/bin/arch "-$mode" /usr/bin/python3 -B novaOS/boot/diagnostic-5r/qualification.py verifier \
        novaOS/docs/validation/diagnostic-uefi-5r/observations.zip "$pr"
    /usr/bin/arch "-$mode" /usr/bin/python3 -B novaOS/docs/qualification-live-5s/controler.py "$pr" \
        --archive novaOS/docs/validation/diagnostic-uefi-5w/renforce-5s.zip
done
```

## Nouvelle cellule QEMU

```sh
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
fw=/opt/homebrew/share/qemu/edk2-x86_64-secure-code.fd
vars=/opt/homebrew/share/qemu/edk2-i386-vars.fd
"$py" -B "$w/supports.py" "$b/rust/$rel" "$vars" "$b/supports"
"$py" -B "$d/console.py" "$fw" "$b/supports" "$b/console"
```

Le banc exige le code OVMF et l’image qualifiés. Il contrôle le magasin de laboratoire, le RIP aux deux bornes et le CRC de la table système avant de simuler la perte de ConOut. Le vCPU reprend avant la première instruction du PE. Le scénario s’arrête si la calibration historique ne correspond pas à cette configuration. Les compteurs, registres, empreintes du canari et du magasin et les traces délimitent l’observation. Le magasin actif initial de l’essai publié porte le SHA-256 `536a14c41cd779445d77aeccd00bc2fe35f4aa68918f6108d7254a709d321e40`. Un magasin reconstruit peut différer par ses métadonnées. La VM travaille toujours sur une copie. Le nouveau scénario ne requiert aucune ancienne campagne QEMU.
