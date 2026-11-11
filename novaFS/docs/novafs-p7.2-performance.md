# NovaFS P7.2 · Validation du chemin d’écriture

## Diagnostic sur la v0.9.10

La référence est le commit `a4ba7173a6dc212e1a59d868e4cfa4488690b4ec`. Avant la modification du moteur, le banc a effectué 64 écritures sur clone LZ4 chiffré par ISA, moitié sur secteurs de 512 octets, moitié sur secteurs de 4 096 octets. Chaque écriture partielle de 8 Kio était suivie d’une lecture comparée à un modèle de 16 Kio. Les deux ISA sont parties des mêmes images. [Diagnostic individuel](validation/v0.9.11/diagnostic.jsonl.gz)

Le problème était une répétition du contrôle intégral dans la même section exclusive. La validation du journal ouvrait les graphes de g et g−1, contrôlait les montages FS puis abandonnait l’index. Le moteur d’écriture recommençait immédiatement ce travail. Pendant chacun de ces montages FS, la traduction de chaque LBA relisait également les racines et les feuilles déjà authentifiées.

| Diagnostic par écriture, moyennes | ARM64 natif | x86_64 Rosetta |
|---|---:|---:|
| Constructions de l’index du graphe | 2 | 2 |
| Blocs de 4 Kio lus au backend | 2 225,9 | 2 225,9 |
| Allocations du tas | 2 415,1 | 2 415,1 |
| Montages FS de vues protégées | 5,81 | 5,81 |
| Temps mural de ces montages | 21,03 ms | 38,62 ms |
| Empreintes des paires, lecture et création | 994,7 | 994,7 |
| Ouvertures AEAD / scellements | 986,7 / 8 | 986,7 / 8 |
| Vérification des empreintes et calcul des nouvelles, temps mural | 13,66 ms | 22,35 ms |
| Ouverture AEAD, temps mural | 4,00 ms | 7,60 ms |
| Section exclusive, temps mural | 34,27 ms | 56,39 ms |

Avec 64 appels, le rang p99 du diagnostic correspond aussi au maximum. Sur cet appel ARM64, la durée totale était de 39,12 ms, les montages de vues de 23,21 ms, F1 de 6,37 ms et F2 de 4,14 ms. Sous Rosetta, ces valeurs étaient de 61,98, 40,03, 5,97 et 7,28 ms. Le coût répété des montages est observable sur ces appels. Les attentes des deux barrières contribuent aussi à la queue. Les durées des fonctions imbriquées se recouvrent. L’ancienne pointe Rosetta de 170,90 ms ne dispose pas de cette décomposition. [Campagne P7.1](novafs-p7.1-consolidation.md)

## Contrôle et durée de vie de l’index

1. L’écrivain acquiert le verrou du périphérique. Le journal relit les huit attestations, les deux banques protégées et les en-têtes. Les contrôles de filiation, de liaison à la session et les HMAC des vues utilisent cet état physique frais.
2. Sur le chemin courant, le parcours complet authentifie chaque nœud, vérifie les arêtes, leur rôle, leur domaine et leur position, puis construit l’index temporaire. Le montage FS utilise ces arêtes pour retrouver ses blocs. Chaque lecture de contenu passe encore par la vérification de son empreinte et, si le volume est chiffré, de son tag. L’énumération indépendante des LBA sur le support, le classement données/métadonnées, les quotas et les deux générations restent contrôlés.
3. Un objet interne déplaçable une seule fois contient le graphe entièrement vérifié et emprunte la garde du mutex. Le seul appelant qui le rattache à une vue inscriptible possède le même périphérique, les mêmes clés et la banque qui vient d’être vérifiée. Le verrou exclut un second écrivain et le code conserve cette banque intacte jusqu’au rattachement. La génération suivante sert à la création des nouvelles unités. Le moteur consomme le graphe, conserve ses réservations CoW et le détruit avant de publier la banque.
4. À la première écriture après montage, le contrôle avant drain et le contrôle après drain sont indépendants. Un changement de répertoire des domaines emprunte le chemin complet historique. Une réparation sur un écrivain déjà actif invalide aussi le graphe temporaire. Le moteur relit alors le graphe pour le montage inscriptible après la barrière de réparation.
5. F1 stabilise les nouveaux objets et leur banque. Les attestations sont publiées ensuite, et F2 précède l’acquittement. En cas de dommage critique détecté pendant la validation, le chemin sort avant l’écriture CoW ou la réparation. La politique d’issue indéterminée après erreur de publication reste en vigueur. [Façade P7](../src/secure_pool.rs) · [Journal](../src/secure_commit.rs) · [Montage des vues](../src/shared.rs)

Le prêt porte sur **une transaction** et un backend qui atteste son exclusivité. Les lecteurs coordonnés et les écrivains concurrents reçoivent `Busy` pendant l’opération. Les fichiers externes importés par descripteur conservent leur accès P7 en lecture seule. Un acteur ou un périphérique qui modifie des octets sous la garde en contournant ce contrat sort du modèle. L’essai de lecture après altération entre transactions couvre une racine de clone appartenant à un autre domaine. Une corruption sur la banque du prédécesseur ferme l’écrivain. Une banque libre de la rotation est entièrement remplaçable sous F1. [Tests ciblés](../tests/p7_secure.rs)

Le diagnostic n’a pas justifié de regrouper les E/S sur le support, de garder les arêtes entre transactions, de déplacer la compression ou la préparation cryptographique hors verrou, ni de modifier les synchronisations. La compression des deux unités prenait en moyenne moins de 0,01 ms dans ce diagnostic. Le choix des nonces, la portée des tags, les enveloppes, le format disque et le format d’export restent ceux de P7.

## Méthode de comparaison

Le banc produit une image P7.1 par configuration, secteur et répétition. Chaque exécutable la copie intégralement dans un fichier temporaire privé et synchronise la copie avant le montage. L’empreinte SHA-256 de la source figure dans chaque observation. Après déverrouillage, une publication de stabilisation et une lecture commune précèdent le chronométrage. Les quatre combinaisons version/ISA partent des mêmes octets, avec le même corpus et les mêmes offsets. Les nouvelles écritures chiffrées utilisent l’entropie réelle. Les cas snapshot et clone écrivent respectivement sur l’origine ayant un snapshot et sur le descendant inscriptible du snapshot.

Deux répétitions inversent l’ordre des versions. Sur chaque profil, chaque ISA et chaque version, le banc recueille 128 durées courantes (2 × 2 géométries × 32 appels) et 16 mesures instrumentées séparées. Le rang est arrondi au supérieur sans interpolation. Le débit divise les octets logiques par la somme des durées d’écriture. L’amplification divise les octets écrits au backend par les octets logiques. La lecture oracle entre écritures est exclue du débit. Le protocole mesure un cache macOS chaud, sans purge globale. L’hôte, le SSD et Rosetta peuvent contribuer à la variance.

Les sondes de phase sont injectées dans des copies temporaires de chaque version. Elles observent les horloges murale et CPU du thread, avec les compteurs locaux bornés de P4.1. Dans les appels instrumentés, chaque triplet est le nombre d’occurrences, la somme murale en ns et la somme CPU en ns. Les deux binaires courants comptent aussi les allocations, les lectures et écritures backend et le temps passé dans F1 et F2. Un appel de bloc peut être servi par le cache de l’hôte. Le banc ne mesure pas les accès physiques internes du SSD. [Banc Rust](../examples/p72-lab.rs) · [Comparaison appariée](../lab/p72-comparaison.py)

| Phase demandée | Observation |
|---|---|
| Acquisition / attente du verrou | `Acquisition` couvre l’obtention de la garde. `try_lock` est non bloquant, donc zéro attente en file. `SectionExclusive` couvre la validation jusqu’après F2. |
| En-têtes, attestations, banques | Classes de LBA 0–1, 2–9 et 10–45 dans `lectures` et `lectures_ns`. `Entetes`, `Attestation` et `Banque` couvrent aussi le décodage associé. |
| Checksums et authentification | `EmpreintePaire` mesure SHA-256 des paires consommées et produites. Le digest de banque est inclus dans `Banque`. `VerificationMac`, `SignaturesVues` et `OuvertureAEAD` distinguent les vérifications cryptographiques. |
| Graphe et générations | `Graphe` mesure le parcours et le nombre de constructions. `MontageVue` comprend le contrôle FS. `SelectionGenerations` mesure la sélection de tête et de parent. `Prevalidation` inclut la vérification de leur filiation et des catalogues. |
| Réservations | `Admission`, `ReservationFS` et `ReservationPhysique` pour l’admission et les acquisitions CoW. |
| Compression et chiffrement | `compression_lz4` et `Scellement`. `OuvertureAEAD` intervient avant la décompression. |
| Préparation et publication | `PreparationFS`, `PreparationBanque`, `Publication`. Cette dernière inclut l’écriture de la banque et les deux synchronisations. |
| Synchronisations | `f_ns[0]` et `f_ns[1]` sur chaque écriture ordinaire. Le banc vérifie que le nombre vaut deux. |
| Invalidation du cache | `Invalidation` mesure les invalidations par bloc dans le cache P4. Le moteur libère aussi le cache temporaire à la destruction du montage. |

## Latences, débit et mémoire sur images appariées

macOS 27.2, Apple Silicon, Rust 1.99.1. Les valeurs présentent P7.1 puis P7.2. Chaque cellule de latence utilise 128 écritures par version et par ISA. Les débits portent uniquement sur ces écritures de 8 Kio. [Tous les appels](validation/v0.9.11/comparaison.jsonl.gz) · [Distributions, horloges et pics par processus](validation/v0.9.11/comparaison.json.gz)

| ISA | Profil | p50 ms | p95 ms | p99 ms | Max ms | Moyenne ms | Débit Mio/s | Amplification × |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| ARM64 | clair | 25,17 → 14,16 | 26,58 → 14,96 | 27,57 → 16,39 | 30,75 → 16,47 | 24,96 → 14,22 | 0,31 → 0,55 | 14,50 → 14,50 |
| ARM64 | chiffré brut | 28,00 → 15,39 | 31,74 → 16,50 | 125,42 → 16,90 | 176,75 → 17,16 | 30,74 → 15,30 | 0,25 → 0,51 | 14,50 → 14,50 |
| ARM64 | chiffré LZ4 | 28,97 → 15,20 | 32,92 → 16,78 | 38,23 → 17,55 | 38,43 → 18,97 | 29,04 → 15,30 | 0,27 → 0,51 | 13,50 → 13,50 |
| ARM64 | origine + snapshot | 35,42 → 16,95 | 40,28 → 22,35 | 44,73 → 32,02 | 50,03 → 32,50 | 35,09 → 17,60 | 0,22 → 0,44 | 13,50 → 13,50 |
| ARM64 | clone chiffré LZ4 | 34,54 → 16,48 | 36,47 → 17,81 | 38,84 → 18,97 | 39,13 → 19,44 | 33,93 → 16,41 | 0,23 → 0,48 | 13,50 → 13,50 |
| Rosetta | clair | 33,08 → 16,99 | 34,54 → 18,25 | 34,72 → 19,14 | 35,25 → 19,36 | 32,71 → 17,05 | 0,24 → 0,46 | 14,50 → 14,50 |
| Rosetta | chiffré brut | 39,34 → 18,74 | 41,24 → 20,25 | 45,21 → 21,10 | 45,50 → 21,73 | 38,86 → 18,82 | 0,20 → 0,42 | 14,50 → 14,50 |
| Rosetta | chiffré LZ4 | 38,61 → 19,03 | 45,18 → 23,70 | 52,40 → 28,25 | 61,80 → 34,57 | 39,35 → 19,73 | 0,20 → 0,40 | 13,50 → 13,50 |
| Rosetta | origine + snapshot | 49,16 → 21,49 | 54,95 → 24,85 | 64,70 → 33,26 | 83,01 → 39,01 | 48,88 → 21,74 | 0,16 → 0,36 | 13,50 → 13,50 |
| Rosetta | clone chiffré LZ4 | 47,79 → 20,97 | 57,77 → 22,73 | 67,23 → 24,84 | 157,66 → 24,92 | 48,37 → 20,85 | 0,16 → 0,37 | 13,50 → 13,50 |

Les phases ci-dessous sont mesurées dans des processus instrumentés distincts, avec 16 écritures par cellule et version. Les lectures de blocs et les allocations sont les moyennes des 128 appels sans sondes internes. La prévalidation inclut la sélection du journal ainsi que le premier contrôle complet des vues.

| ISA | Profil | Lectures de blocs | Allocations | Prévalidation ms | Section exclusive ms | F1 + F2 ms |
|---|---|---:|---:|---:|---:|---:|
| ARM64 | clair | 1711,62 → 528,16 | 1980,81 → 990,59 | 6,43 → 3,40 | 23,17 → 14,18 | 8,31 → 8,29 |
| ARM64 | chiffré brut | 1711,62 → 528,16 | 1980,81 → 990,59 | 7,61 → 3,77 | 26,03 → 14,94 | 10,91 → 8,57 |
| ARM64 | chiffré LZ4 | 1693,62 → 518,16 | 1980,25 → 990,34 | 7,62 → 3,71 | 26,92 → 15,58 | 9,32 → 8,63 |
| ARM64 | origine + snapshot | 2220,88 → 643,53 | 2409,00 → 1135,47 | 9,38 → 4,63 | 32,99 → 19,32 | 9,40 → 9,54 |
| ARM64 | clone chiffré LZ4 | 2225,88 → 646,53 | 2415,12 → 1138,25 | 9,10 → 4,50 | 29,18 → 15,39 | 8,59 → 8,40 |
| Rosetta | clair | 1711,62 → 528,16 | 1980,81 → 990,59 | 11,32 → 6,10 | 34,91 → 18,59 | 8,45 → 8,43 |
| Rosetta | chiffré brut | 1711,62 → 528,16 | 1980,81 → 990,59 | 13,50 → 6,94 | 39,84 → 20,50 | 8,81 → 8,60 |
| Rosetta | chiffré LZ4 | 1693,62 → 518,16 | 1980,25 → 990,34 | 14,03 → 7,14 | 41,08 → 21,05 | 9,56 → 9,50 |
| Rosetta | origine + snapshot | 2220,88 → 643,53 | 2409,00 → 1135,47 | 16,41 → 8,39 | 46,00 → 21,82 | 9,98 → 9,16 |
| Rosetta | clone chiffré LZ4 | 2225,88 → 646,53 | 2415,12 → 1138,25 | 16,26 → 8,35 | 45,79 → 21,73 | 9,05 → 8,73 |

| ISA | Pic tas + pile d’écriture, Mio | Pic déverrouillage Argon2id, Mio | RSS maximal des processus courants, Mio |
|---|---:|---:|---:|
| ARM64 | 2,17 → 2,17 | 66,06 → 66,06 | 67,72 → 67,70 |
| Rosetta | 2,16 → 2,16 | 66,05 → 66,05 | 70,45 → 70,32 |

Le pic de travail additionne le tas comptabilisé par l’allocateur et la totalité des 2 Mio de pile réservée du thread. Le déverrouillage Argon2id a lieu avant la fenêtre d’écriture. Le RSS inclut le processus hôte, ses bibliothèques et la traduction le cas échéant. Tous les processus de mesure retrouvent leur niveau initial de tas après fermeture. Le plafond de 7 Mio a été vérifié sur ces charges.

**Sur le clone chiffré LZ4**, le p99 baisse de 51,2 % sur ARM64 et de 63,0 % sous Rosetta. Les deux objectifs expérimentaux sont atteints sur cette campagne. La moyenne passe de 2 226 à 647 lectures backend par écriture, avec deux synchronisations et 13,5 octets écrits par octet logique dans les deux versions. Dans les séries instrumentées, les constructions du graphe passent de deux à une et les ouvertures AEAD de 736,25 à 194,75 en moyenne, pour huit scellements dans les deux cas. Ces valeurs portent sur les quatre premières écritures de chaque série instrumentée. Le diagnostic initial suivait les 32 positions, y compris après divergence des générations.

**Les pointes restantes dépendent du profil.** Sur l’appel P7.2 au p99 du clone, F1 et F2 occupent ensemble 10,26 ms sur 18,97 ms en ARM64 et 12,14 ms sur 24,84 ms sous Rosetta. Sur l’origine ARM64 avec snapshot, les deux barrières prennent 23,59 ms sur le p99 de 32,02 ms. Sur le volume chiffré brut ARM64 de référence, elles prennent 151,80 ms sur le maximum de 176,75 ms. En revanche, sur le maximum Rosetta du clone P7.1, elles ne prennent que 10,35 ms sur 157,66 ms et le thread accumule 51,66 ms de CPU. La cause du temps restant sur cet appel demeure indéterminée. Aucun des cinq profils ne régresse en moyenne ou au p99 pendant cette comparaison. Deux répétitions sur un hôte partagé ne fixent pas de borne de latence future.

## Récupération, compatibilité et non-régression

| Épreuve | ARM64 natif | x86_64 sous Rosetta |
|---|---:|---:|
| Tests standards réussis | 172 | 172 |
| Tests avec toutes les features réussis | 173 | 173 |
| Opérations différentielles, 100 graines | 1 000 000 | 1 000 000 |
| Publications observées / pénuries injectées / remontages et audits du modèle | 345 183 / 54 643 / 1 000 | 345 183 / 54 643 / 1 000 |
| Coupures P7 / erreurs d’écriture et de synchronisation P7 | 582 / 70 | 582 / 70 |
| Coupures P7.1 pendant réparation et publication | 400 | 400 |
| Observations du banc P7.1, deux répétitions | 20 | 20 |
| Coupures historiques sur fichier / arrêts QEMU, deux remontages chacun | 24 / 10 | 24 / 10 |
| Violations de récupération détectées par ces oracles | 0 | 0 |

Les quatre tests marqués `ignored` dans le harnais général sont exécutés par les bancs dédiés du million d’opérations, des volumes, de la mesure des extents et de l’interopérabilité liblz4. Le banc CoW P5 a exercé par ISA 684 coupures, 152 erreurs d’E/S et douze déchirures sur les deux géométries. La CLI a exercé création, snapshot, export chiffré et restauration sur les quatre combinaisons ISA/secteur. Le sous-ensemble cryptographique a également été compilé pour les deux cibles bare-metal.

Sur les 40 observations du banc de récupération, 8 réparations témoins ont permis une publication remontable. Les 32 observations avec dommage critique ont conservé les octets stables et la génération, avec zéro tentative d’écriture. Les tests supplémentaires exercent une banque hors rotation, une banque parente protégée, un domaine étranger avec plusieurs vues et la réparation sur un montage déjà inscriptible. Le test de la racine partageable compte **86** lectures de son corps sur P7.1 et **2** sur P7.2 pour la même première mutation. Il contrôle aussi les octets du descendant, le nouveau contrôle lors de l’écriture suivante et le refus après altération. La suite historique exerce les corruptions de tags, références, racines, le manque d’espace, le changement de tête entre écrivains et les erreurs de lecture avant réparation.

Le banc de formats a employé huit révisions historiques de P0 à P6. La campagne chiffrée a ensuite effectué seize échanges bidirectionnels avec v0.9.9 et v0.9.10 en changeant d’ISA et de géométrie pour la restauration. Dans chaque cas, le lecteur a contrôlé les données, l’empreinte du pool source a été conservée, puis le pool a été supprimé avant restauration dans un domaine distinct. Les essais de coupure portent sur les modèles et images du banc. La campagne historique QEMU porte sur le stockage virtuel. Aucune coupure électrique d’un SSD physique n’a été exercée. [Détails de qualification et observations individuelles](validation/v0.9.11/qualification.json.gz)

## Reproduction

Depuis la racine du monorepo sur un Mac Apple Silicon avec Rosetta, les cibles Rust stable Darwin et bare-metal, QEMU, la liblz4 de référence, les commits historiques et les dépendances Cargo disponibles hors ligne. Employer un répertoire de sortie neuf. Exécuter la comparaison après la fin des autres charges de validation. Les SHA-256 des exécutables et des images sources permettent de distinguer les campagnes. Les nouvelles images synthétiques utilisent des clés maîtresses et des nonces aléatoires.

```sh
(
  set -eu
  out=$(mktemp -d "${TMPDIR:-/tmp}/novafs-p72.XXXXXXXX")
  lab="$PWD/novaFS/lab"
  python3 "$lab/p72-comparaison.py" reference "$out"
  python3 "$lab/p72-comparaison.py" diagnostic "$out"
  python3 "$lab/p72-comparaison.py" candidat "$out"
  for isa in aarch64 x86_64; do
    python3 "$lab/p6-validation.py" "$isa" --sortie "$out/suites-$isa.json"
    NOVAFS_TARGET="$isa-apple-darwin" "$lab/run" test --offline --test p7_secure -- --nocapture --test-threads=1
    python3 "$lab/p7-cli.py" "$isa"
  done
  awk '/^```sh$/{p=1;next} p && /^```$/{exit} p' \
    novaFS/docs/validation/p7.1-audit.md > "$out/audit.sh"
  for rep in 1 2; do sh "$out/audit.sh" > "$out/observations-$rep.txt"; done
  python3 "$lab/p7-no-std.py"
  python3 "$lab/p6-lz4-interop.py" > "$out/lz4.json"
  python3 "$lab/p6-interop.py" > "$out/p6.json"
  python3 "$lab/p6-historiques.py" --support formats > "$out/formats.json"
  python3 "$lab/p6-historiques.py" --support pannes > "$out/pannes.json"
  python3 "$lab/p7-validation.py" interop --sortie "$out"
  python3 "$lab/p72-comparaison.py" echanges "$out"
  python3 "$lab/p72-comparaison.py" comparer "$out"
  echo "$out"
)
```
