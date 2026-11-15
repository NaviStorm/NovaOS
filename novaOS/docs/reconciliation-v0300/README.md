# NovaOS — Première stable serveur en mode texte

**Décision approuvée par la MOA le 15 novembre 2026.** NovaOS v0.30.0 sera un serveur généraliste administrable et exploitable en mode texte. Une application serveur native devra pouvoir être installée, jointe depuis une autre machine, supervisée, sauvegardée et remise en service avec les droits attendus. La pile graphique complète est reportée après v0.30.0. QEMU x86_64 est la référence principale, QEMU AArch64 la seconde référence obligatoire. Le Tecra A50-J reste la cible physique expérimentale prioritaire. [Registre de décision](decisions.md) · [Plateformes de qualification](contrat-plateforme.md)

## Lire le dossier serveur

| Question | Document de référence |
|---|---|
| Quelles fonctions faudra-t-il démontrer, lesquelles manquent en v0.19.0 et de quoi dépendent-elles ? | [Matrice des exigences serveur, des commandes et des profils applicatifs](exigences-serveur.md) |
| Comment la MOA pourra-t-elle accepter ou refuser un candidat ? | [Seize étapes de bout en bout, sur les deux architectures, et qualification selon le risque](acceptation-serveur.md) |
| Quel est le premier chantier à préparer ? | [Bibliothèque système Rust et cycle de vie des threads](contrat-rust-threads.md) |
| Comment un enfant de nsh accède-t-il à une donnée surprotégée et que devient ce droit après restauration ? | [Identités, délégations et garde des clés](contrat-autorisations.md) · [Limites et options du stockage P9](dependances-p9.md) |
| Quels numéros et choix attendent encore une validation MOA/MOE ? | [Répartition proposée et dépendances critiques](proposition.md) |
| Pourquoi les anciens documents plaçaient-ils NovaWM avant la stable ? | [Historique et sources](historique.md) · [Proposition antérieure figée](https://github.com/NaviStorm/NovaOS/blob/29049a50758f73d472f03ad733bcb3eac74b5259/novaOS/docs/reconciliation-v0300/proposition.md) |

La dernière publication de sources reste NovaOS v0.19.0 au commit `1b3bb5a8df767a09fa0671bcfd95e6e734693a3a`. La distribution UTM AArch64 demeure v0.17.0 et NovaFS v0.9.13 P9. Les contrôles de cette mission portent sur les documents, leurs liens, les contraintes et la traçabilité des verdicts à produire. Le serveur stable reste à développer et à qualifier. [Distribution](../../../distribution/README.md)

## Les versions effectivement publiées

| Version et commit de fusion | Réalisation vérifiée dans la source | Qualification disponible et limites |
|---|---|---|
| v0.17.0 · [010ae3998aa158811b8a05c6d47753cd4589ad9a](https://github.com/NaviStorm/NovaOS/commit/010ae3998aa158811b8a05c6d47753cd4589ad9a) | Démarrage des CPU secondaires sur les deux ISA, ordonnanceur préemptif, IPC multicœur, arrêt coordonné et clôture des réservations DMA. Contexte FPU/SIMD dans le profil explicite `user-fpu`. [Code SMP](../../kernel/src/smp.rs) · [Contrat FPU](../smp-phase2-v0.17.0.md) | 1/2/4 vCPU QEMU. Le profil UTM livré en utilise un, avec VirtIO et polling. Les traces distinguent le premier refus AArch64/512 resté inexpliqué du défaut de réservation corrigé. [Rapport et archive](../fusion-smp-v0.17.0.md) |
| v0.18.0 · [386e53da7e9abb7ce1ef5e32bd64f5c667c74bb7](https://github.com/NaviStorm/NovaOS/commit/386e53da7e9abb7ce1ef5e32bd64f5c667c74bb7) | Découverte ACPI/PCIe, ressources MMIO/IRQ du pilote EDU isolé, domaine expérimental Intel VT-d et retrait conditionnel. [Découverte](../../kernel/src/hardware/discovery.rs) · [Domaine VT-d](../../kernel/src/hardware/vtd_lab.rs) | Attributions et fautes vérifiées sur les topologies d’essai ; la garantie de retrait exige l’invalidation confirmée. Portée distincte d’une HAL universelle AMD/ARM et d’un poste physique prêt à installer. [Fusion matérielle](../fusion-materiel-v0.18.0.md) |
| v0.19.0 · [1b3bb5a8df767a09fa0671bcfd95e6e734693a3a](https://github.com/NaviStorm/NovaOS/commit/1b3bb5a8df767a09fa0671bcfd95e6e734693a3a) | Pilote NVMe Ring 3, capacités READ/WRITE/FLUSH, courtier, domaine VT-d et montage NovaFS P9. F1 puis publication de l’ancre puis F2 avant confirmation. [Pilote](../../user/nvme_driver.rs) · [Contrat du service](../nvme-phase2-service-bloc.md) | Q35 x86_64 sur namespace virtuel jetable, secteurs de 512 et 4 096 octets. Phase 2 : 45 scénarios, 16 coupures. Phase 3 : cinq scénarios, 12 démarrages, cinq remontages RO. Portée source et profil NVMe explicite. L’archive v0.17.0 conserve son propre numéro. [Qualification finale](../nvme-phase3-v0.19.0.md) · [Versions et composants](../validation/nvme-phase3-compatibilite.json) |

Les arbres Git de l’ABI native (`a70d5f23ae91211dd7a54c429e723f5157600e58`) et de NovaFS (`3379ccd023e821e912130c7413113fe7546070cc`) correspondent à ceux de v0.18.0. L’empreinte des images distribuées est également conservée. Les rôles du micro-noyau, des services, des pilotes et du moteur portable restent distincts. [Contrat commun](../../../AGENTS.md)

## Registre, dépendances et priorités

Les **181 entrées** des trois matrices spécialisées constituent l’inventaire de la réconciliation antérieure au mandat serveur. La matrice serveur précise les obligations désormais approuvées. Une entrée réunit des fonctions lorsqu’elles partagent le même composant et le même contrôle. Chaque ligne décrit l’existant, sa portée, le préalable, le risque déterminant, le niveau recommandé, la preuve future attendue et une option de repli. Les désignations A01 à N11 sont des références de dossier, indépendantes des priorités et des numéros de version.

| Registre | Familles et nombre d’entrées | Questions de décision |
|---|---|---|
| [Socle, matériel, services et fiabilité](socle.md) | A noyau/CPU 13 ; B mémoire 10 ; G matériel 18 ; I services 10 ; N fiabilité 11 | Threads et topologies, mémoire sous pression, effet de la révocation, sécurité des pilotes, supervision, lanceur et preuve de régression. |
| [Données, authentification, stockage et distribution](donnees.md) | C autorisations/données 20 ; D authentification/confiance 14 ; F stockage 16 ; M distribution 13 | Principaux durables, ACL et capacités, administrateur et clés, traitement des supports externes, récupération, chaîne de confiance et mises à jour. |
| [Applications, réseau, interfaces et développement](applications.md) | E ABI/bibliothèques 14 ; H réseau 13 ; J CLI/TUI 8 ; K graphique 14 ; L développement 7 | SDK public, API threads/temps/réseau, distinction des deux TLS, outils manquants, surfaces, fenêtres, widgets et parcours accessibles. |

| Priorité recommandée | Décision de livraison |
|---|---|
| P0 | Fonctionnement sûr ou protection indispensable sur le profil annoncé. Exemple : refus d’une nouvelle soumission DMA après révocation et absence de restitution sans confirmation matérielle. |
| P1 | Nécessaire à l’usage stable promis, comme une procédure de restauration éprouvée ou une console accessible après un défaut réseau. Si la condition de profil s’applique, il faut la couvrir ou redéfinir explicitement le profil. |
| P2 | Importante, avec solution temporaire acceptable sur le profil retenu. Exemple : un outil de confort supplémentaire disposant déjà d’un équivalent documenté. |
| P3 | Amélioration future hors besoin de référence identifié. Exemple : placement NUMA de production pour une stable sur machine uniforme. |

Le [registre de l’audit antérieur](arbitrages.md) conserve les arbitrages tels qu’ils se présentaient avant la décision serveur. La [proposition par version](proposition.md) présente un scénario de v0.20.0 à v0.30.0, des alternatives et les prérequis qui peuvent bloquer un jalon. La **matrice de réconciliation** est la [confrontation des plans avec les livraisons](historique.md), dont chaque ligne porte sa source et son niveau de certitude.

## Autorité des sources et contrôle documentaire

| Nature | Ce qu’elle permet d’affirmer |
|---|---|
| Mandat MOA explicite | L’action ou la contrainte nommée est autorisée dans son périmètre. Une demande d’étude ou un souvenir qualifié d’incertain n’établit pas un calendrier adopté. |
| Proposition MOE | Document de planification soumis ou échange avec la maîtrise d’œuvre. Le commit Git établit l’existence du texte. La ratification des numéros nécessite une décision distincte. |
| Recommandation Codex | Les priorités, solutions temporaires et la nouvelle séquence proposées par cet audit attendent examen MOE et validation MOA. |
| Implémentation | Un chemin exécutable existe dans l’arbre identifié. Les sondes, profils expérimentaux, dépendances hôtes et profils activés par défaut sont indiqués séparément. |
| Qualification | Un rapport relie version, environnement, scénario et résultat. Une ancienne preuve demeure attachée à son arbre et à son modèle de panne. |
| Publication | Fusion vérifiée sur main pour les sources ; pour une distribution, artefact identifié et manifeste correspondant. Une archive v0.17.0 ne devient pas v0.19.0 avec le dépôt. |

Le corpus suivi de la révision publiée comprend 89 Markdown et 107 résultats JSON sous `novaOS/docs/`, 52 Markdown, 99 JSON et 12 JSONL sous `novaFS/docs/`, les tableaux de validation, les docs et manifestes des workspaces, le catalogue utilisateur et les manifestes internes des 11 archives UTM. Les archives de traces et images sont traitées comme les pièces des campagnes déjà exécutées. Le dossier d’audit antérieur contient 13 Markdown au commit `a602051c9f3f1119552dd51a6b58ffc64285c556`, sur la base publiée v0.17.0. [Audit précédent figé](https://github.com/NaviStorm/NovaOS/tree/a602051c9f3f1119552dd51a6b58ffc64285c556/novaOS/docs/audit-v0.30.0) · [Journal des versions](../../../CHANGELOG.md) · [Composants](../../../components/README.md)

Les vérifications de l’audit antérieur portaient sur les références et différences Git, les manifestes, l’existence des cibles de liens, la couverture des matrices et le statut des affirmations. Les contrôles ont retrouvé 181 identifiants uniques dans 14 familles, 16 lignes historiques comportant chacune une référence et 13 tranches futures dans le scénario antérieur. Les liens locaux et les objets Git visés, dont les 11 références indirectes de la matrice historique, ont été résolus. Les résultats système cités proviennent des rapports antérieurs. Aucune nouvelle exécution de VM, de benchmark ou de suite NovaFS/SMP n’entre dans cet audit. Le mandat serveur fixe désormais le produit et ses plateformes. L’affectation des versions intermédiaires et l’acceptation d’un candidat restent à décider.

Pour la révision serveur, la vérification documentaire couvre les 16 fichiers Markdown de ce changement et leurs liens. Elle a retrouvé les 181 identifiants du registre initial, les deux décisions MOA séparées sur le produit et la GUI, les onze règles de surprotection et les 26 commandes du mandat. Les seize étapes ont chacune deux cases de résultat, toutes à l’état « Non exécuté ». Les comptes et parcours sont vérifiables dans les matrices liées en tête du dossier.
