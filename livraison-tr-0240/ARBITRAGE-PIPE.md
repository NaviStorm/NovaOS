# Décision demandée pour le lecteur tr

**Proposition TR soumise à validation le 16 novembre 2026.**

| Valeur | Proposition pour tr |
|---|---|
| Indice du catalogue ELF et discriminant interne du rôle lecteur | **22** (`Role::Tr = 22`) |
| Identité d’installation | **`0x4e4f_5641_0000_0116`** |
| Identifiant public de lancement du lecteur | **`PIPE_TR = PIPE_OPEN + 9 = 0x1_0000_0009`** |

`PIPE_TR` serait un argument de `tty::SPAWN` (`0x10042`). Le registre ABI v1 conserverait ses 48 sélecteurs. Ces valeurs sont proposées. Leur affectation et la modification du lanceur partagé attendent une validation MOA/MOE distincte.

Autoriser, dans une intégration distincte, la forme de premier plan `cat fichier | tr [OPTION]... SET1 [SET2]`. Valider les arguments avec le parseur du composant avant la résolution de la source. Les demandes documentaires doivent se terminer avant tout accès métier.

## Motif du refus actuel

À la base `2e3188dee6eaef859becca3919bd0d53a20674bb`, le parseur de nsh reconnaît deux lecteurs. L’exécuteur propose `Count` et `FirstLine`. Le noyau lie une demande de lancement à une image précise en utilisant le rôle. Une demande pour wc ou head lancerait cet autre ELF. Le contrôleur de nsh détient MANAGE. Il ne permet pas de transférer RECEIVE à un ELF arbitraire. Le refus de la grammaire précède la création du pipe et des enfants. [Parseur](../components/shell/parser.rs) · [Exécuteur](../components/shell/executor.rs) · [Admission](../novaOS/kernel/src/sched/process/ipc/spawn.rs)

## Coordination des identifiants

Les références de main et des branches déjà publiées ont été lues directement sur origin. Pour cut et cmp, les worktrees locaux comportaient également des documents en cours. Le contrôle inclut leurs modifications suivies et leurs nouveaux fichiers d’admission. Les mandats des deux commandes demandent une coordination sans préattribuer de chiffres.

| Référence vérifiée | Indice ELF | Identité d’installation (suffixe) | Argument de pipe |
|---|---:|---:|---|
| main `2e3188d` : cat, wc, head | 2, 11, 3 | 0103, 010c, 0104 | +1, +2, +3 |
| main : témoin SDK et SAMPLE | 15, 16 | ffff0001, 00000020 | — |
| uniq `0539455`, intégration de sort | 17, 18 | 0111, 0112 | +4 lecteur uniq, +5 producteur sort |
| sha256sum `1fdce0e` | 19 | 0113 | +6 lecteur |
| awk, proposition non intégrée | 17 proposé, en conflit avec uniq | à coordonner | +4 proposé, en conflit avec uniq |
| grep et sed | admission en arbitrage | — | aucune valeur concurrente trouvée pour TR |
| cut, worktree `b9ddb9e` au premier contrôle | pas de valeur publique à ce stade | — | réservation non communiquée au chantier au premier contrôle |
| cmp, worktree `beb1b2d` au premier contrôle | son document ADMISSION demande une attribution | — | aucune valeur attribuée dans ce document au premier contrôle |

TR propose 22/0116/+9 pour laisser 20/0114/+7 et 21/0115/+8 disponibles aux chantiers parallèles. Aucune affectation de ces deux triplets n’est décidée par TR. Sur une base où 17 à 21 n’auraient pas tous été intégrés, utiliser un catalogue à indices explicites ou insérer des emplacements inoccupés qui refusent le lancement. Une simple insertion dans la liste compacte déplacerait les identités existantes. Réconcilier le registre avant fusion et vérifier à la construction que l’indice 22 désigne le SHA de l’ELF tr pour l’architecture.

[Identités du main](../novaOS/abi/installation.rs) · [Attributions uniq et sort](https://github.com/NaviStorm/NovaOS/blob/0539455d3d32c865d5b3deafc943fffe4bf13402/novaOS/abi/terminal.rs) · [Attribution sha256sum](https://github.com/NaviStorm/NovaOS/blob/1fdce0e4d684ed26e91a43d0d180045388dddcf2/novaOS/abi/installation.rs) · [Proposition antérieure awk](https://github.com/NaviStorm/NovaOS/blob/b151a390920d0d5979ce850ff32fd80ad4b18891/components/awk/proposal/admission-lecteur.diff)

## Contrat proposé à l’intégrateur

- Dans `components/shell/parser.rs`, admettre la paire exacte cat → tr et conserver les arguments littéraux et les guillemets. Dans `script.rs` et `executor.rs`, analyser les arguments avant tout effet, ajouter le lecteur TR et envoyer son vrai bloc argv. Nsh sélectionne et délègue la source exclusivement au producteur. **Cat ouvre et lit le fichier dans sa délégation. Tr lit uniquement les octets du pipe.**
- Dans les manifestes utilisateur et l’assemblage du catalogue, inscrire le véritable ELF tr dans le catalogue après décision sur les trois valeurs. Contrôler à la construction que chaque nom et chaque identifiant restent associés au bon ELF. Préserver les sélecteurs v1 existants, les identités historiques et le format P9.
- Dans le lanceur, reconnaître `PIPE_TR`, lier `Role::Tr` à l’image 22, réutiliser le contrôle du parent et du producteur ainsi que `pipe_attach`. Exiger zéro pour les deux canaux fichier, une extrémité RECEIVE non délégable et seulement OUTPUT|ERROR pour le terminal. Refuser le lancement métier par l’indice numérique seul. La consultation documentaire peut utiliser le profil existant sans capacité métier.
- Réserver à cat la délégation de lecture du fichier et l’écriture dans le pipe. Le lecteur reçoit seulement RECEIVE et les sorties terminal. Vérifier le parent propriétaire, le PID producteur, le sens de l’extrémité et l’unicité du lecteur avant publication du processus.
- Transmettre la sélection C ou UTF-8 dans les données privées de lancement. Actuellement, l’adaptateur ordinaire sélectionne C. Le moteur accepte déjà les deux profils. Le catalogue ARC-026 sélectionne séparément la langue documentaire suivant LANG.
- Exécuter le lecteur avec sa configuration 2, fermer l’extrémité à EOF ou à la première erreur et rendre le statut natif. Après un échec du deuxième lancement, retirer le pipe avant d’attendre cat. Collecter exactement une fois chaque enfant démarré.
- Rejouer sur les quatre profils les 3 644 cas, comparer les octets à la sortie du filtre avant rendu terminal et injecter fermeture de sortie, révocation avant et pendant RECEIVE et échec du second lancement. Relever les deux PID et statuts, les trames, l’absence de capacité fichier et la restitution des ressources.

La recette actuelle fournit le témoin minimal en vrai nsh et les résultats séparés du moteur dans le témoin SDK. Les durées d’admission et de transport de la commande de production restent à mesurer après cette décision.
