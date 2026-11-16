# Livraison ARC-027 · tr · 16 novembre 2026

**Darwin ARM64 a réussi les 3 652 comparaisons avec GNU tr 9.12. Le moteur a réussi ces mêmes 3 652 cas dans chacun des quatre invités QEMU. L’admission de l’ELF de production par nsh reste ouverte.** Depuis le vrai shell, `cat entree | tr '[:lower:]' '[:upper:]'` et `cat entree | tr -s ' '` rendent 3 avant la création des deux processus. Le témoin positif `cat entree | wc -l` lit le même fichier et rend 259 lignes.

| Livrable | SHA-256 |
|---|---|
| [Mach-O Darwin ARM64](bin/tr) | `30c43ada0b43d245ab93a3456df3b81695df8859e3cbd9c4afbd47f195222e24` |
| [ELF NovaOS AArch64](bin/tr-aarch64.elf) | `19fef766414c1f5106a1dacef0348bd9680da62f36aaf8ebadfec86197934fa1` |
| [ELF NovaOS x86_64](bin/tr-x86_64.elf) | `4c7850e0b81908e2f012ee7a457dc5751dec1089ae21965faefb774d19044744` |

La base de départ vérifiée sur origin/main est `2e3188dee6eaef859becca3919bd0d53a20674bb`. Pendant la qualification, origin/main a intégré NovaAuth phase 3 à `425e4e7`. Les chiffres ci-dessous portent sur la base du mandat. Le commit du composant est `66bcd2a9792886f3203bfd880b9398f986356ac4`. Le numéro `0.20.0` affiché par `tr --version` provient du manifeste réel de nova-cli. Le code source du lanceur partagé, les 48 sélecteurs ABI v1 et le format P9 correspondent à la base. [Sources de reconstruction](sources-66bcd2a.tar.gz) · [Étude des références et licences](REFERENCES.md) · [Recettes par cible](REPRODUIRE.md)

## Fonctionnement observé

Le Mach-O traite les données en octets. Il reproduit la classification observée sur le GNU Homebrew en C et en en_US.UTF-8. Le différentiel conserve stderr brut et distingue le nom de l’exécutable des différences de contenu. Les quatre cas supplémentaires par locale couvrent les options inconnues ou ambiguës contenant un octet non ASCII. Au premier passage, Nova réencodait cet octet dans le diagnostic. Le correctif conserve les octets d’origine et reproduit stderr de GNU. Les deux suites GNU `tr.pl` et `tr-case-class.sh` rendent 0. Le bloc ISO-8859-1 conditionnel du script shell a été sauté par son propre test de locale. Les rapports documentaires, le cas d’un répertoire présenté en stdin, la position de stdin après aide/version/rejet et le signal SIGPIPE sont archivés. Les deux pipelines avec le vrai cat de Darwin rendent 0/0 et produisent les octets de GNU. Les 5 249 espaces consécutifs du deuxième pipeline deviennent un espace malgré le découpage en blocs de 128 octets. [Matrice par option](MATRICE.md) · [Pipelines Darwin](preuves/pipe-mac.json) · [Journaux amont et de construction](preuves/journaux-mac.tar.xz)

Dans chaque profil NovaOS, le vrai nsh passe 21 consultations ou rejets préalables sur trois étiquettes linguistiques. L’aide et le manuel choisissent le français pour fr-CA et l’anglais pour en-US et fr--CA. Le SHA du volume P9, le nombre d’octets écrits, les opérations d’écriture et les flush sont identiques avant et après ces essais. Trois tentatives distinctes de pipe vers tr rendent 3 dans chacun des quatre profils, dont l’option inconnue. L’appel direct `tr --inconnue` rend 1 avec son diagnostic avant lecture métier. Les deux ISO SDK par profil contiennent le témoin instrumenté dont l’empreinte a été vérifiée dans le noyau extrait de l’ISO. Chaque témoin compare les octets avant rendu, distingue trame supprimée et EOF, exerce les erreurs de lecture et de fermeture et termine avec restitution confirmée par le noyau. [Couverture par profil](preuves/couverture-qemu.json) · [Sessions AArch64](preuves/sessions-aarch64.tar.xz) · [Sessions x86_64](preuves/sessions-x86_64.tar.xz)

Les PID et statuts d’un couple cat/tr, le retrait sous pression et la restitution de son vrai pipe restent à qualifier après admission. Sur décision de Thierry, la proposition d’extension du lanceur est séparée. [Identifiants et périmètre soumis à validation](ARBITRAGE-PIPE.md)

## Performances Darwin

Mac ARM64 Mac16,5, macOS 27.2.1, RAM de 128 Gio, locale C. Un échauffement par binaire précède trois passages en ordre alterné. La vérification des sorties se fait séparément. Le débit utilise les octets d’entrée. Le pic RSS maximal des 48 passages Nova vaut 2 260 992 octets, celui des 48 passages GNU 1 949 696 octets. Les corpus textuels comptent exactement 1, 10 et 65 millions de LF. Le dernier inclut une première suite de 1 Mio sans interruption. Le corpus binaire compte 33 554 432 octets. [Tableau terminal complet](preuves/tableau-mac.txt) · [Passages, tailles, empreintes et RSS](preuves/mesures-mac.json)

Sur 65 millions de lignes, la médiane de traduction est de 0,1390 s pour Nova et 0,3467 s pour GNU. Sur le binaire de 32 Mio, GNU est plus rapide en suppression et en compression.

## Performances du moteur dans les invités

Durées en secondes pour 65 millions de lignes, soit 521 048 576 octets. La RAM mesurée par QMP est de 268 435 456 octets. Dans les quatre profils, le tas observé avant et après chaque parcours vaut 131 072 octets. La boucle réutilise deux blocs de 4 Kio et ses tables, sans allocation proportionnelle au flux. Le chronométrage par marqueurs UART sur l’hôte englobe la copie dans le tampon et le comptage. Il s’agit d’un passage de traitement en mémoire du témoin SDK pour chaque mode et taille. Le débit du pipeline P9 de production et son pic RSS total restent à mesurer.

| Profil QEMU TCG, 2 vCPU | Traduction | Suppression | Compression | Suppression puis compression |
|---|---:|---:|---:|---:|
| AArch64 · P9/512 | 1,11 | 2,58 | 3,57 | 2,42 |
| AArch64 · P9/4096 | 1,08 | 2,58 | 3,53 | 2,40 |
| x86_64 · P9/512 | 1,36 | 2,99 | 4,07 | 2,83 |
| x86_64 · P9/4096 | 1,37 | 2,99 | 4,11 | 2,84 |

La plus grande entrée réussie dans le témoin est de 521 048 576 octets. Les sorties correspondantes comptent respectivement 521 048 576, 261 048 576, 325 000 000 et 130 000 000 octets. Ces tailles concordent avec GNU. L’espace disponible avant génération est consigné pour chaque corpus dans le JSON Darwin. [64 mesures QEMU](preuves/mesures-qemu.json) · [Tableau QEMU par profil et par taille](preuves/tableau-qemu.txt)

## Régressions du système

Les onze cibles de référence ont rendu 0 sur AArch64 et sur x86_64. Elles couvrent le démarrage, les ELF, le cycle de vie, les courses IPC, le bloc, la CLI, P9, nsh, le terminal et vi. La série de régression a commencé à `6352660` et s’est terminée après le correctif `66bcd2a` du diagnostic. Le différentiel, les suites GNU, les quatre recettes TR en invité et leurs mesures portent sur le binaire corrigé. Les cinq tests du catalogue passent sur Darwin ARM64 natif et sous Rosetta x86_64. Les neuf tests du registre ABI passent aussi. [Journaux et traces de régression par cible](preuves/regressions.tar.xz) · [Registre ABI](preuves/registre-abi.txt)

~~~sh
make -C novaOS ARCH=aarch64 test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test
make -C novaOS ARCH=x86_64 test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test
~~~
