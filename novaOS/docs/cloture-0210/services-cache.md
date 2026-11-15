# Services, cache, compatibilité disque et binaire

## Lecture des preuves

Cette annexe porte sur l’arbre du candidat `4a3d3079bad8a8ef074601df5afdf9c6d5faacfb`, intégré par `4569a6876720f6b4efa901db707b670e415abeeb`. Les essais P3 et les essais du retrait du serveur précèdent cette fusion. Les essais P4, dont les quatre anciens ELF, proviennent du candidat et de ses profils de sonde. Pour cette revue documentaire les fichiers archivés ont été relus et leurs empreintes recalculées.

Chaque ligne renvoie à une archive de la table suivante. L’empreinte s’applique à l’archive entière. Son index interne permet de vérifier séparément les journaux et résultats. Les comptes excluent l’index lui-même.

| Référence | Archive et pièces vérifiées | SHA-256 |
|---|---|---|
| S | [Supervision P3][S], 325 pièces | `b2981cc5ae0883d8ce2704d16660bab977717a4a9fef187f6ab48f6c4bf422b0` |
| R | [Régressions CLI P3][R], 177 pièces | `fa41f299037a1dd03642a7233df252aa0ca4681804ece201778934a678f36a5c` |
| C | [Qualification intégrée P4][C], 510 pièces | `6034653cb7c2182cb3a6010db909e1ed9ed1f0086b04510f5ccc12f0051b3bf2` |
| E | [ELF originaux et provenance][E], 26 pièces | `b336de3a5bd66c673e812392a68cd1604b6cfb82c204ad2b4dd049e1aac4ceec` |
| T | [Retrait du service et compteurs d’exécution][T], 32 pièces | `db9bad7ea89b91aab6f46be312d284787c46aa06c96535d582a6bc260525c063` |
| D | [Disques P8/P9 et arrêts du véritable écrivain][D], 392 pièces | `9f48df6ddac87dc09401c3bbc2ade5edea65fd5247fc543cb59ad6d8c99a0b0e` |

Les invités utilisent QEMU 11.1.3 TCG, Cortex-A72 AArch64 ou Q35 x86_64, VirtIO-blk et des images brutes jetables. « 12 cellules » couvre les deux architectures, 1/2/4 vCPU et des secteurs de 512/4 096 octets. « Hôtes » désigne les exécutables Darwin ARM64 natif et x86_64 sous Rosetta. Le lecteur indépendant remonte les images des invités sur l’hôte. La synthèse distingue les tests de politique, les démarrages invités et les injections du stockage. [Résultats par cellule][M]

## Phase 3 — Gestionnaire de services

| ID | Exigence, contrat et code | Test et observation | Architectures et preuve | Verdict |
|---|---|---|---|---|
| S3-01 | Gestionnaire ELF en espace utilisateur, privilèges du lanceur et du gestionnaire distincts. [Architecture][P3] · [Gestionnaire][G] · [Noyau][K] | Démarrage P9 ordinaire, trois services admis, requête INFO de nsh au service réel 9 dans la session P9. Port de consultation incapable de commander START. [Banc invité][J] | 12 cellules S, 10 démarrages de supervision complémentaires dans C | Validé |
| S3-02 | Déclarations versionnées et rôles d’administration. [Politique][P] · [Catalogue][CAT] | Enregistrement manuel d’une entrée permise, refus d’un ID inconnu, contrôles du catalogue embarqué. La capacité est bornée à douze entrées. [Tests de politique][POL] · [Client][J] | 12 cellules S, 13 tests sur chaque hôte dans C | Validé |
| S3-03 | Dépendances obligatoires et optionnelles. [Politique][P] · [Moteur][MP] | Rejet des cycles et fournisseurs obligatoires absents, consultation de deux types d’arêtes. [Tests de politique][POL] · [Client][J] | 12 cellules S et deux hôtes | Validé |
| S3-04 | Démarrage ordonné. [Politique][P] · [Moteur][MP] | Trois lancements indépendants avant le premier READY. Le dépendant du fournisseur en attente conserve un PID nul jusqu’à son admission. [Client][J] | 12 cellules S | Validé |
| S3-05 | READY au point où le service rend son service réel. [Architecture][P3] · [Session INFO][INFO] | Sonde INFO de préparation avant READY, puis INFO par nsh. Après STOP/START, une nouvelle requête client reçoit une réponse. Le backend NovaFS demeure disponible à travers la relance de sa session INFO. [Pilote][JS] | Deux ISA, INFO ordinaire sur 12 cellules S, exploitation interactive à 2 vCPU | Validé |
| S3-06 | Authentifier l’incarnation et l’autorité de READY. [Architecture][P3] · [Gestionnaire][G] | Ancienne trame sur ancien canal, identité actuelle sur ancien canal et ancienne identité sur nouveau canal rejetées. Service 7 encore WAITING sans PID. Seul le READY courant du 8 le débloque. [Client][J] | 12 cellules dont SMP, S | Validé |
| S3-07 | Supervision des sorties, fautes, silences, perte de canal et dépassements d’échéance. [Politique][P] · [Moteur][MP] | Relances bornées, échec après épuisement, retrait des familles si le gestionnaire tombe. Son propre remplacement à chaud reste hors qualification. [Client][J] · [Pilote][JS] | 12 cellules S | Validé avec limites |
| S3-08 | Redémarrage contrôlé. [Architecture][P3] · [Gestionnaire][G] | STOP/STOPPED/START puis RESTART du service de stockage 9. Nouvelles identités complètes, essais et canaux, nouvelles réponses INFO. [Client][J] | 12 cellules S | Validé |
| S3-09 | Révocation. [Architecture][P3] · [Noyau][K] | Retrait des anciens canaux, du droit de contrôle et des familles. Rejet des accès par l’ancien service et refus de suivi par un client ordinaire. [Client][J] | 12 cellules S | Validé |
| S3-10 | Arrêt ordonné et drainage. [Politique][P] · [Gestionnaire][G] | Arrêt du dépendant avant celui du fournisseur, délai d’arrêt coopératif puis retrait du service récalcitrant, collecte et acquittements attendus. [Client][J] · [Oracle][O] | 12 cellules S | Validé |
| S3-11 | Amorçage du tmp sans cycle. [Architecture][P3] · [Catalogue][CAT] | Pilote VirtIO-blk, relais bloc et serveur NovaFS démarrent sans tmp. Après montage P9, le gestionnaire et nsh ont leur tmp, puis les services déclarés avec ce besoin reçoivent le leur. Le service 10 démarre après 9 sans tmp. Douze arrêts de VM et remontages pour les cinq racines du profil P3. [Pilote][JS] · [Oracle][O] | 12 cellules S, ordre et démarrage du cache dans C | Validé |

S conserve 52 démarrages de supervision. R conserve les 44 démarrages complémentaires des suites P8, VFS P9, CLI, scripts et vi. Le gestionnaire emploie 2 072 octets pour son moteur de politique. Le pic supplémentaire du système sur le parcours de supervision couvre 1 208 à 1 212 cadres sur ARM et 1 205 à 1 216 sur x86, à 4 Kio par cadre. Les compteurs de collecte isolent le résidu des domaines DMA et des tables de la plateforme. [Protocole et résultats de phase 3][P3]

## Phase 4 — Cache persistant

| ID | Exigence, contrat et code | Test et observation | Architectures et preuve | Verdict |
|---|---|---|---|---|
| C4-01 | Séparer utilisateur et installation attestés. [Autorités][P4] · [Provisionnement][BC] | U1/A, U1/B et U2/A ont conservé des témoins disjoints. Attribution fournie par le client et jeton d’un autre acteur refusés. Le fournisseur d’attestations se limite aux profils de qualification. [Client][CC] | 12 cellules C, `certifie/cache-*` | Validé avec limites |
| C4-02 | Session ordinaire sans attestation. [Autorités][P4] · [Courtier][BC] · [Sonde nsh][SH] | La commande réellement lancée depuis nsh a créé `sonde` dans son tmp et reçu `DENIED` à l’ouverture du cache. Le lancement reste possible sans compte global implicite. [Pilote CLI][CLI] | 4 sessions P9, les deux ISA × les deux géométries, C `certifie/cli-*` | Validé |
| C4-03 | Continuité de l’identité d’installation, réinstallation et restauration. [Autorités][P4] · [Courtier][BC] · [Magasin][MAG] | 12 mises à jour attestées, 12 réinstallations sans preuve et 24 restaurations sans preuve. Quatre restaurations avec réadmission explicite. Avec tombe, seul A est passé de l’OID 93 à 168, B et C ont gardé 87 et 85. La confiance en l’origine du disque provient de l’autorité du profil. [Pilote][CP] | 12 cellules et 4 réadmissions réparties entre les deux ISA, C `readmission-certifie` | Validé avec limites |
| C4-04 | Persistance après fin d’un processus et redémarrage. [Contrat][P4] · [Magasin][MAG] | Fin, crash et relance d’un acteur, redémarrage sur l’image conservée, relecture des octets et OID par un lecteur indépendant. Un ancien jeton de démarrage est rejeté. [Client][CC] · [Oracle][O] | 12 cellules C | Validé |
| C4-05 | Partage contrôlé entre deux instances de la même paire. [Contrat][P4] · [Courtier][BC] | Jetons distincts sur le même répertoire. Mort d’une instance, seconde capacité indépendante toujours utilisable. Prêt puis retrait VFS avec cache et tmp utilisables. [Client][CC] | 12 cellules C | Validé |
| C4-06 | Quotas par paire, utilisateur, ensemble des caches et réserve physique. [Quotas][P4] · [Magasin][MAG] | Refus aux limites logiques et physiques, fichier creux facturé à sa longueur. Pression invitée sous le seuil de 128 blocs, éviction ciblée et remontée de 127 à 144 blocs. [Tests stockage][CS] · [Pilote][CP] | Deux hôtes × deux géométries, plus ARM 2 vCPU/4096 et x86 2 vCPU/512, C `certifie/pression-*` | Validé |
| C4-07 | Purge de paire, d’utilisateur, du système et maintenance. [Purge][P4] · [Magasin][MAG] · [Courtier][BC] | Portées et privilèges contrôlés, témoins hors portée conservés. L’intention durable borne les anciennes générations. [Client][CC] · [Tests stockage][CS] | 12 cellules et deux hôtes, C | Validé |
| C4-08 | Révocation des capacités et handles. [Contrat][P4] · [Courtier][BC] | Après admission de la purge ou perte du service 10, retrait des anciennes vues. Après redémarrage du service, nouvelle ouverture puis nouvelle lecture. [Client][CC] | 12 cellules C | Validé |
| C4-09 | Purge et recréation immédiate pendant une écriture. [Purge][P4] · [Courtier][BC] · [Magasin][MAG] | Le client attend le NEXT réel. L’autre acteur engage la purge. L’écriture remise est acquittée dans l’ancienne génération. Nouvelle génération vide, ancien handle rejeté et nom issu de l’écriture en vol absent du nouveau cache. [Client][CC] · [Oracle][O] | 12 cellules C | Validé |
| C4-10 | Récupération après interruption. [Purge][P4] · [Magasin][MAG] | Dans chaque cellule, arrêt forcé de QEMU après publication de la tombe et avant le premier retrait. Tombe et données relues sur disque puis purge achevée au redémarrage. L’hôte est resté alimenté. [Pilote][CP] · [Oracle][O] | 12 cellules C, complétées par injections hôtes | Validé avec limites |
| C4-11 | Quarantaine d’une appartenance douteuse. [Autorités][P4] · [Magasin][MAG] | Un octet du marqueur U1/A altéré via P9. Accès refusé et magasin inchangé, B et C encore accessibles. La vérification structurelle suppose un stockage de confiance et une attestation externe de fraîcheur après restauration. [Fixture][CF] · [Oracle][O] | ARM et x86, deux disques C | Validé avec limites |
| C4-12 | Stockage P9 et confinement. [Contrat][P4] · [Interface P9][BACK] | Rejet de traversée et d’usurpation. Noms plats de 1 à 48 octets UTF-8, fragments de 64 octets. Le service expose des opérations relatives à la capacité. Les types disque de P9 sont fichier et répertoire. [Client][CC] · [Oracle][O] | 12 cellules C | Validé |

L’accès au cache pour des comptes réels attend NovaAuth et la fourniture d’attestations d’installation de confiance. La matrice de feuille de route porte la décision sur ces comptes. Les contrôles de C4-01 et C4-02 établissent séparément le comportement des identités provisionnées et celui d’un utilisateur ordinaire aujourd’hui.

Les 104 démarrages consacrés au cache se répartissent en 96 démarrages de la matrice de base, deux pour la corruption de marqueur, deux pour la pression et quatre pour la réadmission. Les preuves C distinguent aussi les régressions de supervision, de CLI, de vi et du stockage. Le registre de compteurs du courtier enregistre un pic de 1 344 à 1 696 octets d’allocations dynamiques et zéro octet à la collecte des arrêts mesurés. Les pics système sont de 941 à 1 339 cadres ARM et de 939 à 1 337 cadres x86. La collecte compare les ressources processus, IPC, piles et tas, avec un relevé séparé du résidu de plateforme. Une VM tuée avant cette collecte ne fournit aucune mesure finale de fuite. [Mesures par cellule][M]

## Formats P8/P9, intégrité des données et résultats des écritures

| ID | Exigence, contrat et code | Test et observation | Architectures et preuve | Verdict |
|---|---|---|---|---|
| FS-01 | Format P9 inchangé. [Contrat][P4] · [Moteur portable][FS] | Diff vide sur `novaFS/` entre la clôture v0.20.0 (`3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e`) et le candidat. Images P9 remontées et lues via le moteur indépendant. [Oracle][O] | 12 cellules C et deux hôtes | Validé |
| FS-02 | Compatibilité historique P8. [Contrat][RET] · [Moteur][FS] | Les 16 démarrages du banc historique final utilisent l’ancien lanceur et conservent son contrat. [Pilote CLI][CLI] | ARM/x86 × 512/4096, C `certifie/p8-*` et R | Validé |
| FS-03 | Aucune migration automatique. [Contrat][RET] · [Admission][AD] | P8 présenté au lanceur P9 et profil P8 présenté au P9 rejetés. La série de seize refus contrôle l’identité du disque et les écritures. [Banc de refus][REF] | ARM/x86 × 512/4096, C `certifie/refus-*` | Validé |
| FS-04 | Génération préservée dans la session historique de lecture. [Contrat][RET] · [Moteur][FS] | Conservation de la génération de la session P8 de lecture. Les profils qui écrivent progressent. L’assertion porte sur la session auditée, sans généralisation aux accès qui enregistrent un atime. [Pilote CLI][CLI] | ARM/x86 × 512/4096, C `certifie/p8-*`, D | Validé |
| FS-05 | Intégrité des données publiques sur P9. [Contrat][P4] · [Magasin privé][MAG] | Comparaison du catalogue et des sentinelles avant/après. Deux vi ont enregistré et relu 28 octets UTF-8 vérifiés au remontage. [Oracle][O] | 12 cellules C et 2 essais vi complémentaires répartis entre les deux ISA | Validé |
| FS-06 | Reprise des purges interrompues dans P9. [Contrat][P4] · [Magasin][MAG] | Tombe, intention durable et borne de génération relues, purge et première écriture éprouvées par injections à différentes frontières. 200 interruptions après FLUSH et 64 erreurs de FLUSH sur la campagne hôte. Le rollback de tout le disque et la coupure électrique physique dépassent cette preuve. [Tests stockage][CS] | 12 cellules invitées, deux hôtes × deux géométries, C `stockage-hote/qualification-finale` | Validé avec limites |
| FS-07 | Distinguer échec certain, `UNCERTAIN` et acquittement. [Contrat de retrait][RET] · [Courtier tmp][BT] · [Magasin cache][MAG] | Avant NEXT, CLOSED. Après remise et avant COMPLETE, UNCERTAIN si résultat inconnu. Résultat enregistré restitué une fois. Cache en erreur à la dernière barrière de publication, deuxième demande refusée avant E/S. [Courtier témoin][WT] · [Tests stockage][CS] | T 1/2/4 vCPU par ISA, D 1 vCPU par ISA/géométrie, C deux hôtes × deux géométries | Validé |
| FS-08 | Absence de double écriture automatique. [Retrait][RET] · [Courtier tmp][BT] · [Courtier cache][BC] | Comptage indépendant dans le service témoin, 0 ou 1 effet suivant la frontière, ancienne demande absente après réinstallation. Huit arrêts du véritable service P9 autour du SYNC, avant publication ou avec une seule publication. Pour le cache, deuxième demande à l’écrivain en erreur rejetée sans E/S et aucune redirection d’une écriture admise vers une nouvelle incarnation. [Courtier témoin][WT] · [Tests stockage][CS] | T 30 cas, D 8 cas sur disque, C hôtes et 12 cellules | Validé |

Une confirmation de WRITE a la portée de l’opération appelée. Le SYNC de P9 dépend des barrières de son stockage. Le protocole du cache confirme chaque fragment séparément. Les essais ne garantissent aucune transaction globale sur une série de WRITE et la collecte d’un résultat du courtier suppose un noyau et un appelant encore vivants. [Contrat du cache][P4] · [Résultats du retrait de NovaFS][RET]

## Exécution des quatre ELF v0.20.0 et conservation du SDK

Les originaux proviennent de E. C conserve les journaux d’exécution, les empreintes des noyaux ordinaires et de sonde, et les résultats. Les quatre noyaux de sonde comprimés sont conservés dans le dossier de preuves externe. Pour chaque ligne B-01 à B-04, les octets de l’ELF d’origine ont été retrouvés intégralement dans le noyau de sonde décompressé de sa cible. Le bilan et les journaux viennent de l’exécution archivée de ce noyau. Aucune recompilation de l’application ancienne ni nouvelle exécution n’a été requise. Le profil sonde sert à injecter le binaire historique en espace utilisateur. [Banc et vérifications][ELF] · [Origine][E] · [Exécutions finales][C]

| ID | Contrat et binaire conservé | SHA-256 du fichier | Architecture, preuve et résultat | Verdict |
|---|---|---|---|---|
| B-01 | ABI v1, exemple du SDK v0.20.0, 28 400 octets. [Banc][ELF] | `db2d14ec8a1f8d1ac22acc331ee4d3d90336a90adc8d50ca057c3129e129c3a8` | AArch64 EL0, sortie 0 et ressources restituées, E + C | Validé |
| B-02 | ABI v1, sonde threads v0.20.0, 77 176 octets. [Banc][ELF] | `fa1640df7589a2700f172acc43197c2904e6e2f833fb748048bc15984575cbd9` | AArch64, deux migrations, préemptions 5/5, 62 007 cadres libres avant et après, E + C | Validé |
| B-03 | ABI v1, exemple du SDK v0.20.0, 22 080 octets. [Banc][ELF] | `1ad5ebf8b6a0d80b10d2df826500f39052fdc222fac418640b408b9657d78c6e` | x86_64 Ring 3, sortie 0 et ressources restituées, E + C | Validé |
| B-04 | ABI v1, sonde threads v0.20.0, 62 736 octets. [Banc][ELF] | `6073a78fb2ffa5c716e75ac942932728faa218d614c3dedf6f30c9f067f6b30c` | x86_64, deux migrations, préemptions 5/5, 62 542 cadres libres avant et après, E + C | Validé |
| B-05 | Conserver l’[archive du SDK distribué][SDK] 0.2.0, 21 237 octets. [Contrat][P4] | `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4` | Fichier distribué relu, empreinte conforme au bilan C des deux architectures, sans régénération. B-01 et B-03 exercent les exemples | Validé |

Les journaux individuels et la provenance des quatre noyaux se trouvent dans le dossier de preuves durable `OS-worktrees/preuves-cache-0210-phase4-80a3699`. Le bilan intégré `elf-certifie/bilan.json` dans C porte l’empreinte `1f80f27b55b905f0b800b4664d36daa9b1af6e6d2bfc278c29c283039ae7e0df`. Les quatre exécutables observés et leurs résultats délimitent la preuve dynamique de compatibilité.

[P3]: ../services-phase3-v0.21.0.md
[P]: ../services-politique-v0.21.0.md
[P4]: ../cache-prive-phase4-v0.21.0.md
[RET]: ../retrait-service-tmp-v0.21.0.md
[M]: ../validation/cache-phase4-v0.21.0.json
[S]: ../validation/services-phase3-v0.21.0-supervision.tar.gz
[R]: ../validation/services-phase3-v0.21.0-regressions-cli.tar.gz
[C]: ../validation/cache-phase4-v0.21.0-preuves.tar.gz
[T]: ../validation/tmp-fin-service-v0.21.0-qemu.tar.gz
[D]: ../validation/tmp-fin-service-v0.21.0-p9.tar.gz
[E]: ../validation/integration-abi-tmp-0210-elf.tar.gz
[G]: ../../user/service_manager.rs
[K]: ../../kernel/src/sched/process/ipc/service.rs
[CAT]: ../../user/services/catalog.rs
[MP]: ../../user/services/policy.rs
[INFO]: ../../user/storage/info_session.rs
[J]: ../../user/service_control_test.rs
[JS]: ../../tests/services.py
[POL]: ../../tests/services_politique.rs
[O]: ../../tests/tmp_prive_oracle.rs
[BC]: ../../kernel/src/sched/process/cache.rs
[MAG]: ../../user/novafs/files/cache_store.rs
[BACK]: ../../user/novafs/files/cache.rs
[CP]: ../../tests/cache_prive.py
[CC]: ../../user/cache_client_test.rs
[CS]: ../../tests/cache_stockage.rs
[CF]: ../../tests/cache_corruption_fixture.rs
[SH]: ../../../components/coreutils/probe.rs
[CLI]: ../../tests/tmp_prive_cli.py
[REF]: ../../tests/tmp_prive_refus.py
[AD]: ../../kernel/src/sched/process/storage.rs
[FS]: ../../../novaFS/src/fs.rs
[WT]: ../../tests/tmp_attentes.py
[BT]: ../../kernel/src/sched/process/tmp.rs
[ELF]: ../../tests/abi_elf_archive.py
[SDK]: ../../../distribution/sdk/nova-system-0.2.0.tar.gz
