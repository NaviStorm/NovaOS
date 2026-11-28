# Reproduire le diagnostic visible 6A

Depuis la racine du worktree, utiliser des répertoires de sortie libres et le répertoire temporaire commun prévu par les consignes NovaOS. Ces commandes travaillent sur des fichiers ordinaires. Le script de qualification prend le verrou QEMU partagé.

## Relire la livraison hors VM

```sh
set -eu
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
p=novaOS/docs/validation/live-noyau-6a
d=novaOS/boot/live-6a
v=novaOS/build/relecture-livraison-6a
mkdir -p novaOS/build
mkdir "$v" "$v/preuves"
(cd "$p" && shasum -a 256 -c SHA256SUMS)
python3 -B "$d/extraire.py" "$p/novaos-live-x86_64.img.gz" "$v/novaos-live.img"
sha=9945b4334170495d9a9836f1f8c9224c8293600b33445fc8f7c4d799ba26961a
python3 -B novaOS/boot/live-5z/image.py verifier "$v/novaos-live.img" --sha256 "$sha"
tar -xzf "$p/observations.tar.gz" -C "$v/preuves"
(cd "$v/preuves" && shasum -a 256 -c SHA256SUMS)
python3 -B "$d/contrelecture.py" "$v/preuves"
```

L’extracteur vérifie taille et empreinte de tous les octets avant de publier le fichier. Il préserve les zones clairsemées et refuse une destination existante. Les empreintes du microcode OVMF et de son magasin de variables accompagnent les mesures nominales. [Rapport et image](../../docs/live-noyau-6a.md)

## Reconstruire puis observer les démarrages

Le banc reprend les outils 5Z, dont les cibles Rust `x86_64-unknown-none` et `x86_64-unknown-uefi`, LLVM, NASM, GNU Make et QEMU avec OVMF. Fournir au constructeur l’archive officielle `limine-10.8.5.tar.xz` de SHA-256 `84e1e2789a0be980f791627da0e9b189b9ae95cf629acd1994992ba3dc275c5d`. Il l’extrait dans une sortie privée et vérifie son empreinte. [Outillage du banc 5Z](../live-5z/README.md)

```sh
set -eu
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
b=novaOS/build/rejeu-6a
d=novaOS/boot/live-6a
archive=novaOS/build/limine-10.8.5.tar.xz
sha=9945b4334170495d9a9836f1f8c9224c8293600b33445fc8f7c4d799ba26961a
fw=/opt/homebrew/share/qemu/edk2-x86_64-secure-code.fd
vars=/opt/homebrew/share/qemu/edk2-i386-vars.fd
sh novaOS/boot/live-5z/construire.sh "$b" "$archive" 6a 4005560320
python3 -B novaOS/boot/live-5z/image.py verifier "$b/novaos-live.img" --sha256 "$sha"
for cfg in 256-1 384-2 512-4; do
    python3 -B "$d/qualifier.py" "$b" "$b/$cfg" --firmware "$fw" --variables "$vars" \
        --sha256 "$sha" --ram "${cfg%-*}" --cpus "${cfg#*-}"
done
for cas in relais memoire profil topologie sans-surface sans-vga integrite; do
    python3 -B "$d/qualifier.py" "$b" "$b/$cas" --firmware "$fw" --variables "$vars" \
        --sha256 "$sha" --cas "$cas" --ram 384 --cpus 1
done
make -C novaOS abi-check
```

Pour vérifier la reproductibilité, relancer le constructeur dans un **deuxième répertoire libre**, puis comparer les deux fichiers bruts avec `cmp`. Les sorties QMP, GDB, série et captures de scanout sont conservées par scénario. Les PPM proviennent de QMP. Les caractères attendus sont calculés sur l’hôte avec la table ASCII de référence `font8x8` 0.3.1 (domaine public). Les adresses de symboles sont résolues dans l’ELF de cette construction.
