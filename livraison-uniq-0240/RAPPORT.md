# ARC-027 · uniq 0.24.0 · 16 novembre 2026

Toutes les formes GNU demandées ont passé leurs cas discriminants sur Darwin ARM64 et sur les deux architectures NovaOS. Le pipeline des binaires livrés a restitué tous les comptes à 1 Gio sur le Mac et à 256 Kio dans les quatre invités. Le profil natif supérieur aux 256 Mio de RAM reste ouvert.

| Cible et secteur logique | Cas réussis | Plus grand pipeline validé | Statuts sort / uniq | uniq seul puis pipeline, en secondes |
|---|---:|---:|---:|---:|
| Darwin ARM64 natif | 435/435 | 1 Gio | 0 / 0 | 0,210 / 3,093 |
| QEMU AArch64 · 512 | 121/121 | 256 Kio | 0 / 0 | 2,859 / 12,587 |
| QEMU AArch64 · 4096 | 121/121 | 256 Kio | 0 / 0 | 2,779 / 12,348 |
| QEMU x86_64 · 512 | 121/121 | 256 Kio | 0 / 0 | 2,383 / 11,398 |
| QEMU x86_64 · 4096 | 121/121 | 256 Kio | 0 / 0 | 3,189 / 11,326 |

## Options et contrats d’E/S

La [matrice par option](MATRICE.md) réunit les formes courtes, longues, les méthodes, leurs combinaisons et les cas refusés. Elle renvoie aux arguments, entrées, sorties et statuts. Deux locales ont été exercées sur le Mac et dans les invités. Le différentiel Darwin vérifie aussi stderr en neutralisant le nom d’invocation. Les résultats de la suite amont, les statuts natifs et la divergence volontaire de protection d’un fichier existant sont documentés dans les [références](REFERENCES.md). Le moteur commun traite progressivement les enregistrements. Le dernier groupe, les lignes incomplètes et les NUL sont couverts. Le compteur 10 000 019 a été vérifié sur Darwin, AArch64 et x86_64.

Les traces des quatre configurations montrent que le lecteur `uniq` reçoit zéro capacité fichier. `cat cities | uniq cities`, `uniq cities | wc -l` et `sort cities | head -n 1` sont refusés. `cat cities | wc -l` et `cat cities | head -n 1` rendent 0 et produisent la sortie attendue. Les lectures de chemin hors périmètre sont refusées. Les consultations `--help`, `--version` et `man uniq` ont conservé l’empreinte de P9 pour les trois étiquettes de langue invitées. La sonde du chemin documentaire ELF compte zéro droit métier. Le catalogue passe ses cinq tests. L’ABI v1 compte toujours 48 sélecteurs et le format disque reste P9.

Les témoins de restitution comparent les campagnes à une session `pwd` du même profil. Après retrait, les compteurs utilisateur, processus et IPC valent zéro. Les neuf cadres DMA sur x86_64 sont aussi présents dans le témoin. Les onze suites système exercées sur chacune des deux architectures rendent 0. Les commandes, journaux, résultats de collecte et images sont archivés. [Régressions](preuves/regressions/) · [Restitution](preuves/restitution/) · [Recette nsh](RECETTE-NSH.md)

## Débit et mémoire de uniq seul

Sur le fichier trié de 1 Gio, `-c` traite 4 867 Mio/s, soit 4,98 millions de lignes/s, pour 2,08 Mio de RSS maximale. `-D` atteint 2 374 Mio/s et 2,05 Mio de RSS, et `--group` 2 561 Mio/s et 2,06 Mio. À enregistrement fixé à 128 octets, le banc varie de 2 048 à 131 072 lignes et de 3 à 1 024 clés. Avec des enregistrements de 1 Mio, les trois modes atteignent environ 4 Mio de RSS. Ces valeurs proviennent d’un passage par profil, sans purge des caches. [Mesures Darwin](preuves/mesures.json) · [Mesures GNU](preuves/mesures-gnu.json)

Dans les invités, les dix essais autonomes complémentaires passent sur AArch64/512 et sur x86_64/4096. Les sorties P9 ont été relues hors ligne. Les campagnes couvrent `-c`, `-D`, `--group`, des enregistrements de 32 Kio et un grand groupe de 10 000 019 enregistrements NUL. Ce dernier prend environ 52 s sur AArch64 et 43 s sur x86_64. Les durées et débits sont dans les rapports `mesures.json` des archives. Les instantanés noyau mesurent les ressources de la VM. Le pic mémoire individuel des processus et le pic des temporaires de tri invités restent non mesurés. [Mesures natives](preuves/mesures-natives/)

## Pipeline et limite observée

Sur le Mac, la graine 27027 produit 1 048 576 lignes de 1 024 octets, 2 048 clés, un singleton et des occurrences éloignées. Le corpus de tri de 160 Gio portait des numéros de ligne distincts et ne convenait pas à la vérification des groupes. L’oracle du générateur, le pipeline GNU et celui du projet donnent la même sortie complète. Les première et dernière fréquences valent 524 288 et 256. La somme vaut 1 048 576. Le pipeline du projet atteint 331 Mio/s, contre 375 Mio/s pour GNU. Les pics RSS du projet sont de 519 Mio pour le tri et 2,08 Mio pour uniq. Le maximum temporaire du tri échantillonné toutes les 5 ms est de 1 073 750 016 octets. [Deux profils Darwin](preuves/pipeline.json)

Dans chacun des quatre invités à deux vCPU, les vrais ELF communiquent par le pipe natif. Le fichier mélangé de 8 192 lignes de 32 octets est présent sur P9. Les 64 comptes, leur ordre, le dernier groupe et le total correspondent au générateur et à GNU. La sortie ordinaire est comparée octet par octet. Le profil instrumenté relève séparément les deux statuts. Les fréquences et les empreintes figurent dans chaque `corpus.json`. [Quatre campagnes QEMU](preuves/qemu/)

| Palier natif | Configurations exercées | Observation |
|---|---|---|
| 4 194 304 octets | AArch64/512 et x86_64/4096 | Aucun compte. Le lecteur arrive à son échéance de 15 000 ticks avant la première donnée. Le producteur écrit ensuite sur le pipe fermé. Statuts sort 2, uniq 8. |
| 268 436 480 octets, RAM de 268 435 456 octets | Les quatre | Le grand fichier est présent sur P9. Le démarrage du stockage se termine avant l’invite nsh. Le service rend 26 et le client 8. Aucun des deux ELF n’a démarré. |

Le grand support de 264 193 blocs disposait encore de 197 433 blocs de 4 Kio après création du corpus. La cause interne de l’arrêt du stockage reste à établir. Le plus grand succès observé est de 256 Kio. Le seuil exact entre ce succès et l’échec à 4 Mio n’a pas été recherché. Les traces, disques, empreintes et recettes permettent de reprendre ces paliers. [Palier de 4 Mio](preuves/palier-4Mio/) · [Palier supérieur à la RAM](preuves/superieur-ram/)

## Sources et reproduction

Base vérifiée `origin/main` au départ : `ea2d0c3cd5a38671ebdf263c8eeca72b3cabcff2`. Code de uniq et intégration du lanceur : `6b94f2f6349add9fc6ac0bc8276ab1b8747a5e36`. Sort provient de `codex/sort-natif-0240` au commit `fe650613d531e1d224b40cc569f933c89648d7c3`. Les trois exécutables de sort ont été repris à ce commit et vérifiés. La coordination avec sort et sha256sum réserve les images 17/18 à uniq/sort et 19 à sha256sum. [Empreintes des binaires et des noyaux](preuves/binaires.json) · [SHA-256 de sort par cible](preuves/sort.sha256) · [Références GNU et manuel](preuves/references.sha256)

Depuis la racine du dépôt, la sélection `--macOS` exerce les outils GNU et `--novaOS` les Mach-O du projet. Le profil `--large` active les gros essais. Les captures QEMU ont leur propre [recette v0.30.0](RECETTE-NSH.md).

```sh
cd livraison-uniq-0240
shasum -a 256 -c SHA256SUMS
./tester-options-uniq.sh --novaOS --large
./tester-pipeline-sort-uniq.sh --novaOS --large
./mesurer-performances-uniq.sh --novaOS --large
```
