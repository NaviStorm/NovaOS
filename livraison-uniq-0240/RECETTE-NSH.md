# Rejouer sur le candidat v0.30.0

Relever le commit du candidat, le SHA-256 de chaque ELF embarqué et la configuration de la VM. Les mesures archivées ici ont utilisé deux vCPU et 256 Mio. Le fichier `gros-fichier.txt` doit déjà exister sur le volume P9. La recette hôte `lab/guest.py` le crée avec une graine de 27027, conserve sa distribution exacte et produit le fichier trié de référence.

Dans la session série de l’invité, exécuter les commandes suivantes. Dans le lecteur du manuel, appuyer sur `q` pour revenir à l’invite.

```sh
export LANG=fr-CA
uniq --help
uniq --version
man uniq
uniq -u -- -villes
uniq -di cities
uniq -Di cities
uniq -s 1 -c cities
uniq -z --group=both binary sortie-binaire
uniq -c trie
sort gros-fichier.txt | uniq -c
echo $?
cat cities | wc -l
cat cities | head -n 1
```

La sortie `sortie-binaire` exige un nom libre. Relire ses octets hors ligne. `$?` donne le statut du lecteur. Dans le profil de qualification `cli-test`, relever séparément `[pipe sort=0]` et `[pipe uniq=0]`. Pour le corpus de 8 192 lignes, le résultat comporte 64 clés dans l’ordre C. Les effectifs de la première et de la dernière clé, puis tous les effectifs intermédiaires, sont disponibles dans `preuves/qemu/<architecture>-<secteur>/corpus.json`. Le flux de sortie entier doit correspondre au résultat indépendant.

## Reconstruction depuis la racine du worktree

```sh
export NOVAOS_TMPDIR=/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
novaFS/lab/run build --offline --bin novafs
L=livraison-uniq-0240
"$L/reconstruire.sh" macOS
"$L/reconstruire.sh" aarch64 ordinaire
"$L/reconstruire.sh" aarch64 documentation
"$L/reconstruire.sh" x86_64 ordinaire
"$L/reconstruire.sh" x86_64 documentation
for isa in aarch64 x86_64; do
  case "$isa" in
    aarch64) target=aarch64-unknown-none-softfloat ;;
    x86_64) target=x86_64-unknown-none ;;
  esac
  export NOVA_KERNEL_ELF="$TMPDIR/uniq-system-ordinaire-$isa/$target/release/novaos"
  doc="$TMPDIR/uniq-system-documentation-$isa/$target/release/novaos"
  for sector in 512 4096; do
    base=$(mktemp -d "$TMPDIR/uniq-recette.XXXXXXXX")
    python3 -B "$L/lab/guest.py" "$isa" "$sector" "$base/campagne" --rows 8192 --sort --doc-kernel "$doc"
    NOVA_KERNEL_ELF="$doc" python3 -B "$L/lab/restitution.py" "$isa" "$sector" "$base/campagne" "$base/temoin"
  done
done
```

`lab/mesures_invite.py ARCH SECTEUR DESTINATION` exerce `-c`, `-D` et `--group` vers de nouveaux fichiers, la très longue ligne et le compteur 10 000 019. `lab/grand_invite.py ARCH SECTEUR DESTINATION --large` prépare 268 436 480 octets sur P9 et tente le vrai pipeline. `--rows 4096` sélectionne le palier de 4 Mio à enregistrements de 1 024 octets. Utiliser un répertoire de destination neuf. Le frontal `novafs script` plafonne ses images à 128 Mio. Le générateur hôte `image_p9.rs` utilise directement le moteur P9 du worktree pour les grands supports.

## Relecture des preuves

Les archives contiennent les ISO effectivement démarrées, les journaux UART, la configuration QEMU et les disques quand le banc les utilise. Dans un répertoire de vérification neuf, extraire `session.tar.gz` puis exécuter `shasum -a 256 -c /chemin/vers/sources.sha256`. La matrice JSON publiée à côté de l’archive redérive les deux statuts depuis les octets UART. Les octets observés et le verdict original sont conservés dans l’archive. Les sondes de droits utilisent le noyau construit avec `NOVA_DOC_TRACE=1`. Les binaires distribués proviennent du profil ordinaire.
