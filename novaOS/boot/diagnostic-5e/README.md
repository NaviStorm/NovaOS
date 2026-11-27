# Reproduire la phase 5E en laboratoire

Le diagnostic x86_64 est une application UEFI autonome qui remet son rapport en RAM et quitte les services de démarrage. Le support distribué est un fichier FAT16 compressé. Toutes les commandes ci-dessous opèrent sur des **fichiers ordinaires** du worktree ou des VM de laboratoire. Le banc crée lui-même le disque SATA fictif et n’utilise aucun périphérique hôte en passthrough. [Résultats, réserves et décision A8](../../docs/diagnostic-uefi-5e.md)

## Relire la livraison hors invité

Depuis la racine de la branche dédiée, avec Python 3.11 ou suivant, la commande vérifie les sommes des archives et de leurs membres, les réponses QMP, le rapport mémoire, les traces, les statuts de refus et les résultats hôtes.

```sh
python3.11 -B novaOS/boot/diagnostic-5e/verifier_livraison.py
```

Les empreintes identifient les artefacts. La racine d’authentification du premier exécutable est le certificat autorisé par db, sous réserve de dbx. Le profil laboratoire et son magasin OVMF sont fournis pour rejouer le même test. Aucun secret de signature PE n’est livré.

## Rejouer les mêmes octets sous QEMU

Banc qualifié avec QEMU 11.1.3, OVMF x86_64 Secure Boot et SMM. La copie du firmware, son gabarit de variables, les magasins de test, les certificats publics et les médias de refus sont dans `laboratoire.tar.gz`. Les journaux et les compteurs de référence sont dans `preuves.tar.gz`. Utiliser un nouveau répertoire de sortie à chaque replay. Ces commandes supposent la disposition des worktrees prescrite par le projet.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
preuves=novaOS/docs/validation/diagnostic-uefi-5e
mkdir -p novaOS/build
rejeu=$(mktemp -d "$PWD/novaOS/build/rejeu-5e.XXXXXXXX")
(cd "$preuves" && shasum -a 256 -c SHA256SUMS)
tar -xzf "$preuves/laboratoire.tar.gz" -C "$rejeu"
(cd "$rejeu" && shasum -a 256 -c SHA256SUMS)
gzip -dc "$preuves/diagnostic-x86_64.img.gz" > "$rejeu/supports/diagnostic.img"
python3.11 -B novaOS/boot/diagnostic-5e/qualifier.py \
  "$rejeu/supports" "$rejeu/ovmf/code.fd" "$rejeu/resultats" --worktrees ..
```

Le banc acquiert le verrou commun avant les démarrages. Le stockage interne de chaque VM est une copie **amorçable et signée** de l’image de référence. Cela rend détectable le repli du firmware même lorsque le premier EFI externe a été rejeté. Le statut `valide` de chaque cellule signifie que les observations et le verdict attendu concordent. Lire aussi `zero_io_interne_depuis_reset`, `repli_interne_observe`, `commandes_pci_entree` et `porte_physique`. Le résultat physique reste bloqué. Aucun échec de politique n’est converti en réussite de démarrage.

## Reconstruire et signer un nouveau lot synthétique

Le noyau de diagnostic se construit avec Rust stable 1.99.1, la cible `x86_64-unknown-uefi`, les dépendances du `Cargo.lock` en cache et le répertoire temporaire privé du projet. Le compilateur est sélectionné via `rustup which`. Le PE exclut le chemin PDB et l’horodatage de lien. Pour vérifier la reproductibilité, une deuxième valeur de `NOVA5E_BUILD` désigne un autre répertoire de compilation. La nouvelle génération d’identités de laboratoire change le PE signé et le SHA du support. Pour retrouver exactement l’artefact qualifié, utiliser le replay ci-dessus.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
src=novaOS/boot/diagnostic-5e
sh "$src/construire.sh"
NOVA5E_BUILD="$PWD/novaOS/build/phase5e-verif-pe" sh "$src/construire.sh"
cmp novaOS/build/phase5e-target/x86_64-unknown-uefi/release/diagnostic.efi \
    novaOS/build/phase5e-verif-pe/x86_64-unknown-uefi/release/diagnostic.efi
```

Pour fabriquer des images **virtuelles** dans un nouveau répertoire, l’environnement Python du banc contient `virt-firmware==26.10`, `cryptography==50.0.2`, `pyfatfs==1.1.0` et `setuptools==80.10.2`. Il expose `virt-fw-vars`. L’outil de signature testé est osslsigncode 2.10, révision `8329a14f8b8f69bcffbc3b59b885a42e1256cd08`. Les options `--var-tool` et `--signer` acceptent leurs chemins explicites. Le constructeur génère PK, KEK et la clé db dans le périmètre du lot. Chaque clé a un rôle distinct. Le secret de signature sert uniquement pendant la fabrication des témoins. [Projet osslsigncode](https://github.com/mtrojnar/osslsigncode)

```sh
python -B "$src/preparer.py" \
  novaOS/build/phase5e-target/x86_64-unknown-uefi/release/diagnostic.efi \
  "$rejeu/ovmf/gabarit.fd" "$rejeu/nouveaux-supports"
osslsigncode verify -CAfile "$rejeu/nouveaux-supports/db.pem" \
  -in "$rejeu/nouveaux-supports/diagnostic.efi"
python3.11 -B "$src/qualifier.py" "$rejeu/nouveaux-supports" \
  "$rejeu/ovmf/code.fd" "$rejeu/nouveaux-resultats" --worktrees ..
```

Les petits manifestes et signatures du catalogue sont livrés dans `vecteurs/`. Leur génération est déterministe avec `python -B "$src/vecteurs.py"` dans le même environnement Python. La clé publique de test et le paquet ELF de référence proviennent des vecteurs du catalogue 4B. Les tests hôtes ciblés s’exécutent avec `sh "$src/regression.sh" "$rejeu/tests-hotes"` sur Darwin ARM64 avec Rosetta et les deux cibles Rust Darwin installées.

## Lire le résultat mémoire

Le banc capture le rapport hors de l’invité. Son en-tête de 112 octets commence par `NOVA5E-RAM-FINAL` suivi de douze entiers little-endian de 64 bits (adresse du rapport, entrée EFI, arrêt, base PE, statut, nombre de tables, présence DMAR, nombre de DRHD, nombre de nœuds, consommation d’arène, allocations vivantes, adresse de la routine de sortie EFI). Suivent au maximum 64 enregistrements de 72 octets. Le premier quadruplet indique type, compatibilité, plan et activation. La compatibilité vaut 0 sans candidat, 1 pour le candidat sélectionné, 2 en cas de conflit. L’empreinte de manifeste suit, puis la localisation éventuelle (profondeur, périphérique, fonction, octet réservé et empreinte du chemin). Les adresses sont propres à la VM et au démarrage.

Le banc conserve le rapport binaire borné, le relevé CPU, les compteurs du backend, les événements de trace et les décisions OVMF. Les données d’une exécution avec statut non nul ne contiennent aucun résultat matériel partiellement admis. La sonde ne fournit pas d’interface console, de persistance ni d’autorisation d’activation. [Format et statuts côté noyau](main.rs) · [Lecture indépendante côté hôte](banc.py)
