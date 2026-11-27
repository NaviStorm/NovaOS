# NovaOS v0.25.0 — Killer E2400 — décision préparatoire 5C

**Décision proposée : poursuivre l’inventaire Windows. Le dossier actuel ne permet pas d’autoriser le démarrage Live ni l’activation physique du NIC.** Il manque notamment une chaîne de confiance acceptée par le MSI, l’inventaire de ses requesters PCIe et une preuve de drainage propre au contrôleur. Aucun support n’a été créé, aucun accès au MSI n’a été effectué.

| Pièce utilisable | Destinataire et résultat |
|---|---|
| [Dossier Windows](inventaire/README.md) | Opérateur sur place. Collecteur PowerShell, lecture ACPI optionnelle sans dépendance tierce en Python, mode de conservation privée et grille d’interprétation |
| [Spécification du courtier et matrice A7](courtier.md) | MOE noyau, pilote et sécurité. Neuf opérations candidates avec propriétaire, effets, préconditions, preuve de clôture et conduite sur incertitude |
| [Protocole Secure Boot et récupération](demarrage.md) | MOA et opérateur. Critères d’admissibilité du support, vérification de chaque étage, règles de protection du stockage et retour Windows |

### Portée des preuves relues

Le point de départ est la livraison simulée 5B `0fb87dfa4936773f246a053c0705c7f62b36c126`, issue de 5A `7c3bb99a2f63326818024ac8e498ec329c6ad345` et du gestionnaire 4F `908976948ba58fc85e7fe38bfc315f689ed1c44e`. La phase 5C ajoute uniquement le dossier de préparation et ses contrôles hors ligne.

| Invariant | Contrôle 5C du 27 novembre 2026 | Portée |
|---|---|---|
| ABI et code concerné | 48 entrées dans le manifeste ABI, comparaison Git avec 5B du noyau, des bibliothèques, des pilotes, des tests, de NovaFS et du manifeste interne | Identité des sources |
| Propriété, droits, incarnation et génération | Vérification des empreintes des pièces, relecture des réussites des tests hôtes de direction par page, du lien titulaire/incarnation/génération et de l’unicité du crédit de clôture. Dans le scénario 5B, les preuves dupliquées et d’une autre génération sont refusées avant le retour du groupe | Résultats historiques ARM64 natif et x86_64 Rosetta. [Assertions du scénario](../../tests/killer_e2400/scenario_5b.rs) |
| Réserve et retrait | Journal Ring 3 avec 259 cadres, 264 TX/RX et retour par preuves de 259 à 130 puis 0. Le reset isolé conservait les 259 cadres | Q35, oracle CPU uniquement. [Fiche 5B](../validation/killer-e2400-phase5b/q35-5b.json) |
| Non-régression VirtIO-net/NVMe | Trois démarrages ARM à 13 312 trames/sens. Deux journaux Q35 avec 22 READ, 7 WRITE, 3 FLUSH chacun et empreintes des deux volumes P9 vérifiées. Retrait nominal : zéro cadre retenu. Reçu absent : 26 retenus | Relecture hors ligne. [Bilan 5B](../validation/killer-e2400-phase5b/compatibilite.json) |
| Nouveaux outils | Collecteur exercé sur données fictives avec PowerShell 7.7.0 ARM64 Darwin. Cas nominal, erreur de lecture, Secure Boot inactif, identité ambiguë, classe divergente et fonction sœur de stockage détectée. Lecteur de tables exercé sur données fictives valides, corrompues, avec signature erronée, longueur invalide, taille excessive ou réponse incomplète | Le test utilise des cmdlets remplacées par des fonctions factices. Windows PowerShell 5.1, les fournisseurs PnP/CIM et les appels natifs au firmware restent à exercer sur place. Aucune campagne QEMU ou suite Rust rejouée, car leurs sources sont identiques à 5B |

Depuis la racine du dépôt, avec les temporaires prescrits dans AGENTS.md, lancer `python3.11 -B novaOS/docs/killer-e2400-5c/verifier.py` (Python ≥ 3.11) et `pwsh -NoProfile -NonInteractive -File novaOS/docs/killer-e2400-5c/essai-collecte.ps1`. Le second script utilise des fonctions factices et s’exécute dans son propre processus. [Archive source des contrôles](../validation/killer-e2400-phase5b/preuves-5b.tar.gz)

### Informations encore inconnues et décideurs

| Inconnue | Preuve attendue | Porte bloquée |
|---|---|---|
| Identité du MSI à cette époque, firmware, révision et sous-système exacts, pilote actif | Relevé privé Windows puis recoupement de la fonction présente | Identification |
| Modèle de carte mère, signataires réellement acceptés, db/dbx applicables, état BitLocker et retour possible | Revue privée de la politique du firmware et du candidat signé, référence Windows | A8 démarrage |
| Segment, ponts, fonctions sœurs, alias, ACS/ATS, réservations firmware, unité DRHD, contrôleurs et portée des resets communs | Tables et arbre Windows puis recensement PCI/ACPI local au démarrage. Une fonction masquée ou un requester non résolu bloque l’attribution | A7 domaine et IRQ |
| État de protection DMA pendant le transfert UEFI, contrôleur USB et propriétaires antérieurs | Revue du chemin de boot, observation de la continuité de traduction. Aucune extinction de traduction partagée | A8 diagnostic et A7 attribution |
| BAR0 réelle, D0, PHY et horloge MDIO, errata MSI/INTx, cohérence des caches et configuration applicable à la révision | Sources de révision et observations individuelles autorisées, sans extrapolation depuis Q35 ou un autre MSI | A7 PHY, files et IRQ |
| Portée du reset, transactions PCIe en vol, garanties de drain VT-d et périphérique, indépendance du stockage | Preuve par génération, domaine et page et témoin indépendant du stockage. En l’absence de preuve, aucune réattribution | A7 BME, trafic, retrait et réouverture |

### Proposition de mandat suivant — premier passage physique

**Objet soumis à la MOA : un démarrage diagnostic x86_64 unique, sans trafic et sans attribution du NIC.** Préalables à signer : inventaire Windows acceptable, candidat Live et support nommés et hachés, mécanisme de vérification transitive contrôlé, procédure de retour acceptée par le propriétaire, accès locaux de diagnostic précisément recensés et couverts par A7 si nouveaux. Inscrire le modèle et l’unité physique, la date, l’opérateur présent et l’observateur. Plafond proposé : un démarrage et 30 minutes sur place. Une porte non satisfaite clôt le passage avant démarrage.

Sous cet **A8 distinct**, l’opérateur utilise le menu ponctuel et recueille uniquement identité PCI, tables ACPI, chemin de stockage, état initial du bus mastering et état observable des unités IOMMU. Le candidat doit pouvoir observer sans réclamer le NIC, écrire en configuration PCI, toucher BAR/MDIO ni modifier l’IOMMU. Un BME déjà actif, un alias inconnu, une perte de protection, une demande de changement du firmware ou une activité de stockage interne imprévue arrête le diagnostic. Appliquer le protocole de retour et sceller une synthèse locale expurgée. La clôture exige comparaison Windows avant/après et une décision explicite sur chaque inconnue.

Un **deuxième mandat** seulement pourra demander l’admission d’opérations précises de la matrice A7 puis l’A8 qui les exercera. La lecture MDIO, l’armement de l’IRQ, BME, les trames au câble et le retrait actif nécessitent chacun leurs préconditions et témoins. Le premier passage ne délègue aucun de ces droits.
