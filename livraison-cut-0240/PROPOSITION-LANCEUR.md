# Admission de CUT soumise à la MOA/MOE

Contrôle du 16 novembre 2026. Aucune réservation CUT communiquée. Les trois valeurs ci-dessous sont proposées ensemble et attendent validation.

| Chantier | Indice du catalogue ELF | Identité d’installation | Argument du lanceur de pipe |
|---|---:|---|---|
| UNIQ | 17 | `0x4e4f_5641_0000_0111` | `PIPE_OPEN + 4` |
| SORT | 18 | `0x4e4f_5641_0000_0112` | `PIPE_OPEN + 5` |
| SHA256SUM | 19 | `0x4e4f_5641_0000_0113` | `PIPE_OPEN + 6` |
| **CUT proposé** | **20** | **`0x4e4f_5641_0000_0114`** | **`PIPE_CUT = PIPE_OPEN + 7 = 0x1_0000_0007`** |
| CMP | demande d’attribution | demande d’attribution | demande de rôle lecteur |
| TR proposé | 22 | `0x4e4f_5641_0000_0116` | `PIPE_OPEN + 9` |

Le document TR laisse 20/0114/+7 et 21/0115/+8 disponibles aux chantiers parallèles. Le document CMP consulté dans le worktree à `beb1b2d` (ADMISSION.md en cours) demande une décision et décrit aussi une délégation fichier propre à CMP. Le contrôle porte sur les propositions écrites et les attributions connues au moment de la soumission. [UNIQ et SORT](https://github.com/NaviStorm/NovaOS/tree/0539455d3d32c865d5b3deafc943fffe4bf13402) · [SHA256SUM](https://github.com/NaviStorm/NovaOS/tree/1fdce0e4d684ed26e91a43d0d180045388dddcf2) · [Proposition TR](https://github.com/NaviStorm/NovaOS/blob/6352660ab3aa157cbc618b580211c5068f2059c3/livraison-tr-0240/ARBITRAGE-PIPE.md)

## Chemin à intégrer après décision

1. Dans `nsh`, admettre la paire `cat fichier | cut [options]`. Analyser les arguments de CUT avant la sélection du fichier ou l’allocation du pipe. Transmettre ses vrais arguments, la locale et le mode POSIX dans le contrat privé. Traiter les demandes documentaires par ARC-026.
2. `cat` ouvre le fichier dans sa propre délégation, le lit et écrit sur son extrémité SEND. CUT reçoit RECEIVE en configuration 2 et les sorties terminal OUTPUT et ERROR. Ses canaux fichier sont nuls. Le profil lecteur du pipe ne requiert ni accès au clavier ni résolution de nom. Vérifier le parent, le propriétaire, le producteur, le sens et l’unicité du lecteur avant publication.
3. Inscrire l’ELF CUT de chaque ISA à l’indice 20, contrôler son empreinte à la construction et refuser les indices vacants. Le rôle +7 est un argument de lancement. L’ABI v1 possède déjà ses 48 sélecteurs. Le chemin direct `cut -f… fichier` requiert une délégation de lecture distincte, ordonnée par opérande, et le traitement de chaque erreur de fichier.
4. Capturer stdout brut avant rendu et séparer stderr. Le terminal rend des octets de contrôle et les octets invalides sous forme affichable. Qualifier TAB, NUL, erreurs de préparation, EOF, abandon par le lecteur et révocation pendant RECEIVE. Après l’échec du second lancement, retirer le pipe et collecter les enfants. Relever PID, statuts et restitution sur les quatre profils.

La branche CUT fournit le composant, son adaptateur et les témoins. L’attribution commune et l’admission du lanceur sont en attente.
