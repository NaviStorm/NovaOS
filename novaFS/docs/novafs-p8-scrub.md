# NovaFS P8 · Scrub et réparation contrôlée

## Audit préalable

La base inspectée est `a33cc66f54ef779b9f67e5a940cd9437ef3f59d4`, sur `main` propre. Le commit contient déjà le message corrigé de P7.2 et le même arbre que `a196cd2`. P1 impose la synchronisation des dépendances avant publication et une confirmation séparée. P2 contrôle les deux générations FS, P3 leur admission et P5 l’ensemble de leurs références CoW. P6 vérifie les packs et leurs LBA. P7 lie chaque paire à son rôle, sa position, son domaine et sa génération. P7.1 relit le support sous verrou avant et après le drainage. P7.2 prête un index exclusivement pendant la mutation. Un scrub doit repartir du support. [Durabilité](novafs-p1-durabilite.md) · [Intégrité](novafs-p2-integrite.md) · [Partage](novafs-p5-partage.md) · [Compression](novafs-p6-compression.md) · [Authentification](novafs-p7-chiffrement.md) · [Reprise](novafs-p7.1-consolidation.md) · [Exclusivité](novafs-p7.2-performance.md)

| Structure | Détection et localisation | Source de réparation autorisée |
|---|---|---|
| En-têtes P7 | Comparaison des LBA 0 et 1, géométrie canonique, ouverture de l’enveloppe avec la clé administrative. Une divergence seule localise la paire. | Deux LBA indépendants dans le modèle de panne adressée. Le journal actuel exige deux en-têtes concordants avant sa reprise. P8 conserve le refus de réparation de cette classe. |
| Attestations de g et g−1 | HMAC lié au pool et à la position exacte. Chaque copie a son propre LBA. Sans clé, seules la forme et la concordance publique sont accessibles. | Réplique du même enregistrement dans l’autre bloc du même couple, authentifiée à sa position. Le journal réencode le HMAC à la position de destination et stabilise la copie. |
| Banque de neuf blocs | SHA-256 lié à l’attestation. La faute est localisable à la plage de la banque, parfois au champ lors du décodage. | Les quatre banques sont des emplacements de rotation. La banque du prédécesseur protège un autre état. |
| Racine, feuille de traduction, métadonnées FS, données ou pack | Empreinte attendue depuis le parent. Avec clé, tag AEAD avant inspection LZ4. Les vues partageant une racine peuvent être affectées ensemble. | Une paire partagée possède une seule instance physique. Les références et les empreintes permettent de contrôler les octets, pas de les reconstituer. |
| Ancre ou catalogue P0/P5/P6 | Contrôles historiques de structure, génération, SHA-256, filiation et propriétaires. | L’ancien état ou les copies d’un marqueur dans le même bloc ne justifient pas une écriture de réparation. |
| Export autonome | Vérification du manifeste et des données selon son contrat. | Une restauration explicite crée un nouvel objet. Un export ne prouve pas la fraîcheur du volume courant. |

Les LBA indépendants se trouvent sur un seul support. La résistance aux pannes communes du périphérique et la fraîcheur après rejeu intégral exigent des preuves extérieures à l’image. [Format et limites de fraîcheur](novafs-p7-chiffrement.md)

## Parcours et preuve d’intégrité

Le moteur intervient sur un `Device` de blocs de 4 Kio. Il reconnaît le LV autonome P0, le catalogue de LV P0 et les pools partagés P5, P6 et P7. Il vérifie les blocs atteignables depuis les générations protégées. Les blocs libres peuvent contenir des octets de transactions anciennes et n’ont aucune valeur attendue.

| Support | Parcours |
|---|---|
| LV P0 / catalogue P0 | Le contrôle P2 compare les ancres, vérifie les arbres, les extents et les données. Un LV brut ou en préparation rend le contrôle du catalogue incomplet. Pour un LV inclus, le rapport utilise son identité et évite de présenter une adresse relative comme un LBA physique du pool. |
| P5 / P6 | Le moteur vérifie les entrées des catalogues de tête et parent individuellement, puis l’union des racines, l’admission des réservations et la classification des pages. Une même page physique endommagée peut apparaître sous plusieurs identifiants de vue. Un sélecteur déchiré mais lisible donne un résultat incomplet. |
| P7 sans toutes les clés | Contrôle public des en-têtes, des attestations, des banques et des références de racines. Une vue claire est parcourue jusqu’aux données. Le rapport énumère les observations de vues chiffrées qui restent opaques. Le HMAC de publication exige la clé administrative. |
| P7 avec toutes les clés | Vérification du journal, des signatures de vues, de la filiation des catalogues et des références inter-vues. Chaque unité est liée à son pool, domaine, rôle, position et génération. Le moteur existant ouvre l’AEAD avant la décompression. Le montage en lecture seule de chaque FS contrôle les deux générations internes, les extents et leurs données. |

Le parcours suit l’ordre persistant des catalogues et des vues. Un défaut de page localisé distingue la plage physique, la génération et les objets touchés. Une rupture de filiation ou une banque invalide peut empêcher d’atteindre ses objets. Le rapport donne alors la structure dont la vérification a échoué. Les empreintes sans autorité externe des formats historiques détectent les dommages accidentels. L’origine de l’image et la fraîcheur absolue de la dernière génération restent des hypothèses distinctes de l’intégrité du graphe observable.

## Exclusivité, progression et reprise

Avec un backend qui fournit `mutation_lock`, le diagnostic et la réparation acquièrent le même verrou que l’écrivain P7 avant la première lecture. Un concurrent reçoit `Busy`. En l’absence de verrou, l’appelant du diagnostic doit fournir un fichier hors ligne immuable pendant toute l’opération. La bibliothèque refuse une réparation sur ce descripteur. La CLI utilise un fichier ordinaire en entrée, contrôle sa taille et ses dates de modification et de changement avant et après lecture. L’exclusion des écrivains extérieurs reste à la charge du détenteur de l’image. Pour P5 et P6, le détenteur garantit aussi la mise hors ligne des anciens écrivains.

Le rappel de progression reçoit un identifiant aléatoire d’exécution, le nombre de lectures déjà effectuées et le prochain LBA. Il intervient avant la première lecture, puis toutes les 64 lectures. Il peut attendre pour limiter le débit, reprendre sous la même garde, ou renvoyer `false` pour abandonner. Une pause sous garde prolonge l’indisponibilité en écriture. Après abandon, redémarrage ou changement d’image, le contrôle repart de la première lecture. Il n’existe aucun curseur persistant pouvant servir de preuve. Une interruption ou une E/S incertaine après une tentative de réparation impose un nouveau contrôle.

L’index de présence des LBA occupe un bit par bloc adressable, plafonné par la géométrie historique de 2²¹ blocs. Les index des graphes croissent avec les objets atteignables sous les limites du format. Les vues sont contrôlées une à une pour localiser les défauts avant de vérifier leurs relations communes. Ce parcours augmente le nombre de relectures des racines partagées. La mesure ci-dessous porte sur un corpus défini, sans extrapolation à un pool de capacité maximale.

## Autorisation d’une réparation

1. Acquérir la garde de mutation, lire les deux en-têtes, authentifier la clé du journal et examiner les huit LBA d’attestation. Une banque visiblement plus récente sans preuve de publication produit une issue indéterminée. Une copie valide contradictoire, une perte de filiation ou une anomalie de rotation hors des deux générations protégées interdit la réparation.
2. Identifier la réplique de même génération, même digest et même parent. Valider les banques, les données, les tags, les relations inter-vues et les réservations sous cette garde. Plusieurs anomalies peuvent être rapportées sur le même objet partagé.
3. Demander la reprise au journal P7.1. Il draine les commandes en attente. Sur la vue relue après ce drainage, le scrub vérifie à nouveau la tête, le parent, la liste exacte des destinations et les graphes. Un changement constaté interrompt l’opération avant la première écriture de réparation.
4. Le journal recalcule chaque attestation pour son **LBA de destination** et confirme ses écritures par une deuxième synchronisation. Le contrôle ne publie aucune nouvelle génération. Une interruption peut laisser une partie des copies restaurée. Chaque copie source est conservée et chaque destination peut être requalifiée lors du prochain contrôle.
5. Relire toutes les attestations, rouvrir le journal à froid et contrôler de nouveau les graphes. Le statut `repare` exige que le même état authentifié soit lisible. Dans la CLI, l’entrée reste ouverte en lecture seule. La copie privée est émise sur stdout uniquement après cette vérification. En cas d’erreur de transport, écarter toute sortie partielle. La persistance du fichier de destination et la conservation de l’original relèvent de l’appelant.

Les publications ordinaires conservent le protocole F1/F2 P7. Le scrub réutilise directement sa primitive de reprise, sous le verrou du pool. Les essais de coupure portent sur le modèle de persistance du simulateur. Une réponse à la demande de synchronisation du fichier hôte ne démontre pas la résistance à une coupure électrique physique.

## Interface et rapport

```sh
novafs scrub BLOCS SECTEUR --json < image.raw
novafs scrub BLOCS SECTEUR --full --cles-fd3 --json < image.raw
novafs scrub BLOCS SECTEUR --repair --cles-fd3 --json < image.raw > candidate.raw 2> rapport.json
```

L’option `--cles-fd3` fixe P7 comme format attendu. Pour les images P0, P5 et P6, lancer le contrôle sans cette option. Le lanceur de clés doit transmettre le pipe anonyme `NP7KEYS1` sur FD 3. Le banc de reproduction ci-dessous fournit un exemple complet sur des clés synthétiques. Fournir **un nouveau fichier vide** pour la sortie de réparation. Le shell peut tronquer une sortie existante dès la redirection, avant l’exécution de NovaFS. Les secteurs acceptés sont 512 et 4 096 octets.

| Code | Statut | Interprétation |
|---:|---|---|
| 0 | coherent | Toutes les vérifications applicables au format demandé ont abouti. Sur un pool sain, une demande de réparation produit une sortie image vide. |
| 1 | corruption | Le diagnostic a trouvé au moins un défaut. |
| 2 | usage | Arguments, trame ou descripteurs non conformes. |
| 3 | incomplet | Contrôle partiel, interruption, concurrence, publication ambiguë ou support modifié. |
| 4 | repare | Réplique reconstruite et requalifiée. C’est le seul code autorisant à conserver l’image produite. |
| 5 | non_reparable | Une réparation a été demandée et un défaut ne dispose d’aucune procédure sûre. |
| 6 | erreur_io | Erreur de lecture, d’écriture, de synchronisation ou de transport. |
| 7 | authentification_indisponible | L’autorité fournie ne permet pas le contrôle complet demandé. |

Le JSON `novafs-scrub-1` indique l’exécution, le format, la génération, l’authentification, les observations de vues, les lectures tentées, les LBA distincts, les écritures et les synchronisations du moteur. Les compteurs excluent la copie d’entrée et le transport de sortie de la CLI. `constats_corruption` compte les observations qualifiées de dommages. Un pack atteint par deux vues peut produire deux observations du même LBA. Le rapport distingue les réparations possibles, effectuées et les défauts sans source exploitable. Le diagnostic écrit sur stdout, la réparation sur stderr. Une erreur survenue avant le parcours produit un rapport d’erreur court. Les clés et les contenus de fichiers sont exclus des rapports.

## Validation des corruptions et des interruptions

La fixture P8 conserve hors du dispositif fautif les octets acquittés d’un fichier de 16 Kio, la génération attendue et une image complète saine. Elle combine un volume clair, une origine chiffrée LZ4, son snapshot et son clone. L’origine chiffrée est supprimée avant les injections.

| Scénario | Résultat observé sur les deux architectures et géométries |
|---|---|
| Contrôle avec et sans les clés | Le contrôle complet authentifie le graphe. Le diagnostic sans clé mentionne les vues opaques. L’état stable et le journal d’écritures restent identiques avant et après le diagnostic. |
| Interruption, reprise, concurrence | Le rappel abandonne puis un nouveau contrôle reçoit un autre identifiant. Un second thread trouve la garde occupée pendant une pause et le contrôle en cours reprend après son signal. Un dommage ajouté entre un diagnostic favorable à la réparation et le prochain appel empêche toute écriture. |
| Double dommage | En-tête, banque, racine de traduction partagée, enveloppe du pack, tag ou contenu modifié. Une attestation abîmée en plus du défaut de contenu ne conduit à aucune écriture. Les observations du snapshot et du clone désignent la même paire physique du pack. Les deux attestations détruites empêchent la reconstruction. |
| Réparation vérifiée | Une réplique valide restaure son homologue. Un second scénario reconstruit une copie de tête et une copie du parent. L’image stable est identique à l’oracle complet. Un nouveau montage retrouve la même génération, le même snapshot et les octets attendus du clone. Un second scrub sur image saine effectue zéro écriture et zéro synchronisation. |
| État modifié au drainage | Un backend de test présente un contenu de pack différent après le premier flush. La seconde validation situe le défaut dans les vues affectées et refuse avant toute écriture de réplique. |
| Coupures P8 | Pour chaque secteur, neuf frontières de la reprise d’une réplique, chacune soumise à trois persistances possibles. Le remontage vérifie génération, graphe et données dans chacun des 27 cas. |
| E/S P8 | Pour chaque secteur, erreur de lecture du pack, écriture déchirée de la réplique, échec du drainage puis échec de la confirmation. Le statut signale une E/S. Chaque remontage après perte du cache restitue le témoin. |
| Campagne déterministe | 64 tirages par secteur à partir de `0x95a128fd`. Treize répliques réparables, dix-sept dommages combinés, treize interruptions et vingt-et-une erreurs de lecture. Les répliques, positions altérées et frontières varient selon la graine. |
| CLI et compatibilité | 12 rapports JSON par architecture et secteur. Le producteur est compilé au commit P7.2 sur l’autre architecture. Les exécutables P7.2 et P8 remontent la copie réparée. Le lecteur compare les données du snapshot et du clone. La CLI contrôle également le LV canonique P0, un catalogue P0 de 16 LV, puis un fichier partagé en P5 et P6. Dans les deux formats partagés anciens, elle identifie les deux vues du bloc défectueux et refuse la réparation. |

Un tag modifié sur l’image peut être rejeté dès l’empreinte du parent. Les tests unitaires P7 recalculent également l’empreinte extérieure après altération de l’enveloppe afin de vérifier le rejet cryptographique interne. Les tests P1 sur fichier et disque virtuel QEMU exercent la récupération historique. Les nouvelles coupures P8 sont injectées au niveau du backend simulé.

## Qualification et mesures

Sur Apple M4 Max (Mac16,5, 128 Go), macOS 27.2 et Rust stable 1.99.1, chaque architecture a exécuté **178 tests standards et 179 avec instrumentation, zéro échec**, ainsi qu’un million d’opérations différentielles. Les quatre entrées marquées `ignored` dans la suite générale ont leurs campagnes dédiées. Les deux géométries sont exercées au sein des scénarios. Les effectifs de tests Rust sont des totaux par ISA.

| Contrôle | ARM64 natif | x86_64 sous Rosetta |
|---|---:|---:|
| Coupures P7 / P7.1 et erreurs d’écriture ou de flush P7 | 582 / 400 / 70 | 582 / 400 / 70 |
| Coupures / erreurs d’E/S P8 avec remontage | 54 / 8 | 54 / 8 |
| Tirages P8 déterministes | 128 | 128 |
| Rapports JSON validés via la CLI P8 | 24 | 24 |
| Observations de l’audit P7.1, deux répétitions | 20 | 20 |
| Scénarios historiques fichier / disque virtuel QEMU, deux remontages par cas | 24 / 10 | 24 / 10 |
| Vecteurs liblz4 dans chaque sens | 256 | 256 |

Les fixtures historiques de huit révisions, les échanges de pools P6 et les exports P7 ont été rejoués. Le sous-ensemble cryptographique a compilé sur les deux cibles bare-metal. Le moteur de scrub a été exécuté sur Darwin. [Suites, oracles, échanges et rapports JSON](validation/v0.9.12/qualification.json.gz)

Le banc chronomètre 100 opérations par ligne, soit 1 600 échantillons. Chaque secteur utilise une image de 16 Mio identique pour les deux exécutables, avec un fichier de 16 Kio, un snapshot et un clone chiffrés LZ4 après suppression de l’origine. Le banc recrée le dommage d’un octet avant chaque réparation, hors chronométrage. Les ouvertures utilisent une clé de récupération. La copie privée et la préparation de la fixture sont hors mesure. Le cache de fichiers macOS reste actif. Les percentiles sont ceux du rang supérieur. Les horloges CPU du thread sont étalonnées sur les deux exécutables. Le contrôle de référence appelle le même chemin que le vérificateur P7.2, compilé avec le paquet courant, sur les mêmes octets.

| ISA | Secteur | Opération | p50 ms | p95 ms | p99 ms | Max ms | CPU moyen ms | Lectures / LBA distincts | Mio/s lus / distincts | Tas pic Kio | Alloc. | RSS pic Mio |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ARM64 | 512 | Contrôle P7.2 | 7,86 | 8,063 | 8,226 | 8,481 | 7,872 | 760 / — | 377,1 / — | 81,3 | 659 | 3,14 |
| ARM64 | 512 | Diagnostic sans clé | 0,465 | 0,495 | 0,511 | 0,524 | 0,469 | 41 / 34 | 341,63 / 283,3 | 74,8 | 40 | 2,77 |
| ARM64 | 512 | Scrub complet | 24,939 | 25,319 | 25,368 | 25,398 | 24,916 | 2334 / 72 | 365,89 / 11,29 | 81,8 | 2030 | 3,23 |
| ARM64 | 512 | Réparation | 78,335 | 83,743 | 85,053 | 86,515 | 74,307 | 7006 / 72 | 347,09 / 3,57 | 214,2 | 6077 | 3,36 |
| Rosetta | 512 | Contrôle P7.2 | 12,651 | 12,8 | 12,861 | 12,867 | 12,643 | 760 / — | 234,81 / — | 80,9 | 659 | 4,66 |
| Rosetta | 512 | Diagnostic sans clé | 0,721 | 0,75 | 0,774 | 0,774 | 0,723 | 41 / 34 | 221,75 / 183,89 | 74,4 | 40 | 4,25 |
| Rosetta | 512 | Scrub complet | 39,335 | 40,109 | 40,813 | 41,268 | 39,384 | 2334 / 72 | 231,47 / 7,14 | 81,4 | 2030 | 5,46 |
| Rosetta | 512 | Réparation | 122,305 | 126,471 | 127,877 | 129,012 | 117,73 | 7006 / 72 | 222,83 / 2,29 | 213,3 | 6077 | 6,55 |
| ARM64 | 4096 | Contrôle P7.2 | 7,801 | 7,898 | 8,057 | 8,072 | 7,79 | 760 / — | 381,07 / — | 81,3 | 659 | 3,16 |
| ARM64 | 4096 | Diagnostic sans clé | 0,457 | 0,494 | 0,507 | 0,51 | 0,46 | 41 / 34 | 348,2 / 288,75 | 74,8 | 40 | 2,8 |
| ARM64 | 4096 | Scrub complet | 24,834 | 25,361 | 25,626 | 25,8 | 24,841 | 2334 / 72 | 366,94 / 11,32 | 81,8 | 2030 | 3,25 |
| ARM64 | 4096 | Réparation | 77,91 | 84,257 | 85,368 | 85,7 | 74,131 | 7006 / 72 | 348,26 / 3,58 | 214,2 | 6077 | 3,38 |
| Rosetta | 4096 | Contrôle P7.2 | 12,597 | 12,725 | 12,85 | 12,868 | 12,595 | 760 / — | 235,72 / — | 80,9 | 659 | 5,06 |
| Rosetta | 4096 | Diagnostic sans clé | 0,719 | 0,744 | 0,754 | 0,776 | 0,723 | 41 / 34 | 221,79 / 183,93 | 74,4 | 40 | 4,04 |
| Rosetta | 4096 | Scrub complet | 39,268 | 39,95 | 40,398 | 40,494 | 39,306 | 2334 / 72 | 231,93 / 7,15 | 81,4 | 2030 | 4,88 |
| Rosetta | 4096 | Réparation | 122,416 | 126,919 | 127,527 | 128,346 | 117,736 | 7006 / 72 | 222,28 / 2,28 | 213,3 | 6077 | 6,69 |

Les lectures sont les appels de 4 Kio au backend. Le débit « distincts » déduplique les LBA dans une opération. Ces mesures n’observent pas le trafic interne du SSD. Sur ce corpus, le contrôle complet effectue 2 334 lectures contre 760 pour le contrôle de référence. Le parcours individuel sert à localiser les anomalies dans toutes les vues, y compris les vues partageant une même racine. Le contrôle global conserve les vérifications entre vues. Les écritures normales restent exclues pendant la garde. Il n’y a donc aucune distribution de latence d’écritures simultanées admises à publier pendant le scrub.

La réparation d’une attestation écrit **un bloc de 4 Kio et effectue deux synchronisations**. L’amplification au niveau du moteur vaut 1× par attestation et 4 096× par rapport à l’octet altéré dans cette expérience. La CLI copie en plus les 16 Mio d’entrée et transporte 16 Mio de sortie pour cet exemple. Le banc comptabilise le tas Rust via l’allocateur Darwin, une pile réservée de 2 Mio et le RSS maximal de chaque processus enfant via `wait4`. Le tas vivant retrouve sa valeur initiale à chaque itération et après restitution du dispositif. Les essais ne mesurent pas la mémoire du cache système, d’éventuelles fuites du noyau hôte ni le coût sur une image maximale. [Distributions et compteurs](validation/v0.9.12/mesures.json.gz) · [1 600 échantillons bruts](validation/v0.9.12/mesures.jsonl.gz)

## Reproduction

Depuis la racine du monorepo sur un Mac Apple Silicon avec Rosetta, Rust stable, les deux cibles Darwin, QEMU, liblz4 et les dépendances historiques disponibles localement. Le banc crée lui-même ses images et ses pipes. Les commandes suivantes utilisent un répertoire de résultats sous `target`.

```sh
set -eu
out=novaFS/target/qualification-p8
mkdir -p "$out"
for isa in aarch64 x86_64; do
    python3 novaFS/lab/p6-validation.py "$isa" --sortie "$out/suites-$isa.json"
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run test --offline \
        --test p7_secure --test p8_scrub -- --nocapture --test-threads=1
    python3 novaFS/lab/p7-cli.py "$isa"
done
python3 novaFS/lab/p6-historiques.py --support formats > "$out/formats.json"
python3 novaFS/lab/p6-historiques.py --support pannes > "$out/pannes.json"
python3 novaFS/lab/p6-interop.py > "$out/interop-p6.json"
python3 novaFS/lab/p6-lz4-interop.py > "$out/lz4.json"
python3 novaFS/lab/p7-validation.py interop --sortie "$out/interop-p7"
python3 novaFS/lab/p7-no-std.py
awk '/^```sh$/{p=1;next} p && /^```$/{exit} p' \
    novaFS/docs/validation/p7.1-audit.md > "$out/audit.sh"
for rep in 1 2; do sh "$out/audit.sh" > "$out/observations-$rep.txt"; done
python3 novaFS/lab/p8-validation.py cli --sortie "$out/cli-p8.json"
python3 novaFS/lab/p8-validation.py mesures --repetitions 100 --sortie "$out/mesures.json"
```
