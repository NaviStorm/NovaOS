# Image Live x86_64 de laboratoire

[Rapport et matrice avant A8](../../docs/diagnostic-uefi-5n.md) · [Image prête à décompresser](../../docs/validation/diagnostic-uefi-5n/novaos-x86_64.img.gz)

## Obtenir l’image et vérifier ses structures

Depuis la racine du worktree dédié, dans un shell POSIX, exécuter dans un répertoire de travail neuf. La bibliothèque standard Python 3.9 ou ultérieure suffit pour construire et relire l’image. Les licences et le PE proviennent des archives publiques épinglées 5E et 5M.

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5n
v=novaOS/docs/validation/diagnostic-uefi-5n
mkdir -p novaOS/build
b=$(mktemp -d "$PWD/novaOS/build/rejeu-5n.XXXXXXXX")
(cd "$v" && shasum -a 256 -c SHA256SUMS)
gzip -dc "$v/novaos-x86_64.img.gz" > "$b/livraison.img"
python3 -B "$d/support.py" creer "$b/reconstruit.img"
cmp "$b/livraison.img" "$b/reconstruit.img"
shasum -a 256 "$b/livraison.img"
python3 -B "$d/support.py" verifier "$b/livraison.img"
python3 -B "$d/verifier5n.py"
```

Le SHA-256 brut attendu est `c69df9dc41cf5cfff7bf0fb81765f3287111656ad6c8575bdb2db8d94d2e9c92`. Le générateur crée exclusivement des fichiers neufs. Pour une géométrie différente, par exemple un disque virtuel de 96 Mio, ajouter `--mio 96`. Si la capacité est connue en secteurs logiques de 512 octets, utiliser `--secteurs N`. Cette option place la seconde GPT au dernier secteur exact. Les volumes FAT conservent leurs tailles.

## Écriture avec dd sous macOS

**Seuls des fichiers réguliers de laboratoire ont servi de destination en phase 5N.** Cette commande reproduit sur Mac la copie brute qualifiée. L’option du shell interdit l’écrasement d’un fichier existant.

```sh
(set -C; dd if="$b/livraison.img" bs=1m > "$b/cle-virtuelle.img")
cmp "$b/livraison.img" "$b/cle-virtuelle.img"
python3 -B "$d/support.py" verifier "$b/cle-virtuelle.img"
```

**Procédure opérateur réservée à une autorisation physique ultérieure.** La copie brute écrase la table des partitions et les fichiers existants de la destination. Prévoir un média externe dédié et vide. Identifier séparément le disque entier externe avec `diskutil list external physical`, puis contrôler avec `diskutil info` son caractère externe, sa capacité exacte et la taille de secteur logique. Abandonner si plusieurs supports correspondent, si la destination est interne, si l’identité change ou si la géométrie n’est pas celle de l’image vérifiée. Les GUID de l’image sont publics et identiques sur chaque copie.

Après autorisation physique distincte, la procédure consisterait à démonter **tous** les volumes du disque externe sélectionné avec `diskutil unmountDisk`, à écrire le fichier décompressé sur **le disque entier** et à relire exactement sa longueur pour comparer le SHA-256. Sur macOS, la forme de la commande est `sudo dd if=IMAGE_VERIFIEE of=/dev/rdiskN bs=1m`. La lettre N représente l’identifiant vérifié du média dédié. Une variante terminée par `s1` ou `s2` désignerait une partition et détruirait sa structure. Éjecter uniquement après synchronisation et contre-lecture. A8 est fermé, la commande physique n’a pas été exécutée.

## Rejouer les démarrages et l’export

Les variables et le firmware sont ceux des archives déjà livrées. Prévoir QEMU 11.1.3 x86_64, Python 3.9+, les outils macOS et le répertoire de worktrees partagé du laboratoire. Le banc acquiert lui-même le verrou de qualification. Il faut une destination de campagne neuve et des fichiers ordinaires. Aucun support physique ni périphérique en passthrough n’intervient.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/"
fw="$b/ovmf.fd"
tar -xOzf novaOS/docs/validation/diagnostic-uefi-5e/laboratoire.tar.gz ovmf/code.fd > "$fw"
python3 -B "$d/qualifier.py" "$b/livraison.img" "$fw" "$b/serie" "$b/captures"
python3 -B "$d/controle_modes.py" "$b/livraison.img" "$fw" "$b/hors-db" > "$b/modes.json"
python3 -B "$d/lire.py" "$b/serie/standard/usb.img"
python3 -B "$d/lire.py" "$b/serie/standard/usb.img" --json
```

À chaque lancement, le terminal du banc présente le refus extrait du PE et les états de sécurité. Le lecteur hors ligne vérifie la géométrie et les sceaux avant de présenter les rapports validés. Un rapport interrompu est signalé. Un ancien sceau invalide entraîne un refus. Le JSON de la série publiée omet volontairement toute date non attestée en lui donnant la valeur `null`. Le dépositaire peut ajouter `--horloge-utc-attestee` à la campagne si l’horloge hôte a été vérifiée. Les identifiants de démarrage changent à chaque rejeu. L’image de distribution reste reproductible.

Sur une **copie** déjà montée en lecture seule, ouvrir le répertoire du démarrage. On peut y afficher `rapport.json` et `JOURNAL.TXT`. Depuis ce répertoire, `head -n 2 VALIDES.SHA | shasum -a 256 -c -` contrôle les octets des deux fichiers, bourrage inclus. Un sceau absent, nul ou incomplet interdit de considérer les fichiers comme publiés.

## Contre-lectures par les systèmes hôtes

Le script macOS attache une image régulière en lecture seule, utilise un point de montage temporaire privé et détache uniquement le disque virtuel retourné par cet attachement. Les lecteurs Windows et Linux amorcent chacun une VM ARM64 jetable. Les ISO sont fournies localement par le laboratoire. Le lecteur Windows utilise LLVM, le lien COFF de Rust, xorriso et Tesseract. Il place le petit témoin de lecture sur un CD distinct de NOVALOGS et compare les octets au CD. Le fichier source livré permet de reconstruire ce témoin.

```sh
img="$b/serie/standard/usb.img"
zip="$b/captures/rapports.zip"
python3 -B "$d/lecture_macos.py" "$img" "$zip" > "$b/lecture-macos.json"
# ISO_WINPE et ISO_ALPINE désignent les ISO ARM64 de laboratoire en lecture seule.
# QEMU_SHARE contient edk2-aarch64-code.fd et edk2-arm-vars.fd.
# Placer rust-lld dans PATH si le sysroot de rustc ne contient pas ce binaire.
python3 -B "$d/lecture_windows.py" "$ISO_WINPE" "$img" "$zip" "$QEMU_SHARE" "$b/winpe" > "$b/lecture-windows.json"
python3 -B "$d/lecture_linux.py" "$ISO_ALPINE" "$img" "$zip" "$b/linux" > "$b/lecture-linux.json"
python3 -B "$d/support.py" compresser "$b/livraison.img" --sortie "$b/novaos-x86_64.img.gz"
python3 -B "$d/verifier5n.py" "$b"
```

La relecture Windows qualifiée utilise le WinPE ARM64 français issu du support Windows 11 du laboratoire et un lecteur de console. Une autre disposition de clavier ou une autre édition peut demander une adaptation du lecteur de banc. Le témoin ouvre les 48 fichiers de comparaison, uniquement pour lecture, via Kernel32. Le WinPE fonctionne en RAM. Les ISO et les installations Windows ne font pas partie des livrables.

## Non-régression

```sh
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "$d" -v
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/verifier5n.py"
  /usr/bin/arch "-$isa" /usr/bin/python3 -B novaOS/boot/diagnostic-5m/verifier.py
done
for phase in i j k l m; do
  (cd "novaOS/docs/validation/diagnostic-uefi-5$phase" && shasum -a 256 -c SHA256SUMS)
done
make -C novaOS abi-check
```
