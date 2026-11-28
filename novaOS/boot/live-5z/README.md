# Reproduire le Live USB du noyau 5Z

Exécuter depuis la racine d’un worktree NovaOS. Les chemins de sortie ci-dessous doivent être libres. Le banc crée des disques ordinaires privés et prend le verrou QEMU partagé du projet. Les seules sorties de l’invité sont la console et la série. Le tableau des profils et la portée de l’admission figurent dans le [rapport de qualification](../../docs/diagnostic-uefi-5z.md).

## Vérifier le candidat publié

```sh
set -eu
d=novaOS/boot/live-5z
p=novaOS/docs/validation/diagnostic-uefi-5z
b=novaOS/build/relecture-5z
mkdir -p novaOS/build
mkdir "$b"
(cd "$p" && shasum -a 256 -c SHA256SUMS)
gzip -dc "$p/novaos-live-x86_64.img.gz" > "$b/novaos-live.img"
sha=c0e0d01c03d7cd6c8214c5bf1cbadd46cc9855aa010f6dace3204e264a5e2c83
python3 -B "$d/image.py" verifier "$b/novaos-live.img" --sha256 "$sha"
tar -xzf "$p/observations.tar.gz" -C "$b"
for cfg in 256-1 384-2 512-4; do
    python3 -B "$d/relire.py" "$b/nominal-$cfg" --sha256 "$sha"
done
```

## Reconstruire et rejouer

Le banc a utilisé Darwin ARM64, rustc 1.99.1, les cibles `x86_64-unknown-none` et `x86_64-unknown-uefi`, Cargo avec les dépendances verrouillées disponibles hors ligne, clang/LLVM 23.1.6, LLD livré avec Rust, NASM, GNU Make et QEMU 11.1.3 TCG. Le constructeur vérifie l’archive amont avant extraction, applique le correctif USB dans un répertoire privé et construit aussi l’outil d’inscription du condensat de configuration. [Archive source officielle Limine v10.8.5](https://github.com/Limine-Bootloader/Limine/releases/download/v10.8.5/limine-10.8.5.tar.xz)

```sh
set -eu
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp" TMPDIR="$(dirname "$PWD")/tmp/"
export PYTHONDONTWRITEBYTECODE=1
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
d=novaOS/boot/live-5z
b=novaOS/build/rejeu-5z
mkdir -p novaOS/build
archive=novaOS/build/limine-10.8.5.tar.xz
test -f "$archive" || curl -fLsS -o "$archive" https://github.com/Limine-Bootloader/Limine/releases/download/v10.8.5/limine-10.8.5.tar.xz
sh "$d/construire.sh" "$b" "$archive"
sha=c0e0d01c03d7cd6c8214c5bf1cbadd46cc9855aa010f6dace3204e264a5e2c83
python3 -B "$d/image.py" verifier "$b/novaos-live.img" --sha256 "$sha"
fw=/opt/homebrew/share/qemu/edk2-x86_64-secure-code.fd
vars=/opt/homebrew/share/qemu/edk2-i386-vars.fd
for cfg in 256-1 384-2 512-4; do
    python3 -B "$d/banc.py" "$b" "$b/$cfg" --firmware "$fw" --variables "$vars" \
        --mode qualifier --sha256 "$sha" --ram "${cfg%-*}" --cpus "${cfg#*-}"
    python3 -B "$d/relire.py" "$b/$cfg/nominal-$cfg" --sha256 "$sha"
done
python3 -B "$d/banc.py" "$b" "$b/refus" --firmware "$fw" --variables "$vars" \
    --mode refus --ram 384 --cpus 1
for isa in arm64 x86_64; do
    /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/test_support5z.py" "$b" "$sha" -v
    /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s novaOS/boot/diagnostic-5n
    /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s novaOS/boot/diagnostic-5o
done
make -C novaOS abi-check
```

Le premier démarrage de chaque géométrie localise l’entrée du PE. Le suivant réutilise une copie vierge des variables OVMF, vérifie indépendamment le RIP, la table système, les compteurs bloc et les jalons du noyau. Le fichier VARS livré avec QEMU garde Secure Boot inactif. Les empreintes initiales du CODE et du VARS accompagnent chaque mesure. Dans le mode `refus`, quatre copies synthétiques sont présentées directement au premier EFI afin d’exercer ses propres vérifications.

Pour une autre capacité virtuelle, `python3 -B "$d/image.py" assembler "$b/stage" "$b/autre.img" --octets 87040512` génère une autre GPT avec les mêmes composants. L’empreinte de publication ci-dessus correspond exclusivement aux 68 Mio qualifiés.
