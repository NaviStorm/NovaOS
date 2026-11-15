# NovaOS — Dossier de décision pour la première stable v0.30.0

**Situation au 15 novembre 2026.** `origin/main` publie les sources NovaOS v0.19.0 au commit `1b3bb5a8df767a09fa0671bcfd95e6e734693a3a`. Le dernier installateur UTM AArch64 distribué est en v0.17.0 et NovaFS en v0.9.13 P9. La décision en attente porte sur les engagements futurs et leur ordre. Le mandat autorise un audit documentaire sur la branche `codex/roadmap-reconciliation-v0300`. [README publié](../../../README.md#feuille-de-route) · [Distribution](../../../distribution/README.md)

## Ce que doit trancher la MOA

- **Ordre des travaux.** La livraison SMP a avancé la partie multicœur ; l’API permettant à une application de créer et joindre ses propres threads reste ouverte. Le NVMe publié n’a pas livré la bibliothèque système Rust publique que la MOA se rappelle pour v0.19.0. Pour NovaWM, le mandat graphique ancien indiquait v0.18.0 ; la MOA se rappelle également v0.28.1. Le 15 novembre, elle précise ne pas disposer du tableau d’origine daté et demande de traiter v0.19.0 et v0.28.1 comme références à vérifier. [Chronologie, sources et matrice](historique.md)
- **Protection des données.** Les capacités bornent les opérations que peut faire un client. Il reste à décider qui accorde ces capacités, comment l’autorisation persiste, qui détient les clés et ce que signifie « administrateur non autorisé » sur une machine en fonctionnement. L’impact d’une révocation doit être défini pour les handles, les pages déjà lues, les snapshots et les sauvegardes. [Modèle proposé et conditions](donnees.md)
- **Profil de la stable.** Choisir les machines, le périmètre réseau, le parcours graphique NovaWM/NovaUI, les programmes cibles du SDK, les moyens de secours et les limites publiables. Une preuve sur disque Q35 virtuel ne qualifie pas le NVMe du portable. Le choix d’un démarrage physique modifie les prérequis et le coût. [Options et critères par version](proposition.md) · [Décisions à soumettre](arbitrages.md)

## Les versions effectivement publiées

| Version et commit de fusion | Réalisation vérifiée dans la source | Qualification disponible et limites |
|---|---|---|
| v0.17.0 · [010ae3998aa158811b8a05c6d47753cd4589ad9a](https://github.com/NaviStorm/NovaOS/commit/010ae3998aa158811b8a05c6d47753cd4589ad9a) | Démarrage des CPU secondaires sur les deux ISA, ordonnanceur préemptif, IPC multicœur, arrêt coordonné et clôture des réservations DMA. Contexte FPU/SIMD dans le profil explicite `user-fpu`. [Code SMP](../../kernel/src/smp.rs) · [Contrat FPU](../smp-phase2-v0.17.0.md) | 1/2/4 vCPU QEMU. Le profil UTM livré en utilise un, avec VirtIO et polling. Les traces distinguent le premier refus AArch64/512 resté inexpliqué du défaut de réservation corrigé. [Rapport et archive](../fusion-smp-v0.17.0.md) |
| v0.18.0 · [386e53da7e9abb7ce1ef5e32bd64f5c667c74bb7](https://github.com/NaviStorm/NovaOS/commit/386e53da7e9abb7ce1ef5e32bd64f5c667c74bb7) | Découverte ACPI/PCIe, ressources MMIO/IRQ du pilote EDU isolé, domaine expérimental Intel VT-d et retrait conditionnel. [Découverte](../../kernel/src/hardware/discovery.rs) · [Domaine VT-d](../../kernel/src/hardware/vtd_lab.rs) | Attributions et fautes vérifiées sur les topologies d’essai ; la garantie de retrait exige l’invalidation confirmée. Portée distincte d’une HAL universelle AMD/ARM et d’un poste physique prêt à installer. [Fusion matérielle](../fusion-materiel-v0.18.0.md) |
| v0.19.0 · [1b3bb5a8df767a09fa0671bcfd95e6e734693a3a](https://github.com/NaviStorm/NovaOS/commit/1b3bb5a8df767a09fa0671bcfd95e6e734693a3a) | Pilote NVMe Ring 3, capacités READ/WRITE/FLUSH, courtier, domaine VT-d et montage NovaFS P9. F1 puis publication de l’ancre puis F2 avant confirmation. [Pilote](../../user/nvme_driver.rs) · [Contrat du service](../nvme-phase2-service-bloc.md) | Q35 x86_64 sur namespace virtuel jetable, secteurs de 512 et 4 096 octets. Phase 2 : 45 scénarios, 16 coupures. Phase 3 : cinq scénarios, 12 démarrages, cinq remontages RO. Portée source et profil NVMe explicite. L’archive v0.17.0 conserve son propre numéro. [Qualification finale](../nvme-phase3-v0.19.0.md) · [Versions et composants](../validation/nvme-phase3-compatibilite.json) |

Les arbres Git de l’ABI native (`a70d5f23ae91211dd7a54c429e723f5157600e58`) et de NovaFS (`3379ccd023e821e912130c7413113fe7546070cc`) correspondent à ceux de v0.18.0. L’empreinte des images distribuées est également conservée. Les rôles du micro-noyau, des services, des pilotes et du moteur portable restent distincts. [Contrat commun](../../../AGENTS.md)

## Registre, dépendances et priorités

Les **181 entrées** des trois matrices spécialisées couvrent les quatorze domaines du mandat. Une entrée réunit des fonctions lorsqu’elles partagent le même composant et le même contrôle. Chaque ligne décrit l’existant, sa portée, le préalable, le risque déterminant, le niveau recommandé, la preuve future attendue et une option de repli. Les désignations A01 à N11 sont des références de dossier, indépendantes des priorités et des numéros de version.

| Registre | Familles et nombre d’entrées | Questions de décision |
|---|---|---|
| [Socle, matériel, services et fiabilité](socle.md) | A noyau/CPU 13 ; B mémoire 10 ; G matériel 18 ; I services 10 ; N fiabilité 11 | Threads et topologies, mémoire sous pression, effet de la révocation, sécurité des pilotes, supervision, lanceur et preuve de régression. |
| [Données, authentification, stockage et distribution](donnees.md) | C autorisations/données 20 ; D authentification/confiance 14 ; F stockage 16 ; M distribution 13 | Principaux durables, ACL et capacités, administrateur et clés, traitement des supports externes, récupération, chaîne de confiance et mises à jour. |
| [Applications, réseau, interfaces et développement](applications.md) | E ABI/bibliothèques 14 ; H réseau 13 ; J CLI/TUI 8 ; K graphique 14 ; L développement 7 | SDK public, API threads/temps/réseau, distinction des deux TLS, outils manquants, surfaces, fenêtres, widgets et parcours accessibles. |

| Priorité recommandée | Décision de livraison |
|---|---|
| P0 | Fonctionnement sûr ou protection indispensable sur le profil annoncé. Exemple : refus d’une nouvelle soumission DMA après révocation et absence de restitution sans confirmation matérielle. |
| P1 | Nécessaire à l’usage stable promis, comme une procédure de restauration éprouvée ou un parcours graphique utilisable. Si la condition de profil s’applique, il faut la couvrir ou redéfinir explicitement le profil. |
| P2 | Importante, avec solution temporaire acceptable sur le profil retenu. Exemple : plusieurs écrans si un écran unique est supporté et documenté. |
| P3 | Amélioration future hors besoin de référence identifié. Exemple : placement NUMA de production pour une stable sur machine uniforme. |

Le [registre séparé des exigences sans affectation](arbitrages.md) regroupe les décisions ouvertes et conserve les deux reports explicites. La [proposition par version](proposition.md) présente un scénario de v0.20.0 à v0.30.0, des alternatives et les prérequis qui peuvent bloquer un jalon. La **matrice de réconciliation** est la [confrontation des plans avec les livraisons](historique.md), dont chaque ligne porte sa source et son niveau de certitude.

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

Les vérifications de cet audit portent sur les références et différences Git, les manifestes, l’existence des cibles de liens, la couverture des matrices et le statut des affirmations. Les contrôles ont retrouvé 181 identifiants uniques dans 14 familles, 16 lignes historiques comportant chacune une référence et 13 tranches futures dans le scénario proposé. Les liens locaux et les objets Git visés, dont les 11 références indirectes de la matrice historique, ont été résolus. Les résultats système cités proviennent des rapports antérieurs. Aucune nouvelle exécution de VM, de benchmark ou de suite NovaFS/SMP n’entre dans cet audit. La décision à consigner est la combinaison **profil stable, priorités, affectations approuvées, critères de sortie, options reportées et propriétaire de la qualification**.
