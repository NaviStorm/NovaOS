# NovaAuth v0.24.0 R2 — Point de suspension du 25 novembre 2026

**Décision MOA/MOE.** Développement et qualification R2 suspendus à la suite des interruptions de plateforme signalées. La suspension ne prononce ni la clôture de v0.24.0 ni un verdict sur la classification de ces interruptions. Ce relevé s’appuie sur Git et sur la relecture des archives. Aucun nouveau test hôte ou invité n’a été exécuté.

## Références de reprise

Branche `codex/novaauth-administration-0240-phase1`. Tête fonctionnelle et documentaire relue directement sur origin avant ce dépôt documentaire : `72279a07bf5fdd8076cb4f80e356ef5e2e24b500`, arbre `d676b0f117fed84af43a8f8498cdaf4b55efb5ba`. La tête distante de main est `5c7f1bfa313a3859ccc0830660438e76d598acb1`.

| Commit | Portée à conserver |
|---|---|
| `46d04ff`, `82349ed` | Audit de l’administration native puis état des fonctions. Les 36 cellules S1–S9 et les campagnes fonctionnelles ont chacune leur révision et leur oracle. |
| `1fdf10d`, `5665ba5` | Contrat de reprise froide et conservation des faits originaux par classe d’effecteurs. Toute substitution d’acquittement ou réactivation reste soumise à arbitrage. |
| `5361ff4`, `6498dd4`, `e471f36`, `777ff38` | Portée fichier, synchronisation du banc, consolidation, preuve E3 et proposition d’isolation duplex. |
| `4c21fd7`, `357351b` | Raccord duplex du banc, E2 sur les quatre profils, mesures et bilan unitaire. |
| `72279a0` | Drainage du registre vivant corrigé et seize parcours de la dernière matrice E2/E7G/E8G/E8H. |

Le worktree R2 se trouvait à `777ff38` au début de l’inventaire. Il a avancé en ligne directe sur les trois publications distantes, après vérification de la disjonction des chemins. L’index était vide et les dix empreintes étaient identiques avant et après. Le worktree de diagnostic est détaché sur `72279a0`. Son état local comporte aussi quatre copies de scripts de banc non suivies. L’inventaire ci-dessous identifie les originaux à conserver sans les intégrer au code publié.

## Qualifications acquises

Les quatre profils désignent x86_64 et AArch64, chacun en P9/512 et P9/4096. Les résultats gardent les binaires, le modèle de panne et la fixture spécifiés par leur dossier. Les campagnes historiques se recoupent.

| Parcours | Preuve disponible | Limite pour la reprise |
|---|---|---|
| Administration et isolation R1/R2 | 36 cellules S1–S9 dans leur périmètre accepté. Création, consultation, renommage, ajout d’appartenance, confirmation et consultation de reçus exercés dans les invités. La campagne ultérieure de 52 parcours couvre notamment la concurrence sur un facteur, l’authentification de Carol, les six ELF en script et depuis `nsh -c`, les pertes de réponse et interruptions définies par la recette. | Les 52 parcours ont 13 résultats par profil. Les conserver avec les échecs diagnostiqués et les rapports antérieurs, sans additionner les campagnes comme des garanties indépendantes. [Audit](novaauth-r2-audit-cloture-20261122.md) · [Fonctions et reprise](novaauth-r2-fonctions-20261123.md) · [Cinq fonctions](novaauth-r2-cinq-fonctions-v0.24.0.md) |
| Direct et G | E1 Direct réussi sur fixture compacte. E3 G rend `REVOKED(10)` avant expiration, sans admission ni création d’enfant après retrait. Sur AArch64/P9-512 les deux succès compacts ont 7 et 9 ticks de marge. Les essais initiaux étaient en retard de 41 et 44 ticks. | Conserver la réserve de fixture complète AArch64/P9-512. [Direct et matrice Q1–Q4](novaauth-r2-ancien-direct-v0.24.0.md) · [Preuve G](novaauth-r2-bilan-exec-g-et-arbitrage-duplex-v0.24.0.md) |
| E2 H après retrait de G | Sur chaque profil, statut 0, enfant PID 23, stdout de 14 octets conforme, stderr vide, quatre admissions VFS avec leurs issues. | Les nouvelles admissions ont chacune leur identifiant. L’admission historique H en position 19 demeure `Pending`, sans terminal ni enfant attesté. [Premier E2](validation/novaauth-r2-canaux-duplex-777ff38/README.md) · [Historique H 19](validation/novaauth-r2-canaux-duplex-777ff38/h19.json) |
| Courses exercées dans le registre vivant | Sur les quatre profils, E7G ferme la porte avant l’admission distante. E8G annule irréversiblement après admission et avant création. Tous deux rendent 10 avant échéance sans enfant. E8H conserve l’admission indépendante et crée un seul enfant après le retrait. Les classes 1, 2 et 4 ont chacune un acquittement avant `RETIREE`. | La réservation, l’admission et l’annulation sont distinguées par leurs positions durables. Ces essais couvrent uniquement les entrelacements décrits. [Matrice des seize parcours](validation/novaauth-r2-courses-duplex-20261125/matrice.json) |

## Ouvert à la suspension

- **Contention et isolation E10.** La recette sur le dernier emplacement partagé entre deux véritables enfants fichier reste à qualifier. Les résultats bruts locaux de la variante `e10-fichiers` rapportent un succès sur x86_64/512, x86_64/4096 et AArch64/4096 pour la concurrence du client administratif et du lecteur H. AArch64/512 rapporte « opération sans terminal ». Le dossier local `e10-dernier` contient une préparation du banc et aucun résultat invité. Ces pièces intermédiaires sont conservées séparément de la matrice publiée.
- **Reprise froide et admission historique.** Conserver la quarantaine de l’opération H 19 d’origine. Une sortie de processus, un compteur vide ou un succès ultérieur sur une autre base n’établit pas son annulation. Réconcilier par le même identifiant sans seconde mutation. La remise en disponibilité d’un retrait incomplet après disparition de ses effecteurs exige les justifications de chaque classe et l’arbitrage prévu. [Complément de reprise froide](novaauth-r2-preuves-avant-coupure-v0.24.0.md)
- **Ressources et compatibilité.** Derniers exports instrumentés à 4 094/4 096 octets statiques sur les deux cibles, dont 4 octets de compteurs de pics. Maximum observé de quatre racines par acteur et 74 cadres IPC ; endpoints, nœuds, espaces et cadres IPC à zéro en fin des seize essais. Le minimum de cadres physiques libres observé va de 59 025 à 59 027 sur x86_64 et de 58 502 à 58 506 sur AArch64. Ces mesures portent sur la fenêtre instrumentée. Maintenir les réserves des 301 cadres instrumentés AArch64, de reproductibilité entre environnements et de fixture complète AArch64/P9-512. Conserver les 48 sélecteurs ABI v1, les baux, les plafonds IPC et P9. [Tailles, mesures et portée](validation/novaauth-r2-courses-duplex-20261125/README.md)
- **Décision de clôture.** Lors d’une reprise autorisée, rapprocher chaque exigence encore ouverte de son oracle et des empreintes du futur candidat, puis soumettre un nouvel audit. Les sources expérimentales et la qualification QEMU ne valent pas activation ou qualification de production.

## Préservation matérielle des preuves

Les trois manifestes publics EXEC ont été contrôlés en relisant chacun de leurs fichiers. Les 80 empreintes des originaux relevées dans les seize résultats finaux (série invitée, résultat, deux collectes noyau, base de l’autorité) concordent avec la copie locale de conservation. Les autres archives historiques restent à leurs emplacements. Ce contrôle ciblé ne recertifie pas toutes les campagnes anciennes.

| Dossier sous `novaOS/docs/validation/` | Entrées relues | SHA-256 du manifeste |
|---|---:|---|
| `novaauth-r2-exec-g-duplex-20261125` | 52 | `f5258363e04262b1baf3f07e0db2991ced7c72da1757eb39341d814c721c6fea` |
| `novaauth-r2-canaux-duplex-777ff38` | 26 | `c18db8a1aec0e8a0bc9a82046b4531c6fac121e832979e45f25b4b7516b93e8f` |
| `novaauth-r2-courses-duplex-20261125` | 53 | `a1dc732fa303d2c9fc92ad1159d5baf5c321dad3ce5a97eccff4feaceb841c0b` |

Archive privée additionnelle sous `OS-worktrees/archives-novaauth-administration-0240/suspension-72279a0/`, hors des temporaires et de Git. Elle conserve les prototypes locaux, les bancs, les pièces de qualification sélectionnées, les échecs et les images associées. 4 905 fichiers copiés et relus, 951 616 010 octets, SHA-256 du manifeste `a5f51857157b6097aa0180db339567a8ae36b31d475472536e0840456c22a3cb`. Les originaux demeurent en place.

## Inventaire local au gel

`M` indique un fichier suivi modifié et `??` un fichier non suivi. Les chemins sont relatifs à leur worktree.

| Worktree | Git | Chemin | SHA-256 |
|---|---|---|---|
| R2 | `M` | `novaOS/user/novaauth/service/exec_membership_lab.rs` | `64a49106214af81de1ecd3526f00db641fb9f53eedca6ca891d79d3a6238d506` |
| R2 | `M` | `novaOS/user/novaauth/service/membership_exec_tests.rs` | `01a6a0c475e307d6e3d3b17ce4008b6bc988dbee6945d219653548db3e302260` |
| R2 | `M` | `novaOS/user/novaauth/service/membership_profile.py` | `664cfb776986616ec8732e451732713554b830b6d46bafc0d2a1faac336990b8` |
| R2 | `M` | `novaOS/user/novaauth/service/qualify_membership_effectors.py` | `ecba0477c46d911e06e26e09245dbeebf08d4709651d1e1b3bb7f46dc6b11922` |
| R2 | `M` | `novaOS/user/novaauth/service/src/bin/common/exec_admission_guest.rs` | `b6b6791377e3b5ca893267f3a9504ca781baec35bf482e45f780f1eee85621d7` |
| R2 | `M` | `novaOS/user/novaauth/src/distribution.rs` | `7cce47084287fedc2670ee078c621dffab40ab83ed2b4aea76240f7b3567745a` |
| R2 | `M` | `novaOS/user/novaauth/src/membership.rs` | `e2739c78256d57b12236d938ed38f8b36d3fb3d1a9380d1fea0c69d4fd336160` |
| R2 | `??` | `novaOS/user/novaauth/service/membership_exec_origin.rs` | `ae574c9909b832591baca6140a4a882dccba689968e6bb8e7f5fb472872e50dd` |
| R2 | `??` | `novaOS/user/novaauth/service/membership_exec_profile.py` | `13d04363d3e7ccb368c21d18d61c56a76fde7aa99c442a0051c16a377d9f4809` |
| R2 | `??` | `novaOS/user/novaauth/service/src/bin/common/membership_exec_gate.rs` | `5b6f711f077e0c847b088a8baf55de7c91228ff6e89d3aae206424bd0cd50b9f` |
| diagnostic | `??` | `novaOS/docs/validation/novaauth-r2-courses-duplex-20261125/final-e7g-banc.py.txt` | `b9bb734c39581a9afe0ff80cbc48f119644b1bb508ad2bc97023187664fa0e93` |
| diagnostic | `??` | `novaOS/docs/validation/novaauth-r2-courses-duplex-20261125/final-e8g-banc.py.txt` | `20b5631a6aa5049468647552a5926a5ea1e4f47db2d1c6b6bbfacd041d254d9c` |
| diagnostic | `??` | `novaOS/docs/validation/novaauth-r2-courses-duplex-20261125/final-e8h-banc.py.txt` | `c1ec321d2ee4b13b52483e422e6e958e0c998b9e3996a1040ae0bf7744e8b3cd` |
| diagnostic | `??` | `novaOS/docs/validation/novaauth-r2-courses-duplex-20261125/final-h-banc.py.txt` | `bc02c29ecbe1d4e25c53c045af27bb69c42a6de087af623e6921730c1cc11cbd` |
