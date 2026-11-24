# NovaAuth v0.24.0 — Portée VFS des tickets EXEC et reprise froide R2

25 novembre 2026, heure de Paris. Référence `07cceee5d6cd83255f7a246834c804fc2d69e271`.

Code contrôlé `dc5d553865918e8009245df8bdfa2f5958a0ac97`.

Branche `codex/novaauth-administration-0240-phase1`.

## Contrôle raccordé

Le contrôle de révision d’une permission pouvait réussir alors que le sceau du dossier avait été retiré de la vue P9. Le test conserve la règle, retire le sceau et vérifie le refus de revalidation. Le registre compare désormais l’empreinte SHA-256 complète de la liaison `NXScope1` calculée depuis les métadonnées immuables du ticket à celle de la vue P9 fraîche. Il contrôle aussi l’origine authentifiée, les droits et les révisions. Un dossier avec un autre sceau reste refusé, même si l’installation et la règle concordent.

La vérification du descripteur admet la représentation d’une seule portée scellée en lecture pour le rôle simple, avec le dossier du titulaire comme répertoire de travail. Elle exige la même identité, le même dossier et les droits exacts, et refuse une portée de fichier avec canal administratif. Un ticket avec portée et sans descripteur lié ne peut pas emprunter le chemin d’admission historique. Les contrôles d’installation, de session, d’instance et d’échéance continuent de s’appliquer. Le format des descripteurs et les trames privées de 128 octets restent identiques.

## Chaîne G/H encore ouverte

Le chargeur isolé transmet encore une portée vide. L’émission ne lie pas de descripteur G/H. Brancher directement la portée sur le lecteur de fichiers enfant actuel réintroduirait une consultation du registre local et sa règle Direct. Il reste à transmettre la décision du registre isolé au vrai effecteur VFS, à l’associer aux deux canaux fichier et aux admissions individuelles, puis à raccorder la fermeture des tickets, des lecteurs préparés et des baux à l’inventaire du retrait. Le coordinateur doit conserver les issues des admissions antérieures et vérifier les trois acquittements distincts. Les contrôles ajoutés seuls ne permettent pas de confirmer ce drainage.

## Matrice des preuves

| Recette | x86_64/P9-512 | x86_64/P9-4096 | AArch64/P9-512 | AArch64/P9-4096 |
|---|---|---|---|---|
| Q1 VFS Direct/G/H via `nsh` | Historique acquis | Historique acquis | Historique acquis | Historique acquis |
| E1 ancien ticket Direct, consommé par Bob | Acquis sur fixture compacte | Acquis sur fixture compacte | Acquis sur fixture compacte | Acquis sur fixture compacte |
| E2 ancien ticket H utilisé après retrait de G | Non exécuté | Non exécuté | Non exécuté | Non exécuté |
| E3 ancien ticket G, refus avant échéance, aucun PID | Non exécuté | Non exécuté | Non exécuté | Non exécuté |
| Q2 admission VFS en vol et annulation avant GO | Historique acquis | Historique acquis | Historique acquis | Historique acquis |
| Q3 confirmations concurrentes et E/S VFS en vol | Historique acquis | Historique acquis | Historique acquis | Historique acquis |
| Q2/Q3 admission EXEC G, issue et drainage | Non exécuté | Non exécuté | Non exécuté | Non exécuté |
| Q4 réponse après ACK durable perdue | Historique acquis | Historique acquis | Historique acquis | Historique acquis |
| Q4 coupure après réservation avant publication | Refus sûr historique | Refus sûr historique | Refus sûr historique | Refus sûr historique |
| Q4 reprise avec ancienne incarnation disparue | Contrat soumis | Contrat soumis | Contrat soumis | Contrat soumis |

Les résultats invités et leurs commits restent ceux de la [matrice historique](novaauth-r2-ancien-direct-v0.24.0.md#matrice-consolidée-q1q4). En particulier, E1 demeure une preuve sur `b67f25d8ea30b8ce6d6955020ecc407a29b8367a` avec le générateur de fixture `d20b8637ed44cfd2563d0a5a00c802cfa8a24cc3`. Sur le scénario à portée nulle, la vérification de métadonnées exige toujours exactement la dépendance d’installation et les mêmes champs. Les tests natifs confirment cette équivalence au niveau du descripteur. Le nouveau code n’a pas été exécuté dans l’invité.

## Exécutions et ressources

- 6 tests de distribution, 12 de descripteur et 17 de coordinateur et de layout réussis sur ARM64 natif et x86_64 sous Rosetta. Les tests de politique utilisent des volumes P9 simulés de 512 et 4 096 octets. Ils enregistrent les droits Direct, G et H, puis vérifient notamment le refus de G, le maintien de H et le refus d’un sceau divergent ou retiré.
- 18 tests hôtes de l’autorité passent. Ils couvrent la réservation concurrente à l’admission, le refus via le chemin historique, la perte de réponse et le traitement de l’issue par identifiant.
- 9 contrôles ABI réussis sur l’export brut avec accès en lecture à l’historique Git. Le premier lancement sur un export sans métadonnées Git a produit quatre erreurs d’accès à cet historique. Les 48 sélecteurs et les sources des noyaux ordinaires et de NovaFS P9 sont inchangés.
- Compilations invitées réussies sur les deux architectures dans l’export sélectif. Durées hôte de 23,14 s sur x86_64 et 22,52 s sur AArch64. Les empreintes des noyaux et des registres figurent dans les résultats. L’avertissement historique `rust-objcopy` sur `libLLVM.dylib` subsiste.
- Sous-total statique additionnel mesuré de **4 026 / 4 096 octets** sur les deux cibles natives, soit 70 octets disponibles. La vérification de portée travaille sur la pile à partir des métadonnées et du descripteur lus. Aucun endpoint ni racine IPC n’est ajouté par ce changement.
- Aucune VM démarrée, aucune acquisition du verrou et aucun temps consommé par le banc QEMU. Les relevés précédents de quatre racines maximum, 74 cadres IPC au pic et zéro cadre IPC en fin de drainage restent des mesures de la fixture Direct compacte, sans mesure nouvelle de pic invité.
- La fixture **complète AArch64/P9-512** conserve sa réserve d’expiration pendant l’admission. Les réserves de 301 cadres instrumentés AArch64 et de reproductibilité entre environnements restent ouvertes.

## Reprise froide et preuves

Le [contrat soumis à arbitrage](novaauth-r2-reprise-froide-proposition-v0.24.0.md) définit la quarantaine, la reprise explicite de la même intention, les preuves par effecteur, la confirmation durable et les conditions de restauration de disponibilité. La disparition d’un effecteur ne produit aucun acquittement. Aucune règle proposée n’est implémentée dans cette publication.

Les dix fichiers du prototype local reproduisent leurs empreintes précédentes. L’archive historique a été revérifiée. La nouvelle archive contient vingt fichiers et toutes leurs empreintes ont été relues. [Résultats](validation/novaauth-r2-portee-exec-v0.24.0/resultats.json) · [Archive](validation/novaauth-r2-portee-exec-v0.24.0/archive.json) · [Manifeste SHA-256](validation/novaauth-r2-portee-exec-v0.24.0/MANIFEST.sha256).
