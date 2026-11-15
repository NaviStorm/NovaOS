# NovaOS v0.22.0 — Audit des sources intégrées

**Avis technique du 16 novembre 2026.** Les trois phases remplissent leurs critères sur les profils qualifiés. Leur clôture peut être soumise à la MOA/MOE. Cette décision concerne les sources intégrées au commit `b83ebb1dd6eb5aeaa2f67bf478112acfd0c72556`. Le noyau affiche v0.20.0 et l’image UTM distribuée est celle de v0.17.0. La décision officielle sur le jalon reste ouverte.

## Fusion et preuves

| Référence | SHA Git vérifié |
|---|---|
| Ancien `origin/main`, premier parent | `7e705d37989bf451b486d1fa6c97e00e4da6e751` |
| Candidat distant de phase 3, second parent | `dec941d25302ffcd4619aa563ecabb3894257e74` |
| Fusion publiée sur `origin/main` | `b83ebb1dd6eb5aeaa2f67bf478112acfd0c72556` |
| Arbre commun au candidat et à la fusion | `5f67cf474314b3cf296bdcc602a53a9ba429c5f8` |

La fusion conserve les trois commits de phase 3 `736d04c7ba968cc97e13c30bcefb7bec19a0c8a5`, `a0139fd6368300fba08906442987941aff9a3597` et `dec941d25302ffcd4619aa563ecabb3894257e74`. Le worktree d’intégration et celui de la branche documentaire sont sous `OS-worktrees/`. Le dépôt principal a conservé son HEAD local `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e`, ses changements de `.gitignore` et ses deux liens non suivis. Les 18 worktrees préexistants et les branches historiques sont conservés. L’audit documentaire part de la fusion du code, sur `codex/cloture-0220`.

| Dossier de preuve durable | Contrôle réalisé le 16 novembre | SHA-256 de l’archive complète |
|---|---|---|
| Phase 1, configuration | 498 membres relus, dont le manifeste. 48 images décompressées comparées aux empreintes des cellules ainsi que leurs traces série. | `e5824324e3ae59bfdc96c1caef2ffedae4202d524dfa3735c8caaf4a78bb0d89` |
| Phase 2, journalisation | 1 724 membres décompressés, comparés aux originaux et au manifeste. | `60baf4334d844a99d0b00c8a4868dcd93305042cde6663a6d310d5f6c857101b` |
| Phase 3, restrictions | 642 membres décompressés, comparés aux originaux et au manifeste. | `2452e903452c90b1237d190e7090e4ae378f9c405c6a55ab88d575498889d2f9` |
| Contrôles sur le commit de fusion | 32 membres relus dans les originaux et dans une archive distincte, y compris les sondes et les journaux des quatre ELF. | `166d7781e78710fb78f4f4417482a943a19b5b006739ac1cdbb3ecd68685c95e` |

Les chemins, les SHA des manifestes et les membres justificatifs figurent dans les annexes. Les archives locales préservent les originaux et restent exposées à la perte de ce Mac. Un extrait portable des traces des quatre ELF accompagne l’audit. [Phase 1](audit-0220-phase1.md) · [Phase 2](audit-0220-phase2.md) · [Phase 3 et stockage](audit-0220-phase3-stockage.md) · [Quatre ELF historiques](validation/elf-0220-rejeu.tar.gz)

## Matrice d’acceptation

Les identifiants des annexes relient exigence, code, essai, architecture, verdict et archive. `VALIDÉ AVEC LIMITES` désigne une preuve dont le modèle de panne, le profil ou la distribution restreint la portée. Les lignes futures restent hors du dénominateur des contrôles de v0.22.0. Les effectifs ci-dessous comptent des exigences, indépendamment du nombre de démarrages.

| Périmètre | VALIDÉ | VALIDÉ AVEC LIMITES | NON VALIDÉ | REPORTÉ |
|---|---:|---:|---:|---:|
| [P1-01 à P1-10, configuration persistante](audit-0220-phase1.md) | 8 | 2 | 0 | 0 |
| [P2-01 à P2-11, journalisation](audit-0220-phase2.md) | 7 | 4 | 0 | 0 |
| [P3-01 à P3-10, restrictions](audit-0220-phase3-stockage.md) | 8 | 2 | 0 | 0 |
| [FS-01 à FS-04, stockage et récupération](audit-0220-phase3-stockage.md) | 2 | 2 | 0 | 0 |
| [ABI-01 à ABI-07 et LIV-01 à LIV-04, ABI et distributions](audit-0220-abi-livraisons.md) | 10 | 1 | 0 | 0 |
| **Sources et contrôles de livraison, 46 lignes** | **35** | **11** | **0** | **0** |
| Travaux futurs FUT-01 à FUT-10 ci-dessous | 0 | 0 | 0 | 10 |

- **Configuration.** Le redémarrage après `service disable 9` laisse 10 en attente avec la dépendance 9 explicite, pendant que `nsh` accède à P9. `service enable 9` puis un redémarrage rétablissent la réponse NVFS de 9, puis le service 10. Les traces distinguent l’état de l’instance, la politique et les générations. [Matrice de phase 1](audit-0220-phase1.md)
- **Journal.** Les événements des deux démarrages sont retrouvés par BootID. Quota, rotation, arrêt du démon et indisponibilité de son stockage laissent la supervision opérationnelle. Avant acquittement, la file est volatile. Après purge ou éviction, l’identifiant d’événement peut sortir de la fenêtre de dédoublonnage. [Matrice de phase 2](audit-0220-phase2.md)
- **Restrictions.** La phase 3 couvre la réserve historique de phase 1 sur le retrait individuel d’une capacité. Le plafond du catalogue, le masque stocké et le masque de l’incarnation sont contrôlés séparément. Les démarrages manuel et automatique refusent une décision de sécurité invérifiable. Le service 16 restreint puis restauré laisse intacts les réglages de 9 et 10. [Matrice de phase 3](audit-0220-phase3-stockage.md)
- **Compatibilité.** Les neuf contrôles ABI sur la fusion couvrent 48 sélecteurs distincts, leurs numéros, le dispatch et les interfaces générées. Quatre ELF v0.20.0 d’archive ont effectivement été exécutés sans recompilation. Le diff entre v0.21.0 et la fusion est vide pour `novaFS/`, le registre des appels, les sorties ABI générées et `distribution/`. Les essais historiques P8 conservent la génération 43 en lecture, puis retrouvent l’écriture confirmée en génération 44. Les nouvelles configurations exigent P9. [ABI et produits distribués](audit-0220-abi-livraisons.md) · [Stockage](audit-0220-phase3-stockage.md)

## Mesures et portée des essais

QEMU 11.1.3 a exercé AArch64 et x86_64 avec 1, 2 et 4 vCPU et secteurs de 512 ou 4 096 octets selon cellule. En phase 1, 48 images de la matrice de configuration ont été relues. La phase 2 compte 50 démarrages dans l’oracle des images, plus les deux rejeux de pagination. La phase 3 compte 96 démarrages qualifiés, dont 56 pour la matrice et les courses. Les annexes identifient le binaire de chaque campagne et les scénarios rejoués après évolution. Seuls les contrôles ciblés et les quatre ELF ont été relancés pour la fusion.

| Mesure | AArch64 | x86_64 | Périmètre |
|---|---:|---:|---|
| Pics de cadres supplémentaires, restauration après restriction | 1 038–1 043 | 1 044–1 051 | Cadres de 4 Kio, ensemble de l’invité P3 |
| Pages privées mappées du démon de journalisation | 276 Kio | 276 Kio | 5 pages hors pile et 64 de pile. Cache interne de 3 072 octets inclus |
| Courtier cache, six profils exerçant l’acteur 11 | 1 696 octets | 1 696 octets | Pic du courtier, retour à zéro au drainage |
| File de publication du gestionnaire | 4 096 octets | 4 096 octets | 32 trames de 128 octets. RSS du gestionnaire isolé non mesuré |
| Latence acquittée depuis nsh, p50 / p95 / p99 | 51,61 / 106,49 / 108,57 ms | 43,98 / 48,97 / 75,72 ms | 33 émissions, 1 vCPU, secteurs 512, saisie et sortie série comprises. p99 = maximum |
| Débit depuis nsh | 16,01 événements/s | 22,17 événements/s | Même parcours, autres invités actifs sur le Mac |
| Intervalle BOOT → construction de ONLINE | 100–208 ms | 380–528 ms | 18 BootID par ISA, avant l’append durable du démon |

Le temps total du démarrage à l’invite, la durée complète de récupération après panne et le débit soutenu de plusieurs émetteurs simultanés restent non mesurés. La table des processus atteint 19 360 octets, dont 256 pour les 32 nouveaux masques. L’occupation maximale réelle des piles n’a pas été instrumentée. Une perte de courant physique, une recette serveur sur machine réelle et le fonctionnement en interface graphique UTM des sources v0.22.0 restent hors de cette qualification. Les arrêts forcés des invités, les erreurs FLUSH simulées et les comparaisons après remontage ont chacun un résultat propre. [Mesures de journalisation](audit-0220-phase2.md) · [Pics détaillés par profil](validation/capacites-services-v0.22.0.json)

## Planificateur de tâches

**Statut décisionnel.** L’affectation à v0.22.0 figurait dans une séquence prospective. ARC-019 laissait les lots intermédiaires soumis à décision. La décision serveur du 15 novembre imposait déjà les tâches planifiées pour v0.30.0 (§13 et recette S07/S10). Les mandats de configuration (§20), journalisation (§23) et restrictions (§22) ont chacun fixé leurs propres critères de phase sans y inclure la réalisation du planificateur. Le 16 novembre, la MOA a rendu explicites la crontab de chaque utilisateur et son échéance avant la stable. Cette décision confirme que l’absence de planificateur ne bloque pas v0.22.0. [Registre MOA, ARC-019 et ARC-024](reconciliation-v0300/decisions.md) · [Feuille de route](reconciliation-v0300/proposition.md) · [Exigences serveur](reconciliation-v0300/exigences-serveur.md)

Les trois mandats consultés ont respectivement les SHA-256 `5b575c0f2d3dc57405f75846bff64ff0eb8928cad8fe8fcd21030c594d5c5da7`, `211a9d9e692ac1bd37fbcf53885d92c49cb6c8409e8f05e04c45f5372b3ed4c2` et `6232b242750a1449e56b1d3c358e3fbf73f49bad5feb48462d683e051abd9b16`. Le mandat initial pour la stable porte le SHA-256 `b19f19a1154519412df599e3b8b787524e44611d1477185a04e0e68df3b78a48`. La correction conserve l’obligation initiale et donne à chaque propriétaire la gestion de sa table depuis `nsh`. [Traçabilité des décisions](reconciliation-v0300/decisions.md)

**Recette future.** Sur chaque ISA, deux utilisateurs attestés créent chacun deux tâches, consultent leur table, en modifient une et suppriment l’autre. Après redémarrage, avant toute reconnexion, les deux versions modifiées doivent s’exécuter sous leurs propriétaires respectifs. Aucune des deux entrées supprimées ne doit produire d’effet ni d’événement de lancement aux échéances observées. Les essais de refus croisé, de droits retirés et de clôture de session comportent leurs propres témoins dans le journal. La proposition v0.24.3 dépend de la qualification des comptes, de NovaAuth, des sessions révocables et des capacités persistantes. L’attribution définitive du lot revient à la MOA/MOE. Le présent audit enregistre le scénario. Les deux colonnes de recette restent **Non exécuté**. [Matrice d’acceptation](reconciliation-v0300/acceptation-serveur.md)

## Surprotection

**Statut décisionnel.** ARC-017 a approuvé la surprotection pour la stable serveur v0.30.0. Le contrat détaille onze résultats et la recette serveur les exerce avec deux comptes, des sessions distinctes et un enfant. Le `tmp` privé, le cache et les retraits de capacités de services fournissent des mécanismes utilisables. Ils ne valident aucune des huit garanties futures ci-dessous. [ARC-017](reconciliation-v0300/decisions.md) · [Contrat, §8](reconciliation-v0300/contrat-autorisations.md) · [Recette serveur](reconciliation-v0300/acceptation-serveur.md)

| Garantie approuvée | Traçabilité vers la recette v0.30.0 |
|---|---|
| Protection attachée à chaque fichier identifié | §8.1, S04. Vérifier l’objet après renommage, y compris par ancien chemin ou handle. |
| Protection récursive des dossiers | §8.2, S04 et S16. Descendants présents, créés ou déplacés, sans déclassification implicite à la sortie de l’arbre. |
| Conservation après redémarrage | §8.3, S04. Vérifier propriétaire et classement avant ouverture. |
| Conservation après sauvegarde et restauration | §8.11, S11/S15/S16. Restaurer données et politique liées, sans jeton de session réutilisable. Vérifier les droits après restauration. |
| Autorisation limitée à une session du propriétaire | §8.4, S04/S16. La deuxième session du même compte doit demander une autorisation distincte. |
| Révocation à la fermeture de la session | §8.5, S04/S16. Tester les handles dérivés, l’enfant et les opérations en attente après fermeture. |
| Interdiction du partage direct de l’original | §8.6–8, S04/S09. Refuser le partage direct, le don de handle et tout export équivalent. L’administration et le shell ne confèrent aucun accès implicite à l’original. |
| Copie indépendante et déclassification explicite par le seul propriétaire | §8.9–10, S04/S16. Le propriétaire crée une copie initialement surprotégée. Une seconde décision désigne la copie à déclassifier. L’original conserve sa protection. |

Les huit exigences figurent dans la feuille de route et ses contrats référencés. Aucun ajout de garantie ni déplacement de jalon n’est requis pour les couvrir. La matrice attribue explicitement **Non exécuté** aux parcours serveur sur les deux ISA. Le choix du stockage de politique, des clés, des autorités de secours et de fraîcheur reste à arbitrer. [Dépendances NovaFS et restauration](reconciliation-v0300/dependances-p9.md)

## Couverture prévue vers la première stable serveur

| ID | Engagement ou report documenté | Recette prévue et statut de livraison |
|---|---|---|
| FUT-01 | Crontab individuelle et exécution hors connexion, avant v0.30.0 | S07/S10 et recette à deux comptes sur les deux ISA. REPORTÉ |
| FUT-02 | Surprotection, sessions et copies du propriétaire | ARC-017, S04/S11/S15/S16 sur les deux ISA. REPORTÉ |
| FUT-03 | Comptes utilisateurs, NovaAuth, ACL persistantes et commande native permissions | S01–S04/S16. Les capacités de maintenance des services ne fournissent pas l’identité d’un utilisateur. REPORTÉ |
| FUT-04 | Réseau IPv4/IPv6 et pare-feu à états | S05/S06. Vérifier séparément accès admis et refusés pour chaque famille. REPORTÉ |
| FUT-05 | Serveur et client SSH, SFTP, administration distante | ARC-020, S08/S09. Vérifier requête réelle, transfert, refus hors racine et révocation. REPORTÉ |
| FUT-06 | Sauvegarde et restauration cohérentes des données, des droits et des retraits | S11/S15/S16, sauvegarde sur stockage distinct. REPORTÉ |
| FUT-07 | Mises à jour signées, échec d’amorçage et retour autorisé | S12–S14. Vérifier santé de l’image et conservation des politiques lors d’un retour. REPORTÉ |
| FUT-08 | Compilateurs natifs Rust, C et C++ | ARC-022. Compiler, lier et exécuter chacun des témoins depuis nsh sur chaque ISA. Le SDK croisé livré couvre une autre épreuve. REPORTÉ |
| FUT-09 | Qualification du serveur texte complet | ARC-014/016, S01–S16 sur les candidats QEMU x86_64 et AArch64. Qualification physique distincte. REPORTÉ |
| FUT-10 | Pile GUI, NovaWM et NovaUI | ARC-015. Report **après** la première stable serveur, sans échéance numérotée. REPORTÉ |

Le statut REPORTÉ porte ici sur la qualification intégrale de chaque engagement futur. Les fonctions de v0.22.0 déjà éprouvées gardent leurs propres verdicts dans les annexes. [Exigences du serveur](reconciliation-v0300/exigences-serveur.md) · [Critères d’acceptation](reconciliation-v0300/acceptation-serveur.md) · [Références matérielles](reconciliation-v0300/contrat-plateforme.md)

## Décision soumise à la MOA/MOE

L’avis favorable porte sur les sources des trois phases et les garanties vérifiées dans leur modèle de panne. Les réserves déterminantes pour l’exploitation sont le refus possible de montage sur ancre P9 déchirée, l’impossibilité de détecter le recul cohérent du support entier, la perte possible d’événements non acquittés, et l’écart entre le code qualifié et l’installateur distribué. La clôture officielle et la fusion de cette documentation nécessitent la décision de la MOA/MOE.
