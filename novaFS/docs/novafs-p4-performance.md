# NovaFS P4 v0.9.4 — audit et validation

## Audit préalable du commit 1510eca

| Chemin P3 | Comportement observé | Décision à éprouver |
| --- | --- | --- |
| Cache du moteur monté | CLOCK conserve jusqu’à 256 blocs validés. La recherche d’une clé et le contrôle de doublon parcourent une liste. La limite exprime des pages et ne facture pas leur index. | Borner ensemble le contenu et les métadonnées dynamiques, mesurer le coût de la recherche, des évictions et de la restitution. |
| Lecture et préparation d’une écriture | Pour une plage de 64 Kio, la boucle consulte l’arbre des extents une fois par page, même si un seul extent couvre toute la plage. Le décodage des cellules et des listes d’empreintes alloue à chaque consultation. | Réutiliser le résultat d’une recherche pour les pages suivantes du même extent. Vérifier chaque page et conserver le même plan CoW. |
| Publication | Le plan P3 est complet et réservé avant la première écriture. F1 stabilise les dépendances, une seule ancre est remplacée, F2 précède le succès au client. Le banc P3 agrégeait le temps des deux barrières. | Mesurer séparément F1 et F2, distinguer coût de préparation et coût physique de publication. Garder la séquence ordonnée de P1. |
| Concurrence | Le prêt `Pool::view` mobilise le pool en exclusivité. Un `PreparedWrite` P3 retient son moteur. Le mutex des tickets de grains n’ordonne pas les requêtes FS. | Un propriétaire par pool, admissions bornées, lectures partageables et validation sérialisée. Mesurer les vrais chemins hôtes et décrire les limites de progression. |

Avant les modifications du produit, une copie isolée de P3 a exécuté le scénario de pression et de réemploi sur ARM64 avec des secteurs de 4 096 octets. Pour 162 transactions de 4 Kio, le périmètre instrumenté a compté 1 182 lectures de blocs, 1 083 écritures et 37 064 allocations. Les 162 F1 ont pris 684 504 375 ns au total, les 162 F2 ont pris 671 982 298 ns. Le banc a observé 157 blocs réutilisés. Les essais de comparaison utilisent également l’archive Git immuable de P3. [Mesures de référence publiées en P3](novafs-p3-mesures.md)

## Décisions et effets mesurés

Le cache ne contient que des pages validées. Son index borné remplace la recherche linéaire de P3 et conserve l’éviction à seconde chance. Le budget par montage peut être nul. Les vérifications intégrales et les remontages contournent le cache. Dès qu’une faute est constatée par le moteur, ses pages sont purgées et une lecture en vol ne peut pas les réintroduire. Une image modifiée hors du moteur peut rester **inconnue** jusqu’à une lecture non servie par le cache ou à un audit. Le coordinateur répartit son propre budget entre LV et garde une seule autorité de publication. [Architecture et limites du cache](novafs-p4-cache.md) · [Ordre de publication, annulation et reprise](novafs-p4-coordinateur.md)

### Parcours des extents

La lecture suit un extent déjà résolu jusqu’à sa dernière page utile, puis reprend la recherche au prochain bloc. Elle réserve une fois son tampon de résultat, ajoute les octets vérifiés et initialise à zéro les plages creuses. Une écriture conserve de même le résultat de la recherche pendant la préparation des pages d’un extent. Elle relit et contrôle l’ancienne page, même en cas de remplacement intégral. Le nombre d’adresses réservées et l’ordre des écritures du plan restent déterminés par P3.

Le microbanc utilise un fichier de 64 Kio couvert par un seul extent, une lecture après chauffe et une réécriture préparée puis annulée. Il réalise cinq séries de 128 opérations pour chaque géométrie et chaque architecture. Une troisième variante exécute **uniquement** la modification de `fs/data.rs` sur le moteur P3 pour distinguer le bénéfice propre à ce parcours de celui du nouvel index de cache. Les compteurs incluent le vecteur rendu et les demandes de mémoire temporaires. Les octets demandés cumulés ne constituent pas un pic de mémoire vivante.

| Opération de 64 Kio | P3, lectures de blocs par opération | P4, lectures | P3 → P4, demandes d’allocation | P3 → P4, octets demandés |
| --- | ---: | ---: | ---: | ---: |
| Lecture chaude | 0 | 0 | 81 → 21 | 83 713 → 67 123 |
| Préparation + annulation | 37 | 22 | 150 → 90 | 109 964 → 93 374 |

Les comptes obtenus avec la seule modification des extents sont identiques à ceux de P4 sur cette charge, sur les deux architectures. Le banc exige une génération inchangée après annulation. Les tests fonctionnels couvrent aussi les trous, les limites de pages et d’extents et une corruption sur la dernière page d’un ancien extent. Dans ce dernier cas, ni la lecture ni le remplacement complet ne peuvent réussir, et le refus n’émet aucune écriture ni aucun flush. [Séries brutes des trois variantes](validation/v0.9.4/extents.json) · [Microbanc](../tests/cout_extents.rs) · [Vérification des limites et erreurs](../tests/plages_extents.rs)

### Regroupement des soumissions sur fichier

Un second essai rejoue le journal réel d’une transaction P1 sur deux copies d’une même image macOS. Il alterne l’ordre des variantes, compare les images octet par octet et les fait remonter par le moteur. La variante groupée utilise `pwritev` uniquement pour des blocs contigus d’une même phase. Chaque variante conserve les deux `F_FULLFSYNC` et écrit l’ancre après F1. Le chronométrage porte sur le rejeu hôte Python ARM64 ; il exclut la copie initiale, la préparation Rust et la vérification finale.

Sur 25 paires par géométrie, le regroupement ramène les soumissions d’écriture de huit à quatre. Les médianes de la transaction sont de 8,622 → 8,634 ms en secteur 512 et de 8,635 → 8,616 ms en secteur 4 096. Hors barrières, les médianes passent respectivement de 68,2 à 53,5 µs et de 59,5 à 46,1 µs. Le gain total n’a pas été reproductible entre les deux géométries. Le backend livré conserve les soumissions par bloc et les points d’injection P1. Aucun cache d’écriture n’est introduit. [Script du rejeu](../lab/compare-ecritures.py) · [Durées brutes](validation/v0.9.4/groupement.json)

La répétition sur le code final donne 11,286 → 11,559 ms en secteur 512 et 11,757 → 11,257 ms en secteur 4 096. Une diminution du nombre d’appels système ne suffit donc pas, dans ce banc, à prévoir la latence d’un commit avec ses deux synchronisations. [Deuxième série complète](validation/v0.9.4/groupement-repetition.json)

## Validation fonctionnelle

Cargo a rapporté **95 tests ordinaires réussis sur chacune des deux architectures**. La campagne différentielle séparée a observé, par architecture, 345 183 commits, 54 643 pénuries injectées et 1 000 remontages avec audit en un million d’opérations. Le banc P3 de réservations et de restitution a également été relancé avec les deux tailles de secteur. [Bilan des exécutions](validation/v0.9.4/campagne.json) · [Relevés du vérificateur sur 8 Gio et sur 16 LV](validation/v0.9.4/memoire-controle.json)

| Exigence | Observation et périmètre |
| --- | --- |
| 1–2. Régressions P0–P3 et différentiel | Les suites des deux cibles conservent les 15 scénarios P0. Le test long distinct confronte un million d’opérations par architecture au modèle de référence avec pénuries, remontages et audits. |
| 3–4. Répétitions, évictions, pression | Le cache plein a été exercé à capacité identique à P3. Les essais passent aussi par zéro, une page, des collisions de seaux et le plafond de 512 pages. Le contenu et l’index sont rendus sous pression. Le refus d’allocation pour une admission facultative laisse la lecture vérifiée aboutir. |
| 5–7. Modification, remontage et corruption | Les essais contrôlent le réemploi de la même adresse physique avec une nouvelle naissance, les mutations de données et métadonnées et le remontage. Un audit détecte un bloc physique corrompu alors qu’une autre lecture est en vol. Après constat du défaut, le moteur ne sert plus ses hits et cette lecture ne peut pas repeupler le cache. |
| 8–9. Arrêts et échecs de synchronisation | Le modèle P1 couvre les déchirures et la perte du cache d’écriture. Les essais réels de processus et de VM utilisent toujours les frontières de F1/F2. Le coordinateur a subi des échecs avant et pendant publication, puis un réimport indépendant avec vérification des octets des deux LV. |
| 10–12. Disque plein, quotas et espace | Le corpus P3 confronte la partition des blocs aux deux racines. Le refus pour quota ou saturation avant publication n’écrit aucune dépendance. Les essais P4 exercent des clients sur plusieurs LV du même pool et un refus de quota qui laisse avancer la file. |
| 13. Restitution | Le banc mesure la restitution du tas après évictions. Le test du coordinateur vérifie également sa ligne de base après chaque cycle d’annulation ou de F2 indéterminée, réimport et destruction. Les campagnes P2 comparent aussi les images saines et corrompues de 8 Gio et le pool de 16 LV octet par octet après vérification. |
| 14–15. Architectures et images existantes | Les tests F01 utilisent des Mach-O distincts, ARM64 natif et x86_64 sous Rosetta, les deux tailles de secteur et les lecteurs et producteurs de P0, P1, P2, P3 et P4. Le catalogue, ses 16 LV et leurs compteurs sont également comparés entre P2/P4 et P3/P4. |

Le corpus du coordinateur distingue la charge simulée de huit clients, les opérations dont le chevauchement est attesté par rendez-vous entre threads **macOS**, et la publication exclusive. Les garanties de la file supposent qu’un titulaire avance ou annule sa demande. Le passage en IPC, l’ordonnancement d’un futur service et l’exclusion entre processus externes sur le même PD restent hors qualification. Les coupures de VM du banc P1 testent le disque virtuel et sa sonde UEFI. Aucun composant commun de NovaOS n’a changé. [Résultats et limites du coordinateur](novafs-p4-coordinateur.md) · [Mesures de débit, latence, F1/F2 et mémoire](novafs-p4-mesures.md)

Deux défauts de propagation ont été corrigés pendant la revue. Un appel direct aux traits publics de lecture pouvait signaler une corruption ou une E/S en échec sans retirer les pages du cache de l’instance. Le premier appel préserve désormais sa cause et les appels ultérieurs refusent le moteur gelé. Dans le coordinateur, un périphérique pouvait rendre `NoSpace` après avoir accepté une écriture. Le coordinateur se fonde désormais sur l’état réel du moteur après cet échec de publication, ce qui empêche un deuxième LV de publier. Un refus de quota à la préparation conserve le comportement local attendu. [Tests des lectures directes et en vol](../tests/cache_p4.rs) · [Tests de publication du pool](../tests/coordinator.rs)

## Reproduire depuis la racine

Les deux cibles Darwin, Rosetta et les dépendances Cargo hors ligne sont nécessaires. Les essais de coupure P1 demandent QEMU et EDK II. Les mesures de RSS utilisent `/usr/bin/time -l` et les compteurs Darwin.

```sh
set -e
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    novaFS/lab/run test --offline --test cout_extents -- --ignored --nocapture
    python3 novaFS/lab/test-persistence.py --support tous
    for secteur in 512 4096; do
        novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" reservations
        novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" reprise
    done
done
unset NOVAFS_TARGET
python3 novaFS/lab/compare-ecritures.py
python3 novaFS/lab/measure-check.py

(
    refs=$(mktemp -d)
    trap 'rm -rf "$refs"' EXIT
    for commit in d0ba7cb 38c8699 6f15350 1510eca; do
        mkdir "$refs/$commit"
        git archive "$commit" novaFS | tar -x -C "$refs/$commit"
        CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py \
            --reference "$refs/$commit/novaFS/lab/run"
    done
    p3="$refs/1510eca/novaFS"
    python3 novaFS/lab/test-pool-interop.py --reference "$refs/6f15350/novaFS/lab/run"
    python3 novaFS/lab/test-pool-interop.py --reference "$p3/lab/run"
    for arch in aarch64 x86_64; do
        python3 novaFS/lab/measure-performance.py --arch "$arch" --reference "$p3"
        python3 novaFS/lab/measure-performance.py --arch "$arch"
    done
    # Comparaison ciblée des extents. Modifier seulement la copie temporaire.
    cp novaFS/tests/cout_extents.rs "$p3/tests/"
    for arch in aarch64 x86_64; do
        NOVAFS_TARGET="$arch-apple-darwin" "$p3/lab/run" test --offline --test cout_extents -- --ignored --nocapture
    done
    cp novaFS/src/fs/data.rs "$p3/src/fs/"
    for arch in aarch64 x86_64; do
        NOVAFS_TARGET="$arch-apple-darwin" "$p3/lab/run" test --offline --test cout_extents -- --ignored --nocapture
    done
)
```
