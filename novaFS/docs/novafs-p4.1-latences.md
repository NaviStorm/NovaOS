# NovaFS P4.1 — latence des lectures de 64 Kio

## Ce que mesurait P4

Le débit de 153,638 Mio/s et les 11,739 ms ne résument pas la même population de la même manière. Le premier divise 160 Mio utiles par la durée des **cinq** boucles de lecture. Le second est le **maximum des cinq p99 locaux**. Chaque boucle chronométrait 512 appels synchrones à `Engine::read("/volume", offset, 65536)`, avec validation de chacun des octets rendus et libération du vecteur. Le chrono est mural. La liste des temps individuels disparaissait après calcul des rangs. C’est une durée de requête de 64 Kio, pas une durée de lecture de bloc de 4 Kio. Cette charge s’exécute après les écritures de la série ; aucun ticket, aucune transaction et aucun flush ne figurent dans l’intervalle de lecture.

| Archive ARM64 | p99 des séries 1 / 2 / 3 / 4 / 5 (ms) | Bornes du p99 sur 2 560 appels (ms) | Débit global (Mio/s) |
| --- | --- | --- | ---: |
| P3 | 1,217 / 1,272 / 1,207 / 1,224 / 1,572 | 1,207–1,572 | 101,477 |
| P4 | 0,298 / 0,703 / **11,739** / 0,475 / 0,510 | **1,746–2,368** | 153,638 |

Dans la série 3 P4, le p50 était de 0,335 ms, le p95 de 1,746 ms et le maximum de 27,024 ms. Le p99 local est le 507ᵉ appel sur 512 classés par durée croissante. Le p99 global est le 26ᵉ **plus lent** sur 2 560. Au moins 26 appels de la série 3 atteignaient 1,746 ms ; au plus 25 dépassaient ce seuil dans cette série. Le maximum de toutes les autres séries était de 2,368 ms. Cela borne le p99 global sans inventer les échantillons manquants. Sur Rosetta les bornes globales P3 [1,244 ; 1,839] et P4 [1,233 ; 1,899] ms se recouvrent. [Recalcul sur les quatre archives](../lab/historical-p99.py) · [Valeurs exactes et effectifs](validation/v0.9.5/historique.json)

**Diagnostic avant optimisation :** une queue plus lente a réellement existé dans cette campagne ARM64. Assimiler le pire p99 local au p99 de l’ensemble amplifie l’écart. Les archives ne contiennent ni l’ordre des échantillons, ni le temps CPU, ni les appels au fichier par requête. Elles ne permettent pas d’attribuer cet épisode précis à CLOCK, à APFS, à l’ordonnanceur de macOS ou à une autre charge. P4 déclarait que le Mac servait aussi à des compilations et à des tests. La coïncidence ne prouve pas la cause.

## Expérience comparative

MacBook Pro `Mac16,5`, M4 Max, 16 CPU logiques, 128 Gio, alimentation secteur ; macOS 27.2 (26B123), Xcode 27.2, Rust 1.99.1 stable en release. Les Mach-O ARM64 sont natifs et les Mach-O x86_64 tournent sous Rosetta sur le même Mac. La valeur de `sysctl.proc_translated` a été vérifiée (0 et 1). Les cibles emploient respectivement des secteurs logiques de 4 096 et 512 octets, comme les archives. Une comparaison entre les deux architectures mêlerait aussi l’effet de cette géométrie. Toutes les comparaisons de versions sont faites **à architecture, géométrie, contenu et charge constants**.

Le [banc de lecture](../lab/latency.rs) rejoue les écritures séquentielles et aléatoires de P4, et fige le fichier avant chacune des cinq lectures historiques. Les trois moteurs reçoivent les mêmes images de 96 Mio, numérotées et identifiées par SHA-256. Cela conserve la fragmentation croissante et exclut des chronos les écritures et la préparation des images. Le montage RO contrôle les structures ; la passe suivante démarre après restitution du cache NovaFS. Elle diffère donc sur ce point de la lecture historique dans le montage écrivain vivant. Le banc mixte a également été exécuté dans sa configuration d’origine, après écritures.

- **Froid NovaFS** : 512 lectures contiguës de 64 Kio couvrent 32 Mio. Le cache du moteur est vide au départ ; les métadonnées réutilisées peuvent ensuite donner des hits.
- **Chaud NovaFS** : huit plages couvrant 512 Kio sont préchargées, puis relues 64 fois. Le banc exige zéro défaut, zéro éviction et zéro lecture de fichier au cours de ces 512 requêtes.
- **Cache hôte ordinaire** : lecture explicite de l’image avant la mesure. macOS reste libre d’évincer des pages. **F_NOCACHE** porte sur un fichier temporaire indépendant, pas sur un purge-cache système. Ce contrôle ne vide ni le cache du contrôleur, ni celui du SSD.

Les trois bibliothèques sont compilées avec le même lecteur de laboratoire. Les sondes fines ne sont pas activées durant la comparaison. Chaque appel fournit son temps mural, son CPU de thread, la durée de la seule lecture moteur, celle du contrôle et de la libération, et le temps cumulé des appels au fichier. Ce dernier **inclut** du CPU : il ne s’ajoute pas au temps CPU. Le résidu mur moins CPU n’est pas spécifiquement du temps disque. Le débit utilise la durée de la boucle complète avec l’instrumentation commune ; il ne rémunère ni le montage, ni le préchauffage, ni l’écriture des résultats. La collecte n’intercepte pas les autres activités de macOS et ne fixe pas les fréquences CPU.

Les sondes distinctes couvrent la recherche dans le cache, le traitement des défauts, l’éviction, le parcours d’extents, l’E/S et l’allocateur ; leurs mesures sont inclusives. [Détail des sondes et calibration](novafs-p4.1-sondes.md). Un autre banc fait varier le nombre de clients et horodate l’attente FIFO, la préparation, la publication, F1 et F2. [Méthode, rangs réels et résultats des transactions](novafs-p4.1-transactions.md).

Le mode `F_NOCACHE` identifie les deux images octet par octet **après** la synchronisation de la copie. Le montage strict relit la source immuable avec le cache hôte ordinaire ; les requêtes mesurées relisent la copie par le descripteur sur lequel `F_NOCACHE` était actif dès sa création. Le moteur ne reçoit aucune modification des données entre les deux lectures. Cette disposition économise le temps de montage hors mesure et évite de peupler le cache hôte du fichier étudié.

La première calibration de CPU traduite a échoué : environ 2,88 ms étaient annoncées pour 120 ms de calcul constatées par une autre interface. Le banc utilise maintenant les temps utilisateur et noyau de Mach sur x86_64, sans facteur correcteur ajusté au Mac ; sur ARM64 il utilise l’horloge Darwin validée. Chaque nouveau processus doit passer un contrôle calcul/repos. Aucun résultat basé sur l’ancienne mesure CPU traduite n’entre dans la comparaison. Les lectures de l’horloge CPU encadrent le chronomètre mural. À la résolution de Mach (µs), le CPU peut donc légèrement dépasser le mur. Le CSV conserve les deux valeurs originales ; la synthèse de la queue calcule leur différence signée sur chaque appel, y compris tous les ex æquo au seuil.

## Résultats des mesures

### Résultat des campagnes de lecture appariées

Entre 20:54:49 et 21:04:34 UTC le 10 novembre 2026, six campagnes ont exécuté 72 processus de mesure successifs, sans compilation ni autre banc NovaFS pendant leur chronométrage. L’ordre des trois versions est équilibré (deux passages à chaque position pour chaque architecture et mode hôte). L’ordre des architectures et celui des modes hôtes varient aussi. Le moteur P4.1 sans les sondes a la même politique et les mêmes chemins de lecture que P4 : la paire P4/P4.1 sert de témoin de variabilité. Les 368 640 lectures ont fourni 15 360 observations pour chacune des 24 combinaisons ; chaque série contenait 512 appels ordonnés. Les 184 320 appels à chaud ont donné zéro lecture fichier, zéro défaut et zéro éviction. L’empreinte des cinq images était inchangée à la fin.

| Lecture séquentielle, cache NovaFS vide au départ | ARM64, p99 / débit | Rosetta, p99 / débit |
| --- | ---: | ---: |
| P3, cache hôte préchauffé | 0,736 ms / 99,33 Mio/s | 1,150 ms / 62,87 Mio/s |
| P4, même charge | 0,488 ms / 197,47 Mio/s | 0,750 ms / 128,72 Mio/s |
| P4.1, même charge | **0,478 ms / 198,80 Mio/s** | **0,740 ms / 128,84 Mio/s** |

Sur les six campagnes préchauffées, le p99 froid de **P4 est inférieur à P3 six fois sur six sur chaque architecture**. Le ratio médian des p99 appariés P4/P3 vaut 0,681 en natif et 0,671 sous Rosetta. Pour P4.1/P4 il vaut 0,971 et 0,995. Le maximum de chacune des 15 360 lectures du groupe préchauffé reste sous 1,10 ms en ARM64 pour P4 et P4.1. Les 11,739 ms historiques ne sont donc pas une pénalité reproductible de P4 sur cette charge. L’épisode initial et ses conditions exactes restent distincts de cette constatation.

**Limite sous F_NOCACHE.** À froid, le p99 ARM64 de P3/P4/P4.1 vaut 1,831/1,494/1,664 ms, celui sous Rosetta 2,170/1,768/**2,221 ms**. Les débits P4 → P4.1 passent de 81,56 à 80,24 Mio/s et de 66,05 à 63,20 Mio/s. P4.1 enregistre un appel d’au moins 11,738875 ms sur ARM64 (max 12,584 ms) et 18 sous Rosetta (max **40,601 ms**). Le maximum n’est pas le p99. Les p99 des six campagnes P4.1 traduites vont de 1,719 à 4,862 ms ; le ratio médian apparié P4.1/P4 est de 1,014, avec trois campagnes sur six plus rapides. Le p99 groupé est moins favorable que ce ratio médian : les campagnes lentes contribuent davantage à la queue groupée.

Pour les 154 appels les plus lents de P4.1 en mode froid `F_NOCACHE` sous Rosetta, la médiane est de 3,092 ms muraux ; sur ces mêmes appels le fichier prend 2,589 ms en médiane, le thread 0,738 ms CPU et le contrôle/libération du tampon 0,018 ms. Le reliquat signé mur−CPU est de 2,367 ms en médiane. Ces médianes ne sont pas additives. En mode hôte préchauffé, le fichier ne prend que 0,018 ms en médiane dans la queue Rosetta. L’essai localise donc des pointes dans les appels au fichier ; il ne distingue pas APFS, la file d’E/S, le réveil du thread ni le périphérique matériel.

Le même échantillon Rosetta qui a duré **40,601 ms** a passé **40,073 ms dans les lectures du fichier**, pour 0,757 ms CPU sur le thread et 0,020 ms de contrôle/libération. Pour le maximum ARM64, les valeurs correspondantes sont 12,584 ms, 12,116 ms, 0,648 ms et 0,018 ms. Il s’agit des composantes des appels individuels concernés ; elles ne sont pas déduites par soustraction de quantiles.

**Témoin sans compteurs.** Le harnais commun mesure les blocs, le CPU et les allocations ; son coût peut favoriser P4, qui alloue moins. Trois autres campagnes appariées ont lu les mêmes images avec l’allocateur `System` ordinaire et seulement un chrono mural par requête. Les 92 160 lectures ont contrôlé leurs octets. Sur la charge froide préchauffée par macOS, les p99 P3/P4/P4.1 sont de 0,678/0,449/0,458 ms en ARM64 et de 1,107/0,726/0,710 ms sous Rosetta. Les débits sont respectivement de 105,48/209,93/209,00 et de 65,06/134,24/134,43 Mio/s. P4 bat P3 au p99 dans les douze comparaisons appariées (deux architectures × deux températures × trois campagnes). Ce témoin indépendant du comptage confirme le sens du résultat, sans revendiquer un gain moteur propre à P4.1. [Distributions du témoin](validation/v0.9.5/temoin.json) · [Échantillons du témoin](validation/v0.9.5/temoin.jsonl.gz).

[p50/p95/p99/max et débits des 24 groupes](novafs-p4.1-distributions.md) · [Campagnes, séries, calibration et empreintes](validation/v0.9.5/lectures.json) · [CSV brut compressé](validation/v0.9.5/lectures.csv.gz) · [Synthèse appariée et composition de la queue](validation/v0.9.5/synthese.json).

### Mémoire

| P4.1, pic observé | ARM64 | Rosetta |
| --- | ---: | ---: |
| Tas du processus du lecteur instrumenté (arrondi malloc, banc inclus) | 1 194 096 o | 1 210 624 o |
| Pile principale / pile du worker du lecteur (mesure pthread du témoin) | 2 097 152 / 2 109 440 o | 2 097 152 / 2 097 152 o |
| Tas précédent + réserves de piles de la même configuration | **5 400 688 o** | **5 404 928 o** |
| RSS maximale du lecteur instrumenté (`time -l`) | 4 554 752 o | 12 824 576 o |
| RSS du banc mixte complet | 6 078 464 o | 15 319 040 o |

Le budget du moteur est de 7 Mio, soit 7 340 032 octets. Le total tas/piles du lecteur reste en dessous. Le champ `pile_travail` du lecteur instrumenté enregistre la taille **demandée**. Le tableau utilise la taille **effective** observée sur les 18 exécutions du témoin avec la même demande ; sur ARM64 elle est supérieure de 12 288 octets. La RSS inclut le chargeur, les bibliothèques et les coûts de traduction : le processus complet sous Rosetta dépasse 7 Mio. Les tableaux d’analyse Python et les images du laboratoire ne sont pas des allocations du moteur.

### Décision

P4.1 corrige le **banc** : export des durées avant tri, calcul d’un p99 groupé et d’un pire p99 explicitement distinct, calibration CPU et contrôles appariés. Le test du rapport injecte six longues durées dans une seule série et exige qu’elles figurent dans son p99 local et dans le maximum global, sans fausser le p99 des 2 560 appels. Aucune observation ne justifie de changer l’allocateur, le cache, le coordinateur ou le format pour corriger le chiffre de 11,739 ms.

Le p99 `F_NOCACHE` et ses maxima restent un risque pour une charge sensible aux délais d’E/S. Pour aller plus loin, le prochain diagnostic utile serait un rejeu de la **même trace de blocs** directement sur le fichier temporaire, puis une répétition sur un second Mac ou support dédié avec suivi des temps d’E/S et de réveil. Cela permettrait de décider si un essai de groupement de lectures contiguës mérite son coût mémoire, tout en conservant la validation des checksums. Aucune de ces options ne fait partie de la livraison.

### Charge historique dans un montage écrivain vivant

Le banc historique corrigé a rejoué les cinq cycles d’écritures, lectures et modifications aléatoires sur l’image vivante avant les scénarios de petits fichiers, contention, pression et remontage. Les régressions fonctionnelles et d’autres mesures s’exécutaient aussi sur le Mac pendant une partie de ces deux passages. Ces chiffres contrôlent le chemin d’origine ; ce ne sont pas les campagnes comparatives isolées.

| P4.1, lecture après écritures | n | p50 | p95 | p99 | max | pire p99 de série | débit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ARM64 | 2 560 | 0,322 ms | 0,442 ms | 0,514 ms | 2,214 ms | 0,613 ms | 192,07 Mio/s |
| Rosetta | 2 560 | 0,479 ms | 0,675 ms | 0,803 ms | 1,749 ms | 0,882 ms | 128,48 Mio/s |

Le même rejeu a mesuré l’écriture séquentielle à 3,595 et 6,144 Mio/s (ARM64 et Rosetta), en dessous des archives P4 à 5,918 et 6,773 Mio/s. La charge concurrente rend cette comparaison historique insuffisante pour isoler une cause. Les écritures ont conservé exactement un F1 et un F2 par transaction. Aucune synchronisation, aucune politique CLOCK, aucune réservation et aucun chemin de publication n’ont été changés. [Journal mixte ARM64](validation/v0.9.5/mixte-aarch64.json) · [Journal mixte Rosetta](validation/v0.9.5/mixte-x86_64.json)

Un contrôle distinct sur les cinq cycles mixtes a comparé P4 et P4.1 sans autre banc NovaFS en cours. La baisse d’écriture ARM64 n’y est pas reproduite : 5,852 → 6,122 Mio/s (p99 : 13,861 → 14,164 ms). Sous Rosetta, le premier passage donne 6,733 → 5,424 Mio/s, le passage dans l’ordre inverse 5,611 → 6,130 Mio/s. Les deux paires réunies, à raison de 5 120 écritures de 64 Kio par version, donnent **6,121 → 5,755 Mio/s et un p99 de 14,740 → 19,876 ms**. Le sens de variation du débit s’inverse, mais le p99 d’écriture est plus élevé dans **les deux** paires. Les lectures, allocations, écritures et deux barrières par transaction ont les mêmes comptes. Aucune équivalence des queues d’écriture sous Rosetta n’est démontrée. Un suivi utile serait de corréler **chaque** écriture lente de 64 Kio à ses propres F1 et F2, sur plus de fenêtres temporelles ; les totaux par série ne permettent pas cette attribution. [Premier passage](validation/v0.9.5/rejeu-mixte-apparie.json) · [Contre-épreuve Rosetta](validation/v0.9.5/rejeu-mixte-rosetta-inverse.json).

## Régressions fonctionnelles

| Contrôle | Résultat et portée |
| --- | --- |
| Suites P0 à P4, secteurs 512 et 4 096 | 95 tests ordinaires par architecture ; avec toutes les features du laboratoire, 96 par architecture. Cela inclut les scénarios P0, erreurs d’E/S, corruption de structures, pression, quotas, invalidation de cache, collisions, évictions, échecs de publication et restitution. |
| Modèle différentiel | Un million d’opérations par architecture, 345 183 commits observés, 54 643 pénuries injectées et 1 000 remontages/audits sur chacune. Durées : 26,074 s en ARM64 et 44,983 s sous Rosetta. |
| Coupures P1 | Pour chaque binaire hôte : 24 interruptions du processus écrivain et 10 arrêts brusques de la VM QEMU x86_64, deux remontages après chaque coupure. Les deux cibles Darwin lisent les images avec les résultats attendus (ancien, nouveau ou incertain selon la phase). Cela ne qualifie pas une coupure électrique du SSD. |
| Compatibilité | Échanges dans les deux sens avec les versions P0 à P4 et les deux géométries. SHA-256 canonique : `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. Catalogues de 16 LV comparés octet par octet entre P2/P3/P4 et P4.1 ; 16 contrôles d’intégrité et 8 de comptabilisation par version de référence. |
| Restitution et intégrité | Valeur du tas avant/après identique pour les 64 cycles de reprise dans chaque géométrie. Cycles d’annulation, F2 indéterminé, pression du cache et restitution des réservations également exécutés. Le vérificateur a contrôlé une image saine et sa variante corrompue de 8 Gio ainsi qu’un pool de 16 LV ; contenu inchangé après vérification. Ces résultats bornent l’absence de fuite aux scénarios exercés. |
| Ordre FIFO et durabilité | Trois campagnes par cible, deux géométries, un ou huit clients sur deux volumes. 3 072 tickets au total ; dans chaque cas rang observé à F2 égal au rang d’admission, une dépendance ou plus avant F1, puis une publication d’ancre et F2. Un client inactif en tête peut toujours retarder le suivant. |

[Détail des tests, coupures et restitutions](validation/v0.9.5/regressions.json) · [Mesures sur image et pool](validation/v0.9.5/controle-memoire.json). Les tests ne dépendent d’aucun composant modifié de NovaOS ; le noyau n’a pas été modifié.

## Reproduction

Depuis la racine de NovaOS, sur un Mac Apple Silicon avec Rosetta, les deux cibles Rust Darwin et les dépendances Cargo hors ligne :

```sh
set -eu
tmp=$(mktemp -d)
python3 novaFS/lab/historical-p99.py
python3 novaFS/lab/test-latency-report.py
python3 novaFS/lab/measure-latency.py preparer --travail "$tmp/banc"
python3 novaFS/lab/measure-latency.py mesurer --travail "$tmp/banc" --sortie "$tmp/resultats" --campagnes 6
python3 novaFS/lab/analyse-lectures.py "$tmp/resultats" > "$tmp/synthese.json"
python3 novaFS/lab/temoin.py --travail "$tmp/banc" --sortie "$tmp/resultats"
python3 novaFS/lab/rejeu-mixte.py --sortie "$tmp/resultats/rejeu-mixte.json"
python3 novaFS/lab/rejeu-mixte.py --arch x86_64 --inverse --horodater --sortie "$tmp/resultats/rejeu-inverse.json"
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline
    novaFS/lab/run test --offline --all-features
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    novaFS/lab/run test --offline --test cout_extents -- --ignored --nocapture
    python3 novaFS/lab/test-persistence.py --support tous
    for secteur in 512 4096; do
        novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" reprise
        novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" reservations
    done
    python3 novaFS/lab/measure-performance.py --arch "$arch" > "$tmp/complet-$arch.json"
    novaFS/lab/run run --offline --example txn-latency > "$tmp/transactions-$arch.json"
done
unset NOVAFS_TARGET
python3 novaFS/lab/measure-check.py > "$tmp/controle.json"
python3 novaFS/lab/compare-ecritures.py > "$tmp/groupement.json"
for ref in d0ba7cb 38c8699 6f15350 1510eca 0a89d23; do
    mkdir "$tmp/$ref"
    git archive "$ref" novaFS | tar -x -C "$tmp/$ref"
    CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py --reference "$tmp/$ref/novaFS/lab/run"
    case "$ref" in 6f15350|1510eca|0a89d23)
        python3 novaFS/lab/test-pool-interop.py --reference "$tmp/$ref/novaFS/lab/run";;
    esac
done
```

Le banc de coupures demande QEMU x86_64 et EDK II. Le dossier temporaire conserve les images et mesures jusqu’à sa suppression par l’opérateur.
