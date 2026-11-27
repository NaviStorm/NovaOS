# NovaOS v0.25.0 — Phase 5D — Démarrage physique sécurisé

**Décision du 27 novembre 2026. Le premier démarrage physique reste bloqué.** Le firmware décrit un chemin plausible vers une unité VT-d commune au réseau et au stockage. Les éléments manquants pour un premier essai *passif* sont un support authentifié de bout en bout, un chemin de diagnostic générique dont les effets sont maîtrisés et une autorisation A8 portant sur son empreinte. La preuve d’isolation DMA entre NIC et SATA relève d’une qualification active ultérieure.

| Question | Réponse en 5D | Dossier |
|---|---|---|
| Une même unité peut-elle isoler deux périphériques ? | Oui en architecture VT-d, si leurs requêtes sont distinguables, leurs chemins isolables et leurs contextes indépendants. L’inventaire MSI établit une couverture candidate. | [Découverte et domaines](decouverte-et-domaines.md) |
| Une même image peut-elle découvrir et sélectionner ses pilotes ? | Le catalogue sait sélectionner un plan. Le raccordement à un inventaire physique générique et la confiance de distribution restent à qualifier. | [Image et profil de diagnostic](distribution-et-diagnostic.md) |
| L’ISO actuelle est-elle admissible sur le MSI ? | Aucun chemin de confiance complet démontré. Les deux copies x64 de Limine examinées sont identiques et leur table de certificats PE est vide. La configuration et le noyau sont des fichiers séparés. | [Contrôles et provenance](preuves.md) |
| Quand présenter le premier A8 ? | Après satisfaction des critères hors matériel, avec le mode passif, les limites d’observation, la conservation de Windows et la procédure d’arrêt explicitement acceptés. | [Arbitrages et matrice d’acceptation](acceptation.md) |

## Entrée de qualification

La MOA rapporte sous Windows trois tables valides (DMAR, MCFG et APIC), deux DRHD, une fenêtre ECAM et un IOAPIC. Le Killer E2400 et le SATA relèvent de la même unité candidate par défaut, appelée U2 dans la synthèse de collecte. Aucune portée RMRR ou ATSR n’y correspond d’après le rapprochement effectué. Le firmware déclare le remappage des interruptions. UEFI et Secure Boot étaient actifs sous Windows. Le numéro U2 est local à cette lecture.

L’état actif de VT-d, les traductions du prochain démarrage et l’isolation effective des requêtes restent inconnus. Les octets des tables, chemins individuels, adresses, variables de confiance et données de récupération restent chez l’opérateur. Le dossier emploie uniquement la synthèse communiquée et des preuves logicielles publiques du projet. [Méthode d’inventaire 5C.1](../killer-e2400-5c/inventaire/acpi-5c1.md)

## Périmètre de la livraison

La conception part de `5b7c30103b5ab624b15fc4dec7e1878785acdc25` sur `codex/killer-e2400-phase5b`. Les essais de modèles sur hôte et la relecture des archives sont détaillés dans les preuves. L’image de distribution, son signataire, le diagnostic physique et le courtier physique restent à livrer ou à qualifier dans leurs mandats respectifs. Les 48 sélecteurs, les politiques de capacités et la garde de coexistence conservent leur définition actuelle.
