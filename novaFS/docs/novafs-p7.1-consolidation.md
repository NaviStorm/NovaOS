# NovaFS P7.1 · Récupération des volumes chiffrés

## Correction des deux acquittements erronés

Sur la v0.9.9, le montage avait authentifié l’enveloppe à l’ouverture puis gardé la clé de publication et le préfixe du graphe en mémoire. Après altération de la seconde copie de l’en-tête, la reprise relisait les attestations, les banques et le graphe, mais omettait l’en-tête. Elle réparait un certificat et acquittait 28 écritures sur une image que le montage suivant refusait. Sur l’instance déjà inscriptible, le retour immédiat de `CommitLog::resume` omettait également les banques. Une altération de la banque de tête conduisait à 27 écritures et à la publication d’un successeur dont le prédécesseur était devenu illisible. Les deux tests automatisés ont d’abord échoué sur le moteur v0.9.9 sur les deux ISA. [Tests de non-régression](../tests/p7_secure.rs) · [Oracle et banc de l’audit](validation/p7.1-audit.md)

### De l’exclusivité à l’acquittement

1. L’écrivain acquiert le verrou fourni par le backend. Le formatage d’une image neuve le prend également. Le verrou couvre toutes les E/S de validation, la réparation éventuelle, les réservations et opérations CoW, les barrières et l’installation du nouvel état en mémoire. Une tentative concurrente reçoit `Busy`. Un backend sans autorité d’écriture P7 reçoit `ReadOnly` avant la première écriture.
2. Le journal relit les huit emplacements d’attestations. Il vérifie les HMAC avec la clé de publication, sélectionne la tête et son prédécesseur, et contrôle leurs générations et leur filiation. Chaque digest de banque provient d’une attestation authentifiée. Les neuf blocs de chaque banque protégée sont relus et leur SHA-256 est recalculé. Le catalogue précédent contenu dans la banque de tête doit être égal au catalogue de tête de la banque parente. La tête sélectionnée doit être exactement celle de la session.
3. Le moteur relit les deux en-têtes canoniques, exige leur égalité et compare leur contenu au moyen d’un HMAC de session. Il l’a calculé sur les octets exacts de l’en-tête dont l’enveloppe administrative a été authentifiée à l’ouverture, ou créée au formatage. Cette liaison reste en RAM. Elle détecte aussi deux copies identiquement altérées.
4. Le moteur vérifie les HMAC des vues protégées à partir du nouvel état lu, puis rouvre leurs graphes à travers un adaptateur en lecture seule. Il contrôle les liens des instantanés et clones, les identités de domaines, les références, les quotas, les structures et contenus des vues de g et g−1. L’empreinte SHA-256 d’une paire contrôle sa correspondance avec la référence attendue. L’AEAD authentifie son contenu chiffré avant décompression. La banque parente est authentifiée intégralement. Les objets de g−2 déjà libérables ne font pas partie des dépendances protégées de g.
5. En première reprise, le moteur fait un contrôle intégral **avant le drain**. Une incohérence lisible bloque même la synchronisation de commandes héritées. Après un drain réussi, il recommence le contrôle depuis le support. Une attestation manquante ou déchirée ne peut être réécrite que depuis une copie authentique, à son emplacement de redondance et après le contrôle complet. Les réparations sont stabilisées avant la mutation. La disparition de la tête attendue, l’absence de source authentique ou une divergence critique ferme l’écrivain.
6. L’opération s’appuie sur l’état fraîchement relu. Les mécanismes d’admission P3 et le registre CoW P5 restent responsables des allocations. Les nouvelles unités et leur banque sont préparées hors des deux graphes protégés. F1 précède l’émission des deux attestations. Le succès de F2 précède le remplacement de la tête en mémoire et l’acquittement. Une publication dont le résultat est indéterminé impose un remontage. Une opération courante sans réparation conserve deux synchronisations. [Façade du pool](../src/secure_pool.rs) · [Journal interne](../src/secure_commit.rs) · [Contrat de durabilité](novafs-p1-durabilite.md)

### Caches, erreurs et concurrence

| Élément | Règle appliquée |
| --- | --- |
| Pages propres P4 et index des liens P5.1 | Les moteurs de vue et leurs index ont la durée de l’opération sur la façade P7. Le graphe des préconditions parcourt le backend avant toute réparation. |
| Préfixe P7, preuves des vues, clés déverrouillées | La tête et les banques physiques déterminent l’autorité de la mutation. Les clés sont propres aux domaines déverrouillés. Le répertoire d’identifiants conservé en RAM sert au provisionnement. Les opérations de lecture de contenu et de catalogue contrôlent d’abord l’identité de l’état physique. |
| Attestations | Le HMAC lie pool, emplacement, génération et empreintes. Une copie décodée sans clé ne donne aucun droit d’écriture. Les huit emplacements peuvent contenir des restes de tentatives abandonnées. Seules les générations sélectionnées autorisent la reprise. |
| Publication, remontage, migration ou restauration | Une opération suivante reconstruit sa sélection. Aucun index de validation n’est transféré entre montages ou domaines. La restauration authentifiée continue de créer une maîtresse neuve. |
| E/S défaillante, tête changée ou corruption critique | La prévalidation échouée ferme l’instance aux mutations suivantes. La première erreur est retournée au demandeur. Une nouvelle ouverture et un nouveau contrôle sont nécessaires. Le diagnostic ne soumet aucune écriture ni synchronisation. |

Le fichier anonyme créé par `FileImage::create` appartient au moteur jusqu’à la restitution du descripteur. Une image fournie par `FileImage::from_file` peut avoir des alias inscriptibles inconnus. Elle permet le diagnostic, et les mutations P7 y sont refusées. La CLI continue de transformer une copie temporaire privée et d’émettre l’image résultante. Le provisionnement peut préparer des secrets en RAM, et la restauration peut vérifier une archive dans son fichier temporaire chiffré avant cette section critique. Ces préparatifs n’écrivent aucun bloc du pool de destination. Les bancs qui publient un fichier nommé copient la représentation disque une fois le montage privé fermé. La méthode bas niveau du journal qui acceptait une fonction de vérification arbitraire est désormais interne au paquet. [Backends Darwin et simulé](../lab/device.rs) · [Parcours de la CLI](../src/bin/novafs/secure.rs)

Le simulateur utilise le même verrou pour une mutation et pour le remplacement externe d’un bloc stable. La tentative d’altération au milieu du parcours reçoit `Busy`. Le test de concurrence arrête réellement un écrivain sur une lecture, lance un second écrivain et un lecteur, puis vérifie qu’une session restée à l’ancienne génération refuse de s’en servir après publication. Une copie stable indépendante peut être examinée en parallèle. Les contrôles sur un descripteur externe ouvert en lecture seule décrivent les octets observés. Pour un diagnostic cohérent face à un écrivain étranger, le propriétaire doit fournir un instantané stable.

La correction suppose un backend de confiance, un accès exclusif de bout en bout et des barrières fidèles. Une altération entre appels est détectable par relecture des structures protégées. Un backend incapable d’empêcher une modification entre le contrôle et F2 ne peut pas annoncer l’exclusivité. Un stockage partagé exigerait une réservation imposée par le backend ou une publication conditionnelle atomique. Un flush en échec peut avoir stabilisé des commandes antérieures. Un support qui ment sur F2, une panne matérielle ultérieure ou le retour intégral à une image authentique plus ancienne au remontage échappent à cette garantie. Le format binaire P7, ses identifiants, ses enveloppes, ses nonces, ses exports et les algorithmes cryptographiques sont inchangés.

## Validation de récupération

| Cas | Observation attendue et mesurée |
| --- | --- |
| A · En-tête altéré entre deux appels | La copie redondante de l’attestation manque. L’altération de l’enveloppe de l’en-tête bloque la première mutation, sans drain ni tentative d’écriture. Une nouvelle tentative sur la même instance rend `Unavailable`. |
| B · Banque altérée en mode inscriptible | L’altération du bloc de preuves de la banque de tête interdit la publication. Même génération, même image stable dans son intégralité, zéro tentative d’écriture. |
| C · Attestation et contenu | Une copie survivante autoriserait une réparation. Le corps chiffré altéré provoque le refus avant celle-ci. Sur le témoin sain, une attestation est effectivement restaurée et la transaction acquittée se remonte. |
| D · Dommages combinés | Huit paires de dommages par géométrie couvrent en-tête et attestation, banque et attestation, banque et corps chiffré, racine et unité partagée par snapshot/clone, tag et métadonnée, deux banques, les deux attestations de tête et les deux en-têtes identiquement altérés. Sept E/S de lecture ciblent séparément chaque classe de dépendance. |
| E · Reprise interrompue | Le journal doit restaurer une copie de tête et une copie du parent. Une coupure à chaque événement du backend, avec deux politiques de perte du cache, donne 200 cas par géométrie. Chaque remontage relit et vérifie le fichier selon un oracle avant/après indépendant. Un autre test conserve une écriture volatile antérieure et confirme qu’une corruption détectable empêche même le drain. |
| F · Concurrence | Pendant une mutation, un autre écrivain, le diagnostic d’un lecteur et une altération du support sont refusés. Après publication, une session obsolète ne peut ni confirmer une écriture ni conclure son contrôle. Deux descripteurs indépendants d’un fichier nommé n’obtiennent aucun droit d’écriture P7. Une altération par descripteur externe entre deux diagnostics est détectée sur le fichier Darwin. |

Le banc documentaire a été exécuté deux fois par ISA. Pour chacune des quatre combinaisons ISA × secteur, les quatre cas endommagés conservent tous les octets stables et donnent zéro écriture. Le témoin à une attestation absente répare cette attestation, confirme la nouvelle génération et en vérifie le remontage. Les essais de coupure sur fichier et VM sont les essais historiques du protocole de stockage. Les coupures propres au pool chiffré s’exécutent sur le simulateur. Ces essais ne démontrent pas la persistance physique d’un SSD après perte d’alimentation.

| Qualification exécutée | ARM64 natif | x86_64 Rosetta |
| --- | ---: | ---: |
| Suite standard / toutes fonctionnalités | 170 / 171 | 170 / 171 |
| Cas historiques compris dans ces suites | 163 / 164 | 163 / 164 |
| Différentiel du moteur de contenu | 1 000 000 en 25,535 s | 1 000 000 en 40,729 s |
| Pénuries injectées / remontages du différentiel | 54 643 / 1 000 | 54 643 / 1 000 |
| Coupures historiques du pool P7 / erreurs d’écriture ou de flush | 582 / 70 | 582 / 70 |
| Coupures supplémentaires avec deux attestations à rétablir | 400 | 400 |
| Observations de l’audit, deux répétitions | 20 / 20 conformes | 20 / 20 conformes |
| Coupures historiques sur fichier / VM, avec deux remontages | 24 / 10 | 24 / 10 |

Chaque architecture a exercé les secteurs de 512 et 4 096 octets. Les tests séparés ont exécuté les quatre scénarios ignorés dans la suite standard, dont le million différentiel et l’oracle externe liblz4. La compilation des primitives cryptographiques a également réussi sur les deux cibles bare-metal. Les huit révisions historiques de P0 à P5.1, les échanges et migrations P6, les commandes P7 et la restauration autonome après destruction de la source ont été rejoués. Huit échanges supplémentaires croisent v0.9.9 et v0.9.10, dans les deux sens de version et d’architecture, avec restauration sur l’autre géométrie. [Résultats des suites, injections et observations individuelles](validation/v0.9.10/qualification.json.gz)

## Coût de la revalidation

Deux campagnes par architecture alternent l’ordre v0.9.9/v0.9.10. Chaque processus travaille sur une image temporaire neuve. Les mêmes graines, les deux tailles de secteur, les tailles de fichiers et la séquence des opérations sont rejouées sur les deux exécutables. Les séries comprennent un volume clair brut, un volume chiffré brut et des volumes chiffrés LZ4 à données répétitives ou pseudo-aléatoires. Les comparaisons ci-dessous concernent le clone chiffré LZ4 à données répétitives, avec 128 écritures partielles de 8 Kio et 128 lectures contrôlées de 16 Kio par ISA et version. Les percentiles sont les rangs supérieurs, sans interpolation. Le cache macOS reste actif. Les processus extérieurs et les latences de l’hôte ne sont pas isolés.

| ISA | Version | Écriture p50 / p95 / p99 / max en ms | Lecture / écriture en Mio/s | Lectures de blocs par écriture | F1 + F2 par écriture en ms |
| --- | --- | --- | ---: | ---: | ---: |
| ARM64 natif | 0.9.9 | 22,63 / 25,89 / 28,77 / 37,06 | 3,196 / 0,343 | 1177,2 | 9,05 |
| ARM64 natif | 0.9.10 | 34,60 / 37,09 / 38,10 / 38,80 | 3,001 / 0,229 | 2225,8 | 8,92 |
| Rosetta | 0.9.9 | 29,87 / 31,18 / 33,23 / 33,24 | 2,106 / 0,266 | 1177,2 | 8,69 |
| Rosetta | 0.9.10 | 47,89 / 50,73 / 67,14 / 170,90 | 1,936 / 0,162 | 2225,8 | 10,00 |

**Le p99 d’écriture régresse sur les deux architectures dans la comparaison appariée.** Les anciens points isolés de 34,84 et 69,47 ms ne constituent pas un témoin simultané. La pointe Rosetta atteint 170,90 ms sur la v0.9.10. Les événements individuels ne permettent pas d’attribuer toute la pointe à une seule cause. L’amplification d’écriture reste de 13,5× avec deux flushs par écriture sur les deux versions.

Les sondes détaillées tournent dans des processus distincts des mesures de latence. La durée du parcours mesure la construction de l’index du graphe, hors montage interne des systèmes de fichiers qui suit. Le CPU AEAD porte sur les appels d’ouverture et de scellement des unités, tirage du nonce compris au scellement. Ces activités peuvent s’imbriquer. Les lectures physiques comptent les appels de bloc au backend, indépendamment des accès effectivement servis par le SSD.

| ISA | Version | Parcours du graphe par écriture | Parcours en ms murales | Ouvertures + scellements par écriture | CPU AEAD en ms par écriture |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM64 natif | 0.9.9 | 1 | 0,84 | 535,3 | 2,25 |
| ARM64 natif | 0.9.10 | 2 | 1,68 | 994,7 | 4,17 |
| Rosetta | 0.9.9 | 1 | 1,51 | 535,3 | 4,06 |
| Rosetta | 0.9.10 | 2 | 3,00 | 994,7 | 7,52 |

Le moteur vérifie l’état récupérable avec un adaptateur en lecture seule, puis l’instance de mutation reconstruit son propre index. Cette séparation ajoute un parcours complet. Les appels de bloc sur le chemin d’écriture passent en moyenne de 1 177 à 2 226. Une optimisation ultérieure pourrait transférer un index vérifié à l’intérieur de la même section critique. Elle devrait préserver le contrôle indépendant avant la première réparation, le changement d’époque après drain et les 40 observations de non-régression.

| ISA | Pic de travail v0.9.9 / v0.9.10 en Mio | Pic au déverrouillage Argon2id v0.9.9 / v0.9.10 en Mio |
| --- | ---: | ---: |
| ARM64 natif | 2,198 / 2,198 | 66,062 / 66,062 |
| Rosetta | 2,217 / 2,217 | 66,050 / 66,050 |

Les pics additionnent le tas vivant mesuré et toute la pile réservée du thread de travail. Le plafond de 7 Mio est respecté sur les profils mesurés hors opérations Argon2id. Les 80 processus du banc retrouvent leur niveau de tas initial. Les distributions incluent les temps de montage, de vérification, d’export et de restauration. Le RSS de l’hôte, comprenant Rosetta le cas échéant, figure par processus. Les 5 888 appels chronométrés sont conservés individuellement. [Distributions, mémoire, échanges et empreintes des exécutables](validation/v0.9.10/comparaison.json.gz) · [Mesures individuelles](validation/v0.9.10/appels.jsonl.gz)

## Commandes de reproduction de v0.9.10

Depuis la racine du monorepo sur un Mac Apple Silicon équipé de Rosetta, avec Rust stable et ses deux cibles Darwin, le `Cargo.lock`, liblz4 et QEMU disponibles localement. Les programmes de qualification historique ont également besoin des commits Git référencés. Les résultats du banc comparatif sont enregistrés dans son répertoire de sortie avec les empreintes des exécutables.

```sh
(
  set -eu
  out="$PWD/novaFS/target/qualification-reprise"
  mkdir -p "$out"
  for isa in aarch64 x86_64; do
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run test --offline --test p7_secure -- --nocapture --test-threads=1
    python3 novaFS/lab/p6-validation.py "$isa" --sortie "$out/suites-$isa.json"
    python3 novaFS/lab/p7-cli.py "$isa"
  done
  python3 novaFS/lab/p7-no-std.py
  python3 novaFS/lab/p6-lz4-interop.py > "$out/lz4.json"
  python3 novaFS/lab/p6-interop.py > "$out/p6.json"
  python3 novaFS/lab/p6-historiques.py --support formats > "$out/formats.json"
  python3 novaFS/lab/p6-historiques.py --support pannes > "$out/pannes.json"
  python3 novaFS/lab/p7-validation.py interop --sortie "$out"
  for repetition in 1 2; do
    awk '/^```sh$/{p=1;next} p && /^```$/{exit} p' \
      novaFS/docs/validation/p7.1-audit.md > "$out/audit.sh"
    sh "$out/audit.sh" > "$out/observations-$repetition.txt"
  done
  python3 novaFS/lab/p71-comparaison.py --sortie "$out/comparaison"
)
```


<details>
<summary>Premier audit sur v0.9.9 et décision d’arrêt initiale</summary>

**Premier audit au commit `6d50b5d`.** La publication de v0.9.10 a été suspendue à ce stade. Sur le moteur v0.9.9, une altération du support entre deux appels peut conduire à la réparation d’une attestation, puis à l’acquittement d’une transaction alors que le pool ne peut plus être rouvert. Le cas se reproduit sur ARM64 natif et x86_64 sous Rosetta, avec des secteurs de 512 et 4 096 octets. Les conditions d’arrêt concernent la réutilisation d’une validation périmée et l’ordre des contrôles avant réparation. [Banc reproductible et observations individuelles](validation/p7.1-audit.md)

## Reproduction sur la base P7

Le point de départ était `main`, propre, au commit `6ed50784ae73666caa15846ff380e5c7824faa36`. Le banc lie directement la bibliothèque de ce commit. Il crée un pool de 1 800 blocs de 4 Kio, un volume chiffré et un fichier de 8 Kio. Le montage déverrouille le domaine avec sa clé de récupération et exécute le contrôle intégral avant l’injection. Le simulateur modifie un seul bit du support stable, sans commande en vol. L’instance reste montée, puis tente une écriture partielle du fichier. Le test de reprise commence avec une seule copie d’attestation intacte. Le test en écriture courante conserve les deux copies.

L’oracle ouvre une copie indépendante avec les clés et appelle `check` avant et après la mutation. La sonde compte chaque tentative d’écriture, y compris une tentative qui pourrait échouer. Elle compare aussi tous les octets stables entre l’injection et la fin de l’opération. Le montage frais employé ici autorise la reprise avec une attestation survivante. Le vérificateur hors ligne de redondance a un contrat plus strict. [Contrat des vérificateurs P7](novafs-p7-verification.md)

| État et dommage | Contrôle frais avant | Résultat du montage actif | Écritures / flushs | Image stable identique | Contrôle frais après |
| --- | --- | --- | ---: | --- | --- |
| Reprise, une attestation absente, aucun autre dommage | Accepté | Répare puis publie g4 → g5 | 28 / 4 | Non | Accepté |
| Reprise, même attestation absente, bit altéré dans le second en-tête | Refusé | **Répare puis acquitte g4 → g5** | **28 / 4** | **Non** | **Refusé** |
| Reprise, attestation absente, bit altéré dans la banque de tête | Refusé | Refuse, reste à g4 | 0 / 1 | Oui | Refusé |
| Montage déjà inscriptible, bit altéré dans la banque de tête | Refusé | **Acquitte g5 → g6** | **27 / 2** | **Non** | **Refusé** |
| Reprise, attestation absente, bit altéré dans un corps chiffré référencé | Refusé | Refuse, reste à g4 | 0 / 1 | Oui | Refusé |

Chaque ligne a donné le même résultat dans les quatre combinaisons architecture × secteur. Sur 20 observations, 12 satisfont l’oracle et 8 mettent en évidence les deux défauts. Dans la deuxième ligne, le bit altéré se situe à l’octet 80 de la copie d’en-tête du bloc 1, dans la maîtresse administrative enveloppée. La première écriture vise le bloc 3 et le test vérifie que les octets de l’attestation réparée sont exactement ceux de la copie d’origine à cet emplacement. Dans la quatrième, la nouvelle tête dépend de la banque g5 devenue illisible. Le test établit un acquittement suivi d’un refus de remontage. Il ne mesure pas une perte physique de toutes les données du fichier.

## Chemin critique observé

Le contrôle initial des en-têtes compare les deux blocs canoniques et ouvre l’enveloppe administrative. Ensuite `SecurePool` conserve sa sous-clé de publication, son journal et son `State`. Au début de `change`, `State::signed_views` vérifie les HMAC des vues à partir de ce préfixe en mémoire. Son succès renseigne sur ces octets. Il ne détecte pas à lui seul un bit qui vient de changer sur le support. [Montage, contrôle et mutation](../src/secure_pool.rs)

* **En reprise**, `CommitLog::resume` draine les écritures, relit les huit emplacements d’attestations, authentifie la filiation et fait relire les deux banques. Le rappel contrôle les vues et le graphe sélectionnés. Avec l’en-tête altéré, il y a **0 lecture d’en-tête, 8 d’attestation, 18 de banque et 430 de blocs d’unités** avant la réparation. Le volume de lectures du graphe n’apporte aucune observation de l’en-tête endommagé. [Reprise du journal](../src/secure_commit.rs)
* **En mode inscriptible**, `resume` rend immédiatement la main. La reconstruction du graphe utilise le préfixe de l’instance. Avec la banque altérée, il y a **0 lecture d’en-tête, d’attestation ou de banque et 347 lectures de blocs d’unités** avant la première écriture. F1 et F2 réussissent, puis le nouveau montage rejette la banque du prédécesseur. [Chemin de mutation](../src/secure_pool.rs)
* **Témoins de refus**. La banque altérée en reprise est détectée après les 8 lectures d’attestations et les 18 lectures de banques. Le corps chiffré altéré entraîne en outre 27 lectures de blocs d’unités. Dans les deux cas, le drain initial se termine sur un cache volatil vide, aucune écriture n’est tentée et tous les octets stables sont conservés. [Traces par scénario](validation/p7.1-audit.md)

Les nombres de lectures portent sur les appels au périphérique simulé. Ils incluent les relectures d’un même bloc et ne mesurent ni des nœuds uniques ni des accès physiques à un SSD. Le banc couvre une altération **entre** opérations. Un autre écrivain qui modifierait l’image pendant le contrôle ou entre sa dernière lecture et l’écriture sort du contrat d’exclusivité de `Device`. L’exclusivité applicative ne détecte pas une altération latente du support. [Contrat du périphérique](../src/lib.rs) · [Hypothèses de persistance P1](novafs-p1-durabilite.md)

## Arbitrage proposé

**A · Recommandé.** Traiter d’abord la revalidation à la frontière de chaque mutation. Après le drain requis en reprise et avant la première réparation ou écriture CoW, relire les deux en-têtes, vérifier leur concordance canonique et leur identité avec l’enveloppe authentifiée à l’ouverture. Authentifier à nouveau les attestations sélectionnées, les deux banques protégées et leur filiation, puis comparer la tête à celle attendue par l’instance. Vérifier les preuves des domaines, les références et les contenus sur cet état physique. En cas de divergence ou d’E/S en échec, refuser la mutation et exiger une nouvelle inspection. Le critère d’acceptation pour les quatre cas endommagés est le refus avec zéro tentative d’écriture et la conservation intégrale de l’image stable. Le témoin sain doit encore réparer et publier.

N’étudier ensuite que les parcours redondants **à l’intérieur** d’une opération portant sur ce support exclusif. Un résultat temporaire doit être attaché au support, au domaine, à la génération, aux racines et aux empreintes vérifiées. Il expire en cas de publication, changement de graphe, corruption, erreur d’E/S, remontage, restauration, migration ou modification externe. Le chemin de réparation doit rester capable de recommencer le parcours complet. Une empreinte gardée en RAM, une date de fichier et une attestation simplement décodée ne prouvent pas que les octets courants sont ceux déjà vérifiés. [Cache P4](novafs-p4-cache.md) · [Registre de partage P5](novafs-p5-partage.md)

**B · Si le support autorise des modifications concurrentes hors du service.** Définir d’abord un backend capable de stabiliser la vue pendant l’ensemble contrôle et écriture, ou de refuser atomiquement une publication si cette vue a changé. À défaut de cette garantie, limiter ce mode à l’inspection sur une copie stable. Une modification indépendante à n’importe quel instant ne peut pas être exclue par deux lectures successives. A couvre le modèle au repos de cette reproduction. B élargirait le contrat du backend et nécessite une décision distincte.

Le maintien d’un graphe validé entre transactions, la déduction de sa validité à partir de la seule génération et la suppression du contrôle avant réparation sont écartés. Aucune optimisation du moteur n’a été appliquée. Le format P7, les contrats historiques de lecture et les algorithmes restent ceux de la base auditée. [Format et garanties P7](novafs-p7-chiffrement.md)

## Fraîcheur et renouvellement des clés

Une copie ancienne intégralement authentique peut passer tous les contrôles locaux. Un futur témoin devrait conserver un état monotone protégé contre le clonage, lier identité du pool et du domaine, génération et digest de tête, et authentifier cet état attendu par un canal indépendant. La coordination entre publication locale et avancée du témoin devra définir le traitement des interruptions. Une restauration légitime devrait ouvrir une nouvelle époque explicitement autorisée et conserver la trace de l’ancienne. Si le témoin est requis et indisponible, suspendre les écritures qui promettent la fraîcheur. Une inspection en lecture seule peut rendre un résultat d’intégrité avec fraîcheur inconnue.

Le plafond opérationnel de 2³² scellements par clé effective cumule toutes les copies, y compris les tentatives interrompues. Pour renouveler les clés de données, figer une vue, produire et vérifier son export autonome, la restaurer dans un pool neuf qui génère un domaine et une maîtresse neufs, puis vérifier le contenu, le reçu de la vue et le moyen de récupération de la destination. Répéter pour chaque vue à conserver et retirer les anciennes autorisations d’écriture. Changer uniquement la phrase de déverrouillage conserve la maîtresse. Les sauvegardes existantes conservent leurs propres exigences de protection. [Politique de nonces et de restauration](novafs-p7-chiffrement.md)

## Performances et périmètre de validation

Pour le clone chiffré LZ4 à contenu répétitif, les valeurs ci-dessous proviennent des observations archivées de **v0.9.9**. Chaque ligne regroupe 128 écritures partielles de 8 Kio. La colonne des synchronisations est la moyenne de F1 et F2 **additionnées** par appel. [Distributions P7](validation/v0.9.9/p7-distributions.json) · [Mesures individuelles P7](validation/v0.9.9/p7-mesures.jsonl.gz)

| Architecture | p50 / p95 / p99 / max en ms | Écriture Mio/s | Lecture Mio/s | F1 + F2 en ms | Amplification |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM64 natif | 22,16 / 25,83 / 34,84 / 37,98 | 0,348 | 3,102 | 9,07 | 13,5× |
| x86_64 Rosetta | 30,02 / 44,04 / 69,47 / 86,08 | 0,247 | 1,833 | 9,33 | 13,5× |

Les 12,34 ms ARM64 et 12,28 ms Rosetta de la comparaison initiale portent sur le format P6. Cette comparaison change plusieurs chemins à la fois et ne permet pas d’attribuer la hausse du p99 à l’AEAD ou au seul contrôle avant réparation.

Les pics historiques tas + pile hors dérivation étaient de 2,26 et 2,28 Mio sur les charges de référence. Les pics de déverrouillage Argon2id étaient respectivement de 66,06 et 66,05 Mio. Le point d’arrêt intervient avant la campagne comparative P7/P7.1. Aucun gain de p99, de débit ou de mémoire n’a été mesuré pour un nouveau moteur. Les temps CPU des phases individuelles, allocations transitoires, accès au cache, réservations et comptages de nœuds restent à instrumenter après arbitrage. [Rapport de mesures P7](novafs-p7-mesures.md)

## Suites historiques rejouées sur la bibliothèque auditée

Mac Apple Silicon, macOS 27.2 (26B123), Rust stable 1.99.1, profil release, dépendances du verrou Cargo en mode hors ligne. Chaque suite a tourné avec un thread de test Cargo. Le scénario différentiel utilise ses propres travailleurs. Les durées du différentiel correspondent à cette exécution fonctionnelle.

| Vérification | ARM64 natif | x86_64 Rosetta |
| --- | ---: | ---: |
| Tests standard réussis | 163 | 163 |
| Tests avec toutes les fonctionnalités d’instrumentation réussis | 164 | 164 |
| Opérations différentielles du moteur de contenu | 1 000 000 en 25,124 s | 1 000 000 en 42,301 s |
| Pénuries injectées / remontages dans le différentiel | 54 643 / 1 000 | 54 643 / 1 000 |
| Coupures P7 / erreurs d’écriture ou de synchronisation, par exécution de la suite | 582 / 70 | 582 / 70 |
| Nouveau banc de récupération, invariants satisfaits / observations | **6 / 10** | **6 / 10** |

Les suites Cargo se recouvrent et chacune ignore quatre tests longs. Le million différentiel a été lancé séparément. Le scénario historique de copie divergente, avec nonces distincts observés dans les deux branches, et le scénario historique de double dommage ont tous deux réussi sur chaque ISA. Les nombres de pannes correspondent à 291 coupures et 35 erreurs par géométrie. Le million différentiel porte sur le moteur de contenu sous-jacent. Le banc documentaire porte sur des pools chiffrés créés par v0.9.9 et des défauts injectés en mémoire. Les campagnes externes d’échange de toutes les images historiques, les pannes sur VM et la qualification complète d’un candidat P7.1 restent en attente de l’arbitrage. [Tests du pool chiffré](../tests/p7_secure.rs) · [Différentiel historique](../tests/properties.rs)

Depuis la racine du dépôt, les commandes suivantes rejouent les suites de référence. L’annexe fournit le programme, les commandes et l’oracle du défaut bloquant. [Reproduire le défaut sur les deux architectures](validation/p7.1-audit.md)

```sh
(
  set -eu
  for isa in aarch64 x86_64; do
    export NOVAFS_TARGET="$isa-apple-darwin"
    novaFS/lab/run test --offline -- --test-threads=1 --nocapture
    novaFS/lab/run test --offline --all-features -- --test-threads=1 --nocapture
    novaFS/lab/run test --offline --test properties one_million_model_operations \
      -- --ignored --exact --nocapture
  done
)
```

La version Cargo reste v0.9.9. Les licences vérifiées sont MPL-2.0 pour le paquet NovaFS et MIT pour NovaOS. [Manifeste NovaFS](../Cargo.toml) · [Licence NovaFS](../LICENSE) · [Licence NovaOS](../../novaOS/LICENSE)

</details>
