# NovaOS v0.17.0 — Qualification SMP de phase 4

À quatre vCPU x86_64, les quatre couples épinglés atteignent **19 205 allers-retours/s** en médiane, contre **15 681** pour la phase 3 et **16 567** pour le témoin historique rejoué. Les six démarrages appariés donnent un gain par rapport à la phase 3 sur les deux horloges. À charge constante d’un seul couple, la latence médiane à quatre vCPU descend de 52,60 à 47,58 µs sur x86 et de 69,11 à 60,62 µs sur ARM. [Résultats individuels et quantiles](validation/ipc-smp-phase4-performances.json)

## Références de comparaison

Le témoin historique `f356970` porte le banc IPC sur le noyau qui précédait les optimisations de phase 3. Le témoin `4e741fb` est la branche SMP au début de cette phase. Le candidat `a8dc253` apporte le correctif du chemin chaud. Les trois noyaux de chronométrage emploient les mêmes acteurs et le même contrôleur tirés de `c34a54f`. Les essais d’équité et de profilage utilisent leurs propres constructions. [Phase 3](ipc-smp-phase3-v0.17.0.md) · [Profilage des sites](ipc-smp-phase4-profil.md) · [Ablations et coûts mémoire](ipc-diagnostic-phase4-v0.17.0.md)

## Cause et portée du correctif

Dans le cas de quatre couples concurrents, une acquisition réelle protège plusieurs opérations de gestion des canaux. Le nombre d’acquisitions avait baissé en phase 3, mais le profil historique apparié montre que le temps passé à choisir un receveur et à signaler les attentes avait augmenté. Par lot de 4 000 tours sur x86, les deux sélections de tête s’exécutent chacune environ 130 000 fois. La notification inclusive passait de 88,5 à 124,4 ms. Les sommes de plusieurs sites recouvrent une partie du même travail.

Sur x86, le passage de la profondeur de garde par des lectures/écritures séparées avait remplacé les opérations atomiques de lecture-modification-écriture. La réintroduction isolée des trois opérations antérieures relève le débit dans les trois démarrages exploratoires par rapport à leurs témoins. La diffusion des IPI et l’ancien pool d’attentes ne reproduisent pas ce bénéfice. Les mesures établissent le rôle de la section critique commune et de son coût par appel. Elles ne permettent pas d’attribuer toute la différence à une instruction x86 ou à une préemption précise par macOS. Les opérations sur les tables de pages, les identités et les tables de capacités ont chacune été mesurées, puis des variantes ont été retirées ou combinées pour isoler leur effet. [Mesures par site](ipc-smp-phase4-profil.md) · [Comparaisons expérimentales](ipc-diagnostic-phase4-v0.17.0.md)

| Chemin corrigé | Invariant |
|---|---|
| Identité CPU et acquisition récursive | La garde d’ordonnancement est acquise IRQ masquées. `Core::current`, la sélection et le réveil utilisent l’indice du propriétaire. Les deux sections déjà protégées reçoivent une référence à la garde. Aucun emprunt de `Core` ne traverse la commutation. L’assembleur libère le propriétaire après le changement de pile. Le thread reprend la garde sur le CPU où il s’exécute. |
| Collecte des tables locales de capacités | La création d’une table et le retrait d’un handle marquent le slot pouvant devenir vide. La fin de la transaction, même sur erreur, contrôle les tables marquées et libère celles dont toutes les entrées sont vides. Détenteur, génération et droits sont vérifiés à chaque utilisation. |
| Appartenance d’un cadre à la RAM | Sous le verrou mémoire, le gestionnaire retient les deux bornes de sa dernière région physique admise. Le passage par la restitution du chargeur conserve la carte et peut ajouter des régions admissibles. Chaque accès utilisateur vérifie encore la géométrie, les PTE, les droits et l’étendue entière avant la copie. Un débordement de fin d’adresse est refusé. |

L’ordre d’acquisition entre garde d’ordonnancement et verrou mémoire est conservé. La projection d’un handle, le choix du receveur, le contrôle d’une PTE et la publication restent dans leur transaction protégée. Les déplacer hors de cette transaction exigerait une protection de durée de vie et une revalidation après reprise. Les trois modifications évitent du travail répété sous les verrous existants. La topologie, la FIFO des prêts, les acquittements TLB et le protocole de réveil restent ceux des phases précédentes. [Garde](../kernel/src/sched/gate.rs) · [Ordonnancement](../kernel/src/sched/mod.rs) · [Traitement IPC](../kernel/src/sched/process/ipc/traffic.rs) · [Domaine IPC](../kernel/src/sched/process/ipc/mod.rs) · [Cadres](../kernel/src/memory/frames.rs)

## Méthode de mesure

Le Mac possède une puce M4 Max, 12 cœurs Performance, 4 cœurs Efficiency et 128 Gio de mémoire. macOS 27.2 build 26B123 exécute QEMU 11.1.3 en TCG, avec 256 Mio par invité, machine `q35` et processeur `max` sur x86, machine `virt`, Cortex-A72 et GICv2 sur ARM. Les trois constructions sont en release, hors ligne, avec Cargo verrouillé et rustc 1.99.1. Les fréquences et l’empreinte de chaque ELF figurent dans les résultats.

Chaque démarrage chauffe avec un lot, puis en mesure sept. Chaque client fait 1 000 allers-retours de 128 octets et date sa sortie. Le contrôleur s’endort pendant les échanges. Le débit global vaut le nombre total de tours divisé par le temps du dernier client. Le débit individuel utilise le temps depuis le départ commun jusqu’à la sortie du client considéré. La médiane porte sur les lots, p95/p99 sont au rang supérieur et la dispersion est l’écart interquartile. Sept lots dans une même VM partagent un étalonnage et un état du système. Les gains appariés par redémarrage sont également publiés. Pour les lectures de latence individuelle, les sondes isolées fournissent l’histogramme du premier client du lot.

Les temps de l’hôte entre marqueurs série incluent la collecte du flux. Les temps CPU utilisateur et système du lancement QEMU, relevés par `getrusage`, comprennent l’amorçage et tous ses threads. Les ticks invité indiquent l’activité en mode utilisateur, noyau ou idle. Ces horloges ne donnent ni le placement physique de chaque vCPU, ni une durée exclusive d’exécution de TCG. La fréquence du compteur ARM est 62,5 MHz. Sur x86, un retard du PIT lors de l’étalonnage affecte la conversion du TSC en ns. Les relevés conservent donc les deux horloges et la charge hôte.

## Résultats à charge de référence

La campagne de référence comprend 384 démarrages séquentiels et 2 688 lots hors chauffe. Les suites fonctionnelles et UTM avaient fini avant le premier démarrage. La charge hôte à une minute varie de 4,70 à 10,97 et l’étalonnage x86 de 0,992 à 1,006 GHz. Les variantes d’affinité et les profils instrumentés ont deux redémarrages par cellule. Les six redémarrages par version et par scénario principal donnent 42 lots mesurés par cellule. À quatre couples x86, le gain de débit médian du candidat est de 22,5 % sur la phase 3 et 15,9 % sur l’historique rejoué. Sur les six redémarrages pris séparément, le gain face à la phase 3 va de 17,4 à 28,9 % dans l’invité et de 17,5 à 28,6 % côté hôte. Le lecteur série mesure 19 100 tours/s pour le candidat. Les 42 lots invités du candidat vont de 18 387 à 20 414 tours/s. Le repère publié de 17 187 tours/s se trouvait dans la distribution du témoin historique rejoué. Sur ARM, à quatre couples, le candidat passe de 14 470 à 16 754 tours/s (+15,8 %).

| Débit médian x86_64 à quatre couples, tours/s | Essai 1 | Essai 2 | Essai 3 | Essai 4 | Essai 5 | Essai 6 |
|---|---:|---:|---:|---:|---:|---:|
| Historique | 17 285 | 16 616 | 16 881 | 15 445 | 16 114 | 16 375 |
| Phase 3 | 16 001 | 15 584 | 15 652 | 16 147 | 15 523 | 14 473 |
| Phase 4 | 19 766 | 19 507 | 19 363 | 18 959 | 19 100 | 18 655 |

### Un couple libre sur tous les vCPU

Latence par tour, en µs. Chaque valeur de lot est le temps de 1 000 tours divisé par 1 000. Les colonnes de queue portent sur les lots de chaque topologie.

| Architecture | vCPU | Historique p50 | Phase 3 p50 | Phase 4 p50 / p95 / p99 | IQR phase 4 |
|---|---:|---:|---:|---:|---:|
| aarch64 | 1 | 57,9 | 56,0 | 50,6 / 54,9 / 56,3 | 2,4 |
| aarch64 | 2 | 92,6 | 66,7 | 63,0 / 66,5 / 66,8 | 4,3 |
| aarch64 | 4 | 159,6 | 69,1 | 60,6 / 64,3 / 67,4 | 2,1 |
| x86_64 | 1 | 39,6 | 41,1 | 33,6 / 34,9 / 37,0 | 0,7 |
| x86_64 | 2 | 51,8 | 51,0 | 44,9 / 48,5 / 49,1 | 5,8 |
| x86_64 | 4 | 76,3 | 52,6 | 47,6 / 48,9 / 50,4 | 2,3 |

### Un couple épinglé par vCPU

Débit en allers-retours/s. Les percentiles élevés de débit décrivent les lots rapides. Le minimum conserve le lot le plus lent. Les données donnent aussi les percentiles de durée et les valeurs individuelles de chaque couple.

| Architecture | Couples / vCPU | Historique p50 | Phase 3 p50 | Phase 4 p50 / p95 / p99 | Min. / IQR phase 4 | Hôte p50, hist. / P3 / P4 |
|---|---:|---:|---:|---:|---:|---:|
| aarch64 | 1 | 17 389 | 17 900 | 20 113 / 20 486 / 20 521 | 18 321 / 338 | 17 306 / 17 812 / 19 998 |
| aarch64 | 2 | 12 541 | 17 835 | 20 480 / 21 144 / 21 376 | 19 081 / 526 | 12 486 / 17 703 / 20 316 |
| aarch64 | 4 | 6 620 | 14 470 | 16 754 / 17 113 / 17 288 | 13 686 / 472 | 6 604 / 14 402 / 16 671 |
| x86_64 | 1 | 24 791 | 24 224 | 29 239 / 29 762 / 29 870 | 28 237 / 691 | 24 573 / 24 073 / 29 133 |
| x86_64 | 2 | 22 735 | 20 967 | 25 661 / 26 194 / 26 429 | 23 150 / 608 | 22 782 / 20 950 / 25 576 |
| x86_64 | 4 | 16 567 | 15 681 | 19 205 / 19 920 / 20 414 | 18 387 / 679 | 16 484 / 15 621 / 19 100 |

Les débits médians individuels des clients sont calculés depuis le même départ commun.

| Architecture | Couples | Débit de chaque couple, tours/s, phase 4 | Temps CPU hôte médian, s par VM, P3 → P4 |
|---|---:|---:|---:|
| aarch64 | 1 | 20 113 | 1,16 → 1,13 |
| aarch64 | 2 | 10 251 / 10 250 | 2,65 → 2,41 |
| aarch64 | 4 | 4 245 / 4 271 / 4 219 / 4 227 | 10,12 → 8,93 |
| x86_64 | 1 | 29 239 | 2,20 → 2,16 |
| x86_64 | 2 | 13 056 / 12 830 | 3,69 → 3,41 |
| x86_64 | 4 | 5 065 / 4 861 / 4 817 / 4 828 | 10,48 → 9,02 |

### Affinités et contention

Débits globaux médians en tours/s, phase 3 → phase 4. Les deux acteurs de « distant » sont répartis entre CPU0 et CPU1. Les couples fixes du tableau précédent sont épinglés chacun sur leur propre CPU. Les couples libres peuvent migrer.

| Architecture | vCPU | Un couple sur CPU0 | Un couple distant | Tous les couples sur CPU0 | Tous les couples libres |
|---|---:|---:|---:|---:|---:|
| aarch64 | 1 | 18 075 → 19 946 | s.o. | s.o. | s.o. |
| aarch64 | 2 | 17 093 → 19 613 | 10 854 → 12 425 | 15 372 → 17 193 | 18 035 → 20 364 |
| aarch64 | 4 | 16 914 → 19 462 | 10 556 → 11 996 | 12 365 → 14 163 | 14 472 → 16 663 |
| x86_64 | 1 | 24 204 → 28 919 | s.o. | s.o. | s.o. |
| x86_64 | 2 | 23 349 → 28 295 | 12 581 → 15 177 | 19 983 → 24 127 | 20 522 → 25 091 |
| x86_64 | 4 | 23 369 → 28 285 | 12 708 → 15 169 | 14 592 → 17 557 | 15 841 → 19 303 |

Temps cumulés de garde sur tous les vCPU pendant le lot avec sondes. Les acquisitions, commutations et reprises sont des médianes de compteurs par lot. Une reprise est comptée lorsqu’un thread marqué au réveil est de nouveau élu.

| Architecture | vCPU | Acquisitions P3 → P4 | Attente, ms P3 → P4 | Détention, ms P3 → P4 | Commutations P4 | Reprises P4 |
|---|---:|---:|---:|---:|---:|---:|
| aarch64 | 1 | 16 026 → 16 026 | 0,0 → 0,0 | 51,4 → 46,4 | 2 004 | 1 997 |
| aarch64 | 2 | 32 194 → 32 265 | 120,3 → 108,0 | 138,1 → 134,2 | 4 008 | 3 980 |
| aarch64 | 4 | 65 101 → 65 187 | 1 087,8 → 940,3 | 434,7 → 362,1 | 8 016 | 7 909 |
| x86_64 | 1 | 16 026 → 16 024 | 0,0 → 0,0 | 37,8 → 31,0 | 2 004 | 1 998 |
| x86_64 | 2 | 32 369 → 32 344 | 96,3 → 77,9 | 100,2 → 80,8 | 4 010 | 3 987 |
| x86_64 | 4 | 66 172 → 65 824 | 800,1 → 663,0 | 306,2 → 263,8 | 8 018 | 7 923 |

À quatre vCPU x86 et quatre couples, la durée moyenne par acquisition, calculée pour chaque lot puis résumée par sa médiane, passe de **4,63 à 4,01 µs**. Le temps de détention cumulé baisse de 306,2 à 263,8 ms. La sonde mesure 8 018 commutations et 7 923 reprises médianes du candidat par lot. Le nombre d’acquisitions physiques demeure voisin. Les réentrées locales évitées ne donnaient pas lieu à une nouvelle attribution du verrou.

Les histogrammes instrumentés portent sur les tours individuels du premier client. À quatre vCPU x86, les médianes de leurs p50/p95/p99 passent de 57,3 / 69,6 / 90,1 µs à 53,2 / 61,4 / 90,0 µs. Sur ARM à quatre vCPU, elles passent de 94,2 / 122,9 / 180,2 µs à 94,2 / 127,0 / 180,2 µs. Dans cette dernière cellule instrumentée, le débit médian recule de 0,3 %. Les 42 lots monocouple sans sonde montrent une baisse du p99 moyen par tour de 74,7 à 67,4 µs sur ARM et de 59,0 à 50,4 µs sur x86. Les mesures individuelles dans l’invité et les durées murales entre marqueurs restent disponibles pour la comparaison.

## Essais sous charge hôte

Une campagne complémentaire a démarré 96 invités pendant les régressions fonctionnelles, avec deux démarrages par version et par cellule. La charge hôte à une minute allait de 9,86 à 49,12. À quatre couples x86, les débits médians invités étaient de 9 839, 8 509 et 7 919 tours/s pour l’historique, la phase 3 et la phase 4. L’horloge hôte donne respectivement 9 590, 7 734 et 7 887. Les deux démarrages du candidat ont donné 6 885 et 13 174 tours/s sur l’horloge invitée. À quatre couples ARM, les débits médians invités sont de 2 987, 7 219 et 8 571 tours/s. L’étalonnage x86 atteint 1,421 GHz dans ces 96 essais. Le débit en surcharge et sa dispersion font partie des limites de la qualification. [Démarrages et mesures sous charge](validation/ipc-smp-phase4-sous-charge.json)

## Intégrité et équité

Les douze démarrages d’équité à 1, 2 et 4 vCPU suivent 13 acteurs, y compris des travailleurs de calcul et des acteurs révoqués. Les 360 intervalles de progression des acteurs éligibles avancent. La plus longue attente prêt→élu vaut 182,995 ms, le maximum entre deux entrées en exécution vaut 345,091 ms. Chaque réveil attendu, la séquence et les 128 octets du message sont vérifiés. La FIFO des prêts tient compte de l’affinité. La garde commune par comparaison-échange permet à un nouvel acquéreur de passer devant un ancien. **L’absence de famine est observée pendant ces campagnes. Une borne formelle d’attente demanderait une politique d’admission équitable de la garde et des hypothèses sur le service des vCPU.** Le rapport d’équité détaille les priorités disponibles et la solution envisageable si une telle borne devient un critère. [Progression et délais par acteur](ipc-equite-phase4-v0.17.0.md)

Pour les six topologies, les tests ont exercé les 25 démarrages historiques et 50 000 ordres de course IPC par cellule. À quatre vCPU, deux acteurs de la sonde historique pouvaient occuper chacun leur CPU et ainsi ne jamais déclencher la préemption demandée. Les ELF témoins reproduisent le défaut. Épingler les deux acteurs du test sur le même CPU a rendu la commutation obligatoire et préservé le contrôle des registres. Les autres bancs exercent l’affinité libre et la migration. Le test de frontière T13 sous `ipc-smp-lab` vérifie aussi la collecte du slot 31, la réattribution à une nouvelle génération et le refus des anciens handles sur ARM et x86. [Rapport des régressions](smp-phase4-regressions-v0.17.0.md) · [Test T13](../kernel/src/sched/process/ipc/probes/limits.rs)

L’endurance IPC à quatre vCPU a totalisé 25 524 cycles en 301,041 s sur AArch64 et 32 119 en 300,028 s sur x86_64, avec respectivement 62 492 et 62 993 cadres libres stables. Au pic de saturation, les deux architectures mobilisent 71 cadres IPC, tous rendus à la destruction. Les images de comparaison, sondes incluses, occupent 12 Kio de plus sur ARM et 8 Kio de plus sur x86. Les derniers ELF ordinaires à quatre vCPU ont été revalidés au commit `d7b3544` avec rendez-vous, IPC, courses, ELF et cycle de processus. [Comptabilité mémoire](ipc-diagnostic-phase4-v0.17.0.md#mémoire) · [Endurance](smp-phase4-regressions-v0.17.0.md)

L’empreinte des sources de l’ABI, du moteur NovaFS et des composants utilisateur est identique au témoin de départ. Le profil ordinaire reste en E/S classiques et polling. Les dix profils de stockage construits par application de fixtures ont été réexportés et toutes leurs séries de patches se sont appliquées sans tolérance de contexte au commit `d7b3544`. En plus de ces contrôles de sources, le profil groupé a effectivement écrit, synchronisé et relu 8 Mio après une insertion de 4 Mio. À quatre vCPU, il a pris 274,004 ms sur QEMU ARM, 580,703 ms sur QEMU x86 et 339,986 ms dans l’application UTM avec QCOW2. Chaque essai a nécessité deux FLUSH et 56 échanges contrôles inclus, ou 64 en comptant les copies du prêt. Les coupures de VM conservent l’alimentation du Mac. L’affectation aux cœurs physiques du Mac et la survie à une perte électrique du support restent indéterminées. [Stockage, firmware, DMA et essais UTM](smp-phase4-regressions-v0.17.0.md#utm-et-sauvegarde-durable)

## Décision proposée

**Proposer la fusion de la branche SMP à la MOE/MOA.** Sur le profil QEMU de référence, la régression x86 à quatre couples est corrigée et les latences monocouple baissent sur les six topologies. Les campagnes fonctionnelles, les contrôles des données et les tests de révocation ont réussi. Les baisses constatées sous forte charge hôte, la queue de latence des tours ARM instrumentés, le débit sur matériel physique et la borne théorique de non-famine restent documentés. Le profil de stockage de distribution demeure classique. Une fusion ou une publication de version nécessite une décision distincte.

## Reproduction

Depuis la racine du dépôt, préparer un répertoire neuf. Les noms des trois versions et les deux fichiers du banc commun sont fixés dans le lanceur. Les variantes historiques et celles de qualification sont exécutables séparément par architecture, topologie et cas. La configuration de la VM et la validation des résultats empruntent le banc de phase 3. [Lanceur de phase 4](../tests/ipc_phase4_performances.py) · [Mesure dans une VM](../tests/ipc_multicoeur.py)

```sh
lab=novaOS/build/qualification-ipc
python3 -B novaOS/tests/ipc_phase4_performances.py construire "$lab/elf"
mesurer() {
    python3 -B novaOS/tests/ipc_phase4_performances.py mesurer "$lab/mesures" \
        --historique "$lab/elf/historique" --temoin "$lab/elf/phase3" \
        --candidat "$lab/elf/phase4" "$@"
}
mesurer --cas libre paires --repetitions 6
mesurer --cas local distant paires-cpu0 paires-libres paires-sonde sonde --repetitions 2
python3 -B novaOS/tests/ipc_phase4_performances.py rapport "$lab/mesures" "$lab/resultats.json"
```

## Traçabilité Git

Les commits suivants appartiennent à `codex/smp-0170`. Le commit documentaire qui contient ce rapport et les chronométrages finaux se retrouve par `git log -1 -- novaOS/docs/ipc-smp-phase4-v0.17.0.md`.

| SHA | Modification |
|---|---|
| `a8dc2534ac9fabcca2e480d1054fd0b60f180f17` | Réutiliser la garde et borner les parcours sous verrou |
| `c34a54f8ea622c658bcc6ce30693a0a02c6d2a35` | Mesurer les affinités des couples et le CPU hôte |
| `0533849ef66d2ebe2060afc634acd65767d84e34` | Recaler la fixture de stockage sur le domaine IPC |
| `9d82d6a85747f18b3e5eb40ad8698f3840139e97` | Qualifier la progression individuelle sous contention |
| `17e8de27238355d2dbf6cf07589bfa15e6294bd0` | Vérifier les frontières du cache de régions RAM |
| `6cd0d47a3f83c2deeea99aff3a8d63cb39bbc22b` | Archiver les mesures individuelles d’équité |
| `ead7e98cae43d27207b5f4786178b210eebfff8e` | Exercer la préemption des processus sur les trois topologies |
| `214500e123c0d8741d3fb72f0f5a5c165e9f7816` | Ventiler les coûts IPC et vérifier les parcours RAM |
| `42b69ded5e66bec6d8d1a1d8024f0fa8ac9cd1d6` | Apparier les démarrages et les deux horloges |
| `df10e1d9f81063748bae5203741f6f8f07748684` | Archiver les mesures sous charge concurrente |
| `d7b3544d04a7a2055902904bf3f463a1d8a9c3e8` | Vérifier le dernier slot de capacités |
| `3db72f4df94ad7c22ea2cf9575b57f44874aae4b` | Reproduire les ablations et archiver les cadres mémoire |
| `3becdbd6aa4f2376bbee381172f581f1b7edc55e` | Qualifier les régressions multicœurs, NovaFS, DMA et UTM |
