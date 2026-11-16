# Vérifier une livraison depuis nsh

Démarrer une installation NovaOS contenant le catalogue de cette branche. Placer les fichiers et le manifeste SHA256SUMS de provenance connue sur P9. Ouvrir le répertoire de la livraison. Les deux images P9 de démonstration fournies placent directement les fichiers à la racine. Sur ces images, utiliser `cd /` à la place de `cd /livraison`.

```sh
cd /livraison
sha256sum --version
SHA256SUM_READ_1=fichier.bin
SHA256SUM_READ_2=notice
sha256sum -c SHA256SUMS
echo $?
```

La version attendue est `sha256sum (NovaOS) 0.24.0`. Chaque ligne doit porter `OK` et le statut doit valoir 0. L’administrateur choisit les noms autorisés d’après l’inventaire de livraison de confiance. Un nom lu dans le manifeste ne confère aucun droit. Pour les sous-répertoires, employer le nom exact de l’inventaire, par exemple `SHA256SUM_READ_2=doc/notice`. Les variables sont locales à la session. Utiliser `export` pour les transmettre à un appel imbriqué de nsh.

```sh
sha256sum --quiet -c SHA256SUMS
echo $?
sha256sum fichier.bin
cat fichier.bin | sha256sum
```

La vérification silencieuse réussie n’émet aucune ligne. Les deux dernières commandes donnent la même empreinte, suivie respectivement de `fichier.bin` et de `-`. Les supports `novaos/aarch64/NovaOS-cli.iso` et `novaos/x86_64/NovaOS-cli.iso` contiennent le catalogue ordinaire utilisé pendant la qualification. Rattacher une copie de `novaos/livraison-P9-512.raw` rendue accessible en écriture pour la session, comme volume de données VirtIO à secteurs logiques et physiques de 512 octets. Pour le témoin négatif, répéter la recette avec une copie de `novaos/livraison-modifiee-P9-512.raw`. Cette image contient le manifeste original et un fichier altéré d’un bit. La ligne du fichier porte `FAILED`, les autres concordances restent visibles et le statut vaut 1.

Une entrée `voisin` sans autorisation explicite reste en échec, y compris avec `--ignore-missing`. La consultation de l’aide, de la version et de `man sha256sum` fonctionne sans préparer de fichier. L’empreinte vérifie la concordance avec le manifeste. Son authenticité dépend de sa provenance.
