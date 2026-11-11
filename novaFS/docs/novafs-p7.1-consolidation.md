# NovaFS P7.1 · Audit de la reprise et des métadonnées en mémoire

**Décision du 11 novembre 2026. La publication de v0.9.10 est suspendue.** Sur le moteur v0.9.9, une altération du support entre deux appels peut conduire à la réparation d’une attestation, puis à l’acquittement d’une transaction alors que le pool ne peut plus être rouvert. Le cas se reproduit sur ARM64 natif et x86_64 sous Rosetta, avec des secteurs de 512 et 4 096 octets. Les conditions d’arrêt concernent la réutilisation d’une validation périmée et l’ordre des contrôles avant réparation. [Banc reproductible et observations individuelles](validation/p7.1-audit.md)

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

La version Cargo reste v0.9.9. Les licences vérifiées sont MPL-2.0 pour le paquet NovaFS et MIT pour NovaOS. [Manifeste NovaFS](../Cargo.toml) · [Licence NovaFS](../LICENSE) · [Licence NovaOS](../../LICENSE)
