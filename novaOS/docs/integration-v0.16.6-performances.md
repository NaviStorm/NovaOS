# Sauvegarde durable de vi sur la fusion v0.16.6

La grille a validé 80 sauvegardes. Les 22 observations ordinaires du profil groupé sont restées sous une seconde. Sur la campagne, le maximum est de 131,940 ms dans QEMU AArch64, de 198,201 ms dans QEMU x86_64 et de 909,640 ms dans UTM. Le maximum en classique atteint respectivement 10 444,747, 8 731,638 et 61 721,585 ms. L’ELF du paquet a donné une 81e sauvegarde classique avec redémarrage et relecture. Les observations couvrent le profil mono-vCPU et des arrêts de VM sur Mac alimenté. La variabilité déjà observée dans UTM subsiste.

## Méthode

La révision testée est `a0e7b000c5bf4d3246361ad13978540ec5c4aaf2`. Chaque invité ouvre un volume neuf pour l’essai, écrit vers un nom libre, confirme par `OUI` et attend « fichier enregistré ». L’horloge monotone de l’hôte encadre la saisie de `w copie` et la réception du succès sur le terminal. Le relevé donne aussi le temps après `OUI`. La création du volume, la saisie du texte, la navigation, le redémarrage et la relecture sont hors chronométrage.

| Scénario | Source | Destination attendue |
|---|---|---|
| `neuf-4m` | Tampon vide | 4 194 304 octets ASCII |
| `inserer-milieu` | Fichier dense de 4 Mio | 4 194 306 octets après insertion de Ω au milieu |
| `ajout-4m-multiple` | Même fichier dense | 8 388 609 octets après suppression d’un octet près de la fin, insertion de 4 Mio au milieu et insertion de Ω au début |

Sur le scénario localisé, le noyau classique sans sonde provient de la construction courante ordinaire. Pour les deux grandes entrées, une fixture privée prépare les octets dans le tampon de vi avant le départ du chronomètre. En mode classique, elle alimente une seule opération d’édition par fragments de 4 Kio. Une tentative de calibration qui fournissait les 4 Mio en un seul appel interne était restée dans la boucle d’éviction de l’historique avant la commande de sauvegarde. L’interface classique insère normalement les caractères par frappes. Le profil groupé utilise sa fixture de charge publiée. Les vecteurs et l’oracle sont communs. [Fixture classique](../tests/fixtures/io-0166-charge-classique.patch) · [Vecteurs et vérification P9](../tests/es_groupees_elargies.py)

Le chemin classique utilise les interfaces publiées. Pour les octets nouveaux, vi transporte au plus 127 octets par message vers nsh. Le dialogue de fichier transporte jusqu’à 88 octets utiles par trame. Le moteur peut publier plusieurs transactions CoW de préparation avant la dernière publication du nom. Le profil groupé négocie explicitement le plan et la file bloc. Sa barrière F1 précède la publication de l’ancre et F2 précède le retour du succès. Les deux chemins utilisent le polling VirtIO. [Dialogue vi](../../components/libraries/vi_wire.rs) · [Protocoles de stockage](es-v0.16.6-negociation.md)

Un deuxième passage avec sondes compte les échanges du seul appel de sauvegarde entre vi, nsh, NovaFS, relais et pilote. Le compteur vérifie la symétrie requête/réponse des quatre liaisons, les dons acquittés, les contrôles noyau et les contrôles du prêt, y compris la négociation de fonctionnalités. Les copies locales sont ventilées séparément. La borne de 64 s’applique au total conservateur, contrôles locaux compris. Le pic invité est l’écart entre les cadres libres au repos initial et leur minimum pendant la session entière. Chaque cadre mesure 4 Kio. Les mesures par processus portent sur les pages privées et la pile aux points d’observation du banc. Les sondes donnent ces chiffres également sur le chemin classique. La session publiée en mode 41 n’émet pas ce maximum. [Calcul des compteurs](../tests/vi_ipc_sauvegarde.py)

L’hôte est un Apple M4 Max, Mac16,5, 128 Gio, macOS 27.2 build 26B123. Les invités QEMU 11.1.3 utilisent TCG, un vCPU et une image brute en cache writeback. L’application UTM 5.0.8 utilise son moteur QEMU, un Cortex-A72 émulé, un seul vCPU, GICv3 sans ITS et un QCOW2 clairsemé. Le moniteur QEMU mesure indépendamment les lectures, écritures et FLUSH de cette sauvegarde. La politique UTM reste ordinaire, avec les traces IOMMU/bloc du profil et sans injection de charge par le banc. Les tâches du Mac peuvent mobiliser les cœurs ou le disque pendant la mesure.

Chaque sortie valide passe par `novafs verify`, l’inspection des deux ancres et la relecture octet par octet de la source et de la destination par un lecteur hôte indépendant. Sous UTM, l’éditeur rouvre d’abord la destination après redémarrage, puis le banc exporte le disque arrêté, le convertit en brut et vérifie les deux GPT et leur volume P9. Les empreintes des sources, outils et ELF, des journaux et de la destination identifient les essais. La campagne conserve les charges hôte et les compteurs HMP mesurés. Une coupure électrique du Mac demeure hors du périmètre observé.

## Incidents de collecte

Lors des quatre premières cellules QEMU, le chargement des outils UTM par Python a déplacé la racine utilisée pour lire le journal série de l’invité. Trois collectes ont rejeté le journal absent. Une quatrième pouvait relire un journal préexistant. Le lanceur vérifie désormais la racine des modules CLI et celle du lecteur avant de charger les auxiliaires UTM. Les quatre cellules ont été réexécutées intégralement sur de nouveaux disques. Pendant la cellule UTM 61, le processus de collecte a été interrompu au deuxième démarrage, avant la relecture indépendante. Cette tentative a été archivée et la cellule a été rejouée sur une nouvelle VM.

## Mesures individuelles

Les cellules ordinaires fournissent le temps entre la commande et le succès, en millisecondes. Pour le cas multiple UTM, les trois nombres suivent l’ordre des répétitions. Les mesures de la sonde appariée fournissent le total IPC, contrôles inclus, les FLUSH et le pic de cadres de 4 Kio de la session. Dans ces trois colonnes, les valeurs sont données dans l’ordre classique / groupé. [Données individuelles, lectures, écritures et percentiles](validation/integration-main-v0.16.6-performances.json)

| Environnement | Secteur | Édition | Classique (ms) | Groupé (ms) | IPC | FLUSH | Pic (cadres) |
|---|---:|---|---:|---:|---:|---:|---:|
| QEMU AArch64 | 512 | Création | 4 960,656 | 78,140 | 84 311 / 37 | 132 / 2 | 5 613 / 4 033 |
| QEMU AArch64 | 512 | Milieu | 4 974,603 | 93,825 | 84 336 / 43 | 134 / 2 | 5 869 / 2 990 |
| QEMU AArch64 | 512 | Multiple | 10 444,747 | 128,232 | 168 600 / 56 | 262 / 2 | 11 961 / 9 755 |
| QEMU AArch64 | 4096 | Création | 4 943,882 | 74,915 | 84 311 / 37 | 132 / 2 | 5 613 / 4 033 |
| QEMU AArch64 | 4096 | Milieu | 4 845,946 | 93,908 | 84 336 / 43 | 134 / 2 | 5 869 / 2 990 |
| QEMU AArch64 | 4096 | Multiple | 10 358,913 | 131,940 | 168 600 / 56 | 262 / 2 | 11 961 / 9 755 |
| QEMU x86_64 | 512 | Création | 4 267,998 | 132,416 | 84 311 / 36 | 132 / 2 | 5 621 / 4 044 |
| QEMU x86_64 | 512 | Milieu | 4 244,619 | 137,647 | 84 336 / 43 | 134 / 2 | 5 877 / 3 001 |
| QEMU x86_64 | 512 | Multiple | 8 731,638 | 198,201 | 168 600 / 56 | 262 / 2 | 11 969 / 9 766 |
| QEMU x86_64 | 4096 | Création | 4 274,059 | 127,016 | 84 311 / 36 | 132 / 2 | 5 621 / 4 044 |
| QEMU x86_64 | 4096 | Milieu | 4 308,253 | 135,854 | 84 336 / 43 | 134 / 2 | 5 877 / 3 001 |
| QEMU x86_64 | 4096 | Multiple | 8 640,903 | 192,291 | 168 600 / 56 | 262 / 2 | 11 969 / 9 766 |
| UTM AArch64 | 512 | Création | 23 194,296 | 416,724 | 84 311 / 36 | 132 / 2 | 5 594 / 4 014 |
| UTM AArch64 | 512 | Milieu | 4 510,401 | 92,762 | 84 336 / 43 | 134 / 2 | 5 850 / 2 971 |
| UTM AArch64 | 512 | Multiple | 9 275,481 · 52 127,140 · 51 634,680 | 133,506 · 749,797 · 664,552 | 168 600 / 56 | 262 / 2 | 11 942 / 9 736 |
| UTM AArch64 | 4096 | Création | 23 837,927 | 101,236 | 84 311 / 36 | 132 / 2 | 5 594 / 4 014 |
| UTM AArch64 | 4096 | Milieu | 25 271,771 | 469,460 | 84 336 / 43 | 134 / 2 | 5 850 / 2 971 |
| UTM AArch64 | 4096 | Multiple | 51 739,827 · 52 109,720 · 61 721,585 | 909,640 · 732,769 · 859,135 | 168 600 / 56 | 262 / 2 | 11 942 / 9 736 |

Le témoin UTM de l’ELF effectivement livré a sauvegardé l’insertion locale en **29 816,822 ms**, dont 18 255,931 ms après l’accord. HMP a compté 196 lectures, 1443 écritures et 134 FLUSH. Après redémarrage, les octets de l’ancienne source et de la destination correspondent à l’oracle. Le binaire du paquet n’embarque pas de sonde IPC. [Relecture de l’archive et mesure du binaire livré](validation/integration-main-v0.16.6-temoin-paquet.json)

Les percentiles du JSON emploient le rang supérieur et le nombre d’observations de chaque cellule. Une seule observation est disponible pour la plupart des cellules. Les deux cellules UTM du scénario multiple ont trois répétitions ordinaires pour chaque chemin. Les maxima et tous les temps individuels restent visibles.

## Ressources et portée des mesures

| Chemin | Échanges avec contrôles | Allers-retours interservices et dons seuls | Total incluant les copies locales du prêt | Pic de session (cadres de 4 Kio) | vi privé + pile (Kio aux points de mesure) |
|---|---:|---:|---:|---:|---:|
| classique | 84 311–168 600 | 84 307–168 596 | 84 311–168 600 | 5 594–11 969 | 8 576–20 864 |
| groupe | 36–56 | 27–47 | 42–64 | 2 971–9 766 | 6 600–33 608 |

Le seuil de 64 porte sur les échanges avec contrôles. La colonne incluant les copies locales comptabilise également les opérations mémoire du prêt. Les deux barrières du profil groupé ont été confirmées dans les 18 mesures avec sonde.

## Témoin de distribution

Le banc recoupe le SHA-256 de l’archive complète avec le relevé de livraison. Le manifeste interne identifie les 27 fichiers qu’elle contient. Il extrait le QCOW2, le convertit en brut et lit l’ELF installateur depuis son volume FAT. Cet ELF contient les deux images sectorielles de l’ESP. Après reconstruction des FAT de 512 et 4 096 octets et extraction de `BOOT/NOVAOS.ELF`, les deux fichiers ont pour SHA-256 `3dc5217ad014f5d3b7791f753ebff1f5aa78c7bd5abd1c07db65ad86538944f6`. L’ESP du disque installé et exporté par UTM donne le même SHA-256. [Relevé de livraison](validation/integration-main-v0.16.6-livraison.json)

L’export ordinaire reconstruit pour la grille donne un ELF système différent de celui du paquet. La comparaison des quatre segments ELF chargeables localise tous les octets différents dans l’image embarquée du service NovaFS, `storage-media.bin` (185 752 octets). Les sources Rust et manifestes des composants comparés sont identiques. Les métadonnées Cargo du service donnent le même compilateur, la même cible, le même profil et les mêmes fonctionnalités. La racine absolue du chemin du script de liaison varie et les empreintes des dépendances Cargo diffèrent. Le contenu du script est identique. La cause précise de l’écart binaire reste ouverte. Une cellule supplémentaire utilise directement le binaire du paquet pour l’insertion locale sur secteurs de 512 octets.

## Reproduction

Depuis la racine, renseigner `PROFIL_UTM` avec le paquet système jetable construit par la procédure d’installation et choisir un dossier neuf. Le même commit doit fournir les noyaux, le générateur et le lecteur. Le lanceur accepte `--filtre` pour rejouer certaines cellules et vérifie les empreintes à la reprise. [Profils de construction](../tests/reconstruire_es_0166.py) · [Banc séquentiel](../tests/integration_main_grille.py)

```sh
sortie="$PWD/novaOS/build/rejeu-integration-0166"
python3 -B novaOS/tests/reconstruire_es_0166.py HEAD "$sortie/profils" brut sans sonde utm utm-sonde
python3 -B novaOS/tests/integration_main_performances.py preparer HEAD "$sortie/temoins"
python3 -B novaOS/tests/integration_main_grille.py campagne \
    "$sortie/profils" "$sortie/temoins" "$PROFIL_UTM" "$sortie/mesures"
python3 -B novaOS/tests/integration_main_grille.py bilan \
    "$sortie/profils" "$sortie/temoins" "$PROFIL_UTM" "$sortie/mesures" "$sortie/bilan.json"
```
