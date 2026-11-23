# NovaAuth R2 — Revalidation EXEC : conception et qualifications indépendantes

23 novembre 2026. Base : `91d0da7766e56b2e60481d9768ce280952ec2022`.
Banc validé : `e1718681d07a09cad525d6f832a878a155f9ca98`.
Branche : `codex/novaauth-administration-0240-phase1`.

## Contrat soumis et prototype préservé

Le [contrat de revalidation sélective](novaauth-r2-revalidation-exec-contrat-v0.24.0.md) est soumis à arbitrage **avant implémentation**. Il propose un descripteur immuable des dépendances, une revalidation liée à l'admission durable et un inventaire privé couvrant aussi les tickets Dormant et les lecteurs préparés. Il conserve la tête d'émission, les contrôles d'instance/session/époque/image, les barrières de drainage et les trois acquittements.

La représentation par références réutilisées vise à éviter de dupliquer les provenances dans chaque ticket. Elle doit encore démontrer le layout, les fragments de 128 octets, l'inventaire sans course et le respect des 4 096 octets. La borne proposée de quatre dépendances n'autorise aucun droit supplémentaire. Aucun ticket G ne peut être sauvé par une substitution implicite de provenance H.

Les dix fichiers du prototype non qualifié sont **inchangés, non indexés et non activés**. Leurs SHA-256 avant/après sont archivés. Les octets du prototype ne figurent pas dans les ISO de ces essais. Les deux lacunes principales du prototype restent la provenance EXEC réelle — distincte de la simple lecture du paquet — et la représentation des dépendances multiples.

## Identité des sources et applicabilité

| Élément effectivement exécuté | Référence |
|---|---|
| Noyaux et services QEMU déjà construits | `8cc96199fd1293ab7294e38d48d04d3e4cdafbf0` ; arbre `6722a86eddfa0e27339bd9dbd269b39dd4176d6f`, identique au code publié `e0b246cee517067670fc6d17f7893e2e6269578f` |
| ELF de sonde concurrente | `e63d3f237ae932558f67adcee42a41b2144735e7` ; arbre `5750e97997fd6fe40062a3394cda266201600dc8`, identique au code publié `6e589b45f920505d76c552a716e84409fe1d40a1` |
| Nouveau banc Q4 et tests hôtes | `e1718681d07a09cad525d6f832a878a155f9ca98` ; empreintes identiques aux huit essais réussis |
| Base documentaire et preuves antérieures | `91d0da7766e56b2e60481d9768ce280952ec2022` |

Vingt artefacts privés (ISO, noyaux, registres, shells, sondes, paquets et genèses) ont été relus et comparés aux SHA des exécutions. Depuis les sources invitées ci-dessus, les changements publiés concernent la documentation et le banc hôte : **aucune modification des composants invités, noyaux ordinaires, ABI ou format P9**. Les preuves précédentes restent applicables à leurs périmètres précis, sans devenir une qualification de la revalidation EXEC ni une qualification globale de R2.

L'archive précédente de 1 111 preuves reste à son emplacement, avec l'empreinte globale inchangée `0cd8f95d16fce1039c713d171ca6d76dfca183d80f42094db10385d8b63a9964`. Cette vérification d'identité de l'archive ne prétend pas être une nouvelle exécution des tests qu'elle contient.

## Matrice des nouveaux essais invités

| Profil | Q2/Q3 croisés : G en vol et deux confirmations | Q4 : coupure après RESERVE, reprise froide refusée |
|---|---|---|
| x86_64/P9-512 | Réussi, `q23-croise-03` | Réussi, `q4-reserve-02` ; premier essai non concluant conservé |
| x86_64/P9-4096 | Réussi, `q23-croise-03` | Réussi, `q4-reserve-02` |
| AArch64/P9-512 | Réussi, `q23-croise-03` | Réussi, `q4-reserve-02` |
| AArch64/P9-4096 | Réussi, `q23-croise-03` | Réussi, `q4-reserve-02` |

Chaque nom complet commence par `r2-commandes-<architecture>-<secteur>-retrait-selectif-vide-`. La [matrice machine](validation/novaauth-r2-revalidation-exec-v0.24.0/resultats.json) contient les commandes exactes, les empreintes, les états, les positions durables et les chemins des preuves.

**Q2/Q3 croisés.** Une E/S de G a réellement eu lieu et possède une admission durable dont l'issue n'a pas encore été soumise. Deux confirmations concurrentes portent ensuite sur le **même** retrait : préparations natives 0/0, confirmations 0/5, une seule mutation publiée. Sur les quatre profils : admission G position 8, réservation 13, issue individuelle 14, publication 15. Les classes 1, 2 et 4 sont acquittées distinctement, puis le retrait devient RETIREE. Les droits VFS Direct/H sont conservés, les anciennes et nouvelles utilisations de G refusées. Les ressources des sessions et le drainage global reviennent à zéro.

Ce croisement complète les précédents essais séparés « admission G en vol » et « double confirmation ». Il ne démontre pas une nouvelle admission simultanée au point de retrait ni deux mutations distinctes. Les trois ordres avant réservation / entre réservation et publication / après publication ont été complétés dans les **tests de l'autorité hôte**, sans les attribuer aux invités.

**Q4 négatif.** L'invité A est arrêté par QMP après la réservation durable, avant livraison de sa réponse, publication P9 et ACK. Toutes les admissions VFS précédentes sont déjà terminales dans cette recette. Le même état d'autorité est conservé ; le serveur redémarre depuis ses disques avec une autre incarnation. L'administrateur indépendant donne une nouvelle approbation explicite, mais l'autorité refuse le drainage incomplet : `UNCERTAIN : drainage sélectif incomplet, nouvelle incarnation inactive`. La demande de reprise reçoit le statut RPC natif **5** au contrôle « drainage EXEC non confirmé ». Le registre invité passe effectivement en quarantaine ; ni session authentifiée, ni triplet activé, ni nouveau reçu terminal ne sont observés.

La réservation, la tête, les trois ACK absents et le nombre de publications (**zéro**) restent identiques. L'ancienne route demeure RETRAIT_EN_COURS. Ce succès est celui d'un **refus sûr après redémarrage**, pas celui d'une récupération complète de disponibilité. Il ne remplace pas la recette VFS après récupération 2-sur-2 complète.

## Premier échec Q4 et correction du banc

Le premier x86_64/P9-512, `q4-reserve-01`, a échoué : le mandat administratif à usage unique avait déjà été consommé par A. B était donc refusée avec « approbation hors mandat unique » avant le contrôle de drainage recherché. Le collecteur attendait aussi `NAR2 registre termine`, alors que ce profil passe par `NARV registre quarantaine generation non activable` et le retrait supervisé de sa famille.

Correction limitée au banc : préparer une nouvelle approbation explicite par le mécanisme de laboratoire existant, vérifier la raison exacte de refus distant, la quarantaine invitée et la restitution supervisée réelle. Les tests négatifs refusent une expiration, un autre statut, une absence de demande ou d'événement, une demande dupliquée et toute activation/déchiffrement. L'oracle n'accepte pas un refus quelconque. Les traces du premier échec sont conservées, sans réécriture.

Aucun délai, capacité, contrôle de sécurité ou code invité n'a été modifié. La seconde exécution vérifie cette hypothèse précise ; les trois autres profils ne sont lancés qu'après sa réussite.

## Tests hôtes et ABI

- **27/27 tests hôtes ciblés**, dont sept nouveaux : conservation des issues individuelles à travers le redémarrage de l'autorité, trois ordres admission/publication, blocage de la reprise pour réservation/publication/ACK partiel, et oracles Q4. Les tests existants des modes de commandes et événements restent inchangés.
- **9/9 contrôles ABI**. Les 48 sélecteurs et leurs contrats historiques sont inchangés.
- Aucun nouveau test natif ARM64/Rosetta des effecteurs, ni rejeu des quatre ELF historiques ou des 52 parcours : le code invité et les artefacts restent identiques. Les qualifications antérieures ne sont pas étendues à des chemins nouveaux par cette seule identité.

## Mémoire, IPC et drainage

Le sous-total statique reste **3 994/4 096 octets**, soit 102 octets de marge. C'est la mesure native précédente applicable aux sources inchangées ; le layout du mécanisme de revalidation proposé n'est ni compilé ni mesuré.

| Profil | RAM résidente / retenue, cadres observés | Minimum cadres libres observé | TLS initial invité Q2/Q3 |
|---|---:|---:|---:|
| x86_64/P9-512 | 3 044 / 3 044 | 59 207 | 2 976 ms |
| x86_64/P9-4096 | 3 044 / 3 044 | 59 207 | 2 984 ms |
| AArch64/P9-512 | 2 992 / 3 461 | 58 690 | 3 304 ms |
| AArch64/P9-4096 | 2 992 / 3 461 | 58 690 | 2 856 ms |

Il s'agit des extrema des points instrumentés, pas d'un échantillonnage continu de chaque allocation. L'échéance TLS demeure 10 000 ms. Les durées ci-dessus soustraient uniquement des horodatages monotones invités du même domaine.

Q2/Q3 : maximum observé **quatre racines par acteur**, cinquième demande LIMIT sans allocation résiduelle, registre 4 racines/7 handles/0 attente, signataire 4/6/0. Pages résidentes du registre : 156 x86_64, 152 AArch64 ; signataire : 69 et 61 ; pile : 64 pages chacun. Le retour des compteurs temporaires et le drainage final (endpoints, nœuds, espaces et cadres IPC à zéro) sont vérifiés.

Q4 : la coupure de A détruit son invité ; elle ne prouve pas un nettoyage interne préalable de ses ressources. Dans B, le registre retrouve ses compteurs avant/après, puis le registre et le signataire sont retirés par le superviseur (statut **125**, distinct du refus RPC 5). Pour chacun : racines, handles, attentes, pages, pile et threads à zéro après collecte. Le signataire n'a **pas** un retour TLS normal avec marqueur « après » : le retrait supervisé est documenté séparément. Les services indépendants restant vivants conservent leurs ressources ; aucun drainage global zéro de B n'est revendiqué. Plafond observé de quatre racines, relais hôte drainé.

## Limites et travaux restants

| Parcours | État réel et suite |
|---|---|
| Tickets EXEC indépendants après changement de tête | Contrat prêt pour arbitrage ; aucune implémentation ni qualification nouvelle |
| Tickets réellement dépendants de G, provenances multiples, pipes | Prototype insuffisant ; liaison authentifiée et inventaire à terminer après validation |
| Q2 E/S G en vol, issue et drainage | Qualification complétée par le croisement Q3 sur quatre profils |
| Q2 écriture PREPARE annulée | Annulation locale couverte antérieurement en natif ; association admission durable/issue individuelle manquante dans le parcours invité. `Selective::close` conserve UNCERTAIN si `cancelled_prepare` est présent, sans ACK inventé |
| Q2 perte ADMIT avant GO | Chemin à qualifier séparément ; l'analyse statique de `Io::reconcile` phase 2 impose de vérifier aussi le nettoyage de la requête VFS en attente, pas seulement l'issue distante. Aucun nouveau résultat invité revendiqué |
| Q3 nouvelle admission contre retrait / deux mutations distinctes | Tests d'autorité complétés ; qualification invitée supplémentaire encore nécessaire |
| Q4 réservation interrompue | Refus sûr qualifié sur quatre profils ; aucune reconstruction terminale fictive |
| Q4 retour à disponibilité après retrait incomplet | Non démontré ; les anciennes instances n'ont pas fourni leurs ACK. La neutralisation d'effecteurs disparus n'est pas autorisée par le contrat existant et demanderait une procédure distincte |

Les réserves instrumentées AArch64 de **301 cadres** et de reproductibilité entre environnements restent ouvertes. La revalidation proposée n'est pas activée ; R2 n'est pas clôturé. Aucun développement d'utilitaire, fusion, tag ou activation ordinaire.

## Durées, coordination et archives

Début : 2026-11-23T17:40:37+00:00. Arrêté de la matrice : 2026-11-23T18:11:26.052930+00:00.
Attente du verrou : **0.003106 s** ; exécution des neuf campagnes : **307.103 s** ; travail hors fenêtres de banc : **1541.947 s**.
Les travaux hôtes/conception menés pendant QEMU chevauchent ses fenêtres ; le dernier chiffre ne mesure pas tout le temps de diagnostic. Les neuf acquisitions et libérations sont détaillées dans la matrice, tous les verrous propres sont libérés. Aucun conflit de planification ni verrou forcé. Aucun mélange avec les horloges invitées. Le budget de 90 minutes actives est respecté ; la publication documentaire suit cet arrêté.

Archive : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/revalidation-selective-91d0da7-20261123.tar.gz`. **443 fichiers de preuve**, plus le manifeste, relus intégralement.

SHA-256 archive : `bbe447ef298aa8de75b21492319bb293f095b4b787038c9f7ebd264ffb8c4053`.

SHA-256 manifeste : `ce001a3c3817b7b051935bac285da871a41b7d6525a19c906927caadd7f66bd4`.

[Métadonnées](validation/novaauth-r2-revalidation-exec-v0.24.0/archive.json) · [Manifeste](validation/novaauth-r2-revalidation-exec-v0.24.0/MANIFEST.sha256) · [Identité des sources et binaires](validation/novaauth-r2-revalidation-exec-v0.24.0/applicabilite.json).

La recherche bornée des clés privées connues de ces essais et des marqueurs PEM ne trouve aucune occurrence. Les secrets, ISO et ELF restent dans leurs répertoires privés durables ; seuls leurs SHA figurent dans les preuves publiées. Ce contrôle ne prétend pas détecter tout secret inconnu. Les branches parallèles, leurs worktrees et le worktree principal sont préservés.
