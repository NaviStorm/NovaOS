# Préparation de la première version stable v0.30.0

**Base auditée le 14 novembre 2026** — NovaOS v0.17.0, merge publié `010ae3998aa158811b8a05c6d47753cd4589ad9a`. NovaFS v0.9.13, format P9. La branche documentaire `codex/audit-v0300` part de ce merge. Le prochain arbitrage porte sur le périmètre stable, le matériel de référence et l’ordre des qualifications. [Fusion, profils et preuves](../fusion-smp-v0.17.0.md) · [Versions et licences](../../../AGENTS.md)

| Livrable | Utilisation |
|---|---|
| 1. Inventaire technique réel | [CPU, mémoire, sécurité du noyau et ABI](inventaire-noyau-abi.md) · [Matériel, identité et distribution](inventaire-materiel-confiance.md) · [Stockage, interface et outils](inventaire-stockage-interface-outils.md) |
| 2. [Matrice des exigences](02-exigences.md) | Relier une exigence à la preuve actuelle et au contrôle d’acceptation. |
| 3. [Analyse des écarts](03-ecarts.md) | Délimiter le travail restant et l’environnement dans lequel le démontrer. |
| 4. [Dépendances](04-dependances.md) | Ordonner les chantiers et repérer les travaux parallélisables. |
| 5. [Risques et blocages](05-risques.md) | Décider d’une suspension, d’un changement de périmètre ou d’un nouveau banc. |
| 6. [Feuille de route v0.18.0–v0.30.0](06-feuille-de-route.md) | Treize jalons proposés, chacun avec prérequis, épreuves et livrables. |
| 7. [Plan de qualification](07-qualification.md) | Choisir le modèle de panne, le profil et les preuves à conserver. |
| 8. [Stratégie du lanceur](08-lanceur.md) | Orchestrer les tests individuels et conserver les raisons des cellules manquantes. |
| 9. [Méthodologie CVE](09-methode-cve.md) | Établir l’applicabilité à une version et valider la correction d’un invariant. |
| 10. [Décisions MOA/MOE](10-arbitrages.md) | Choisir les profils, les engagements de compatibilité et les portes de livraison. |

**Lecture des états.** I désigne un chemin implémenté dans le profil indiqué. P désigne une réalisation partielle ou une preuve limitée à un autre environnement. D désigne une orientation décidée dont le résultat attendu reste à développer. A désigne une fonction absente du périmètre livré examiné. Une proposition de l’audit conserve ce statut jusqu’à l’arbitrage. La priorité pour la stabilité est un axe distinct de l’état technique. Une observation sur quatre vCPU émulés ou sur un volume autonome ne qualifie à elle seule ni un portable physique ni tous les modes de NovaFS. [Profils SMP](../fusion-smp-v0.17.0.md) · [Périmètre natif de NovaFS](../novafs-integration-v0.11.0.md) · [Orientations publiées](../../../README.md#feuille-de-route)

Pour préparer un mandat ultérieur, indiquer l’exigence du tableau, le profil cible, les droits accordés, le cas de refus attendu, le banc de régression et la décision MOA/MOE dont il dépend. Le premier stable reste v0.30.0, avec une interface graphique utilisable. CPython est reporté après ce jalon par décision MOA. Le passage à v0.18.0 attend la validation de la feuille de route.
