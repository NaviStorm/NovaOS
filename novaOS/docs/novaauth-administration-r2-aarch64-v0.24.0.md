# NovaAuth R2 — Témoin Alice/Bob AArch64

22 novembre 2026. **Le témoin ciblé réussit sur AArch64/P9-512 et
AArch64/P9-4096.** Cette qualification ne couvre ni toutes les opérations R2,
ni les neuf scénarios de sécurité complémentaires, ni la distribution ordinaire.
Aucun essai x86_64 n'a été lancé dans cette campagne.

## Sources et périmètre

- Départ : `b88a613b13895a38384cdcd9ea55fe294308bbdb`.
- Correction du terminateur série : `e9af8863fd45de7c605ec7f9fdf8d309445bfc71`.
- Sources invitées finales et marqueurs QEMU :
  `35156744e89ad3868fe0d06e3af0217e2ca4ced1`.
- Contrôle hôte strict du préfixe et du PID :
  `c6716de119d45e4fbe65185726206254f89b821c`.

Les deux essais finaux utilisent les mêmes ISO, noyau et registre. P9/512 a été
exécuté avec le banc `3515674`, puis sa trace a été relue avec le contrôle plus
strict de `c6716de` : exactement trois marqueurs complets, tous du PID d'Alice,
aux statuts attendus. Ce dernier commit ne change aucun octet invité. P9/4096
emploie directement ce contrôle strict.

QEMU 11.1.3, TCG, deux vCPU, 256 Mio, réseau désactivé. Chaque essai repart d'une
copie neuve de sa genèse P9, avec une nouvelle autorité indépendante propre au
parcours ; aucune restauration de cette autorité pendant le parcours. Les
12 fichiers des deux genèses de génération 23 ont été relus contre leur
manifeste antérieur avant copie. Les ELF administratifs installés restent ceux
des fixtures qualifiées : les sources `components` sont inchangées depuis leur
construction. Les commandes QEMU intégrales et empreintes initiales/finales sont
conservées dans chaque résultat.

Le contrôle préalable extrait `limine.conf` et le noyau réellement embarqué :
profil unique `novaauth-sessions`, noyau identique à l'artefact du banc.
Toolchain : Rust/Cargo 1.99.1, LLVM 23.1.1, cible
`aarch64-unknown-none-softfloat`, release, locked, offline,
`novaauth-sessions-lab`. Les avertissements de construction sont conservés.

## Échecs démontrés et corrections

Le premier P9/512 atteint la révocation et le reçu terminal, mais le collecteur
attend à tort un retour à la ligne immédiatement après le marqueur de statut.
La trace contient réellement :

```text
NXR2_1_0__FINDEBU
G pid=11 NAR2 Bob fermetnsh> ure acquittee
```

`e9af886` reconnaît le terminateur explicite `__FIN`, sans transformer un statut
absent ou incomplet en succès. Le rejeu P9/512 réussit. Le premier P9/4096 révèle
ensuite un entrelacement **à l'intérieur** du marqueur :

```text
DEBUGNXR2_ pid=1_0__FIN11 NAR2 Bob ferm
eture acquittee
```

L'analyse statique confirme que `hal::log` sérialise ses propres diagnostics,
alors que `CONSOLE` utilise un autre chemin série, avec une garde locale au CPU.
Les traces de Bob peuvent donc s'intercaler dans l'echo d'Alice. Ces deux échecs
restent des échecs complets du banc ; le drainage final n'y est pas qualifié.
La présence d'un refus ou d'un reçu avant l'arrêt ne les convertit pas en succès.

La correction finale porte exclusivement sur l'export
`--administration-ticket-vivant`. Pour l'unique argument canonique
`NXR2_<étape>_<statut>__FIN`, long d'au plus 64 octets, l'echo du banc emploie
`DEBUG_WRITE` **déjà disponible**, sans sortie console concurrente pour ce
marqueur. Le noyau fournit le PID ; son quota existant de 16 diagnostics reste
inchangé. Les erreurs, notamment LIMIT, sont propagées. L'echo ordinaire et les
arguments non canoniques conservent leur chemin. Trois messages sont observés
chez Alice dans chaque succès final.

Le collecteur exige une ligne `DEBUG pid=...` complète, la bonne étape, puis
l'égalité entre ce PID et le shell effectivement lié aux flux de l'enfant.
Un préfixe ressemblant à un PID noyau à l'intérieur du texte est refusé. Les
sorties des ELF restent collectées séparément et comparées octet par octet ;
aucune reconstruction heuristique du marqueur corrompu n'est utilisée.

Ce changement ne corrige ni ne modifie le terminal ordinaire. Aucune nouvelle
capacité, racine IPC, primitive ou temporisation n'est introduite. Aucun contenu
de ticket ni secret n'est ajouté aux marqueurs.

## Matrice réellement exécutée

| Profil / essai | Sources invitées | Résultat | Marge ticket | TLS invité |
|---|---|---|---:|---:|
| AArch64/512 — 01 | b88a613 | Échec collecteur, LF entrelacé | 3 936 ms observées | 3 348 ms |
| AArch64/512 — 02 | b88a613, collecteur e9af886 | Succès intermédiaire | 3 936 ms | 3 320 ms |
| AArch64/4096 — 01 | b88a613, collecteur e9af886 | Échec collecteur, marqueur entrelacé | 4 320 ms observées | 2 628 ms |
| AArch64/512 — 03 | 3515674 | **Succès final** | **3 932 ms** | **3 324 ms** |
| AArch64/4096 — 02 | 3515674, collecteur c6716de | **Succès final** | **4 296 ms** | **2 624 ms** |

Chaque nouvelle tentative répond à un défaut précisément observé. Aucun essai
supplémentaire n'a été lancé après validation des deux profils finaux.

Dans les deux succès finaux :

- Alice PID 5 et Bob PID 11 ont deux identités d'instance et deux sessions
  distinctes, observées sur les routes noyau ; authentifications natives à 0.
- Bob reçoit un ticket valide non consommé. La consultation locale constate
  sa révocation ; son véritable appel SPAWN reçoit **REVOKED (10)** avant expiry.
- Aucun enfant ni admission distante d'enfant n'existe pour Bob.
- Alice exécute préparation, confirmation puis consultation de la même opération
  avec trois vrais ELF `usermod`, PID 21/22/23. Statuts natifs et shell 0,
  stdout 134/70/70 octets exactement conformes, stderr vide.
- Une seule mutation administrative est présente. Le reçu terminal de 128 octets
  et sa signature Ed25519 de 64 octets sont vérifiés indépendamment ; masque
  d'acquittement 7 et époque de Bob `RETIREE` dans l'autorité durable.
- L'invité a lui-même annoncé la vérification du reçu. La relecture hôte de la
  base d'autorité complète les preuves invitées, sans les remplacer.

31 tests hôtes finaux réussissent en 0,915 s : événements, statuts, collecteur,
séparation des flux, rendez-vous et transport. Les tests exécutent également le
corps Rust généré de l'echo avec des effecteurs témoins, y compris l'échec de
DEBUG, les bornes et la conservation des autres arguments. Les premiers
échecs d'invocation/import et d'ancrage du générateur sont archivés ; ils ne sont
pas des résultats QEMU. Les assertions historiques restent conservées.

## Chronologie et points d'effet

Toutes les valeurs suivantes sont des millisecondes **invitées**. Les ticks sont
convertis à 250 Hz. Aucune soustraction entre horloges hôte et invitée.

| Événement | P9/512 final | P9/4096 final |
|---|---:|---:|
| Présentation du ticket, REVOKED | 14 072 | 10 004 |
| Expiration naturelle du ticket | 18 004 | 14 300 |
| Marge restante | 3 932 | 4 296 |
| Entrée Confirm | 13 888 | 9 880 |
| Début échange de publication | 14 232 | 10 092 |
| Retour réussi de l'échange | 17 116 | 12 504 |

Le refus local est observé **avant** l'acquittement durable terminal. Le témoin
démontre ces deux étapes successives ; il ne prétend pas présenter ce ticket
après réception du reçu distant. Les échéances des défis, des baux Alice/Bob et
du TLS sont détaillées dans la synthèse JSON. Leurs durées restent respectivement
1,5 s, 60 s et 10 s ; le ticket reste limité à 5 s. Aucun renouvellement implicite.

## Ressources et réserves

Pic mesuré : quatre racines maximum par acteur, dont Alice deux et Bob quatre.
La cinquième demande reste refusée. Les mesures avant/actif/après du signataire
sont identiques : 4 racines, 6 handles, 0 attente, 59 pages, 64 pages de pile,
31 cadres IPC globaux. Registre : 4 racines, 7 handles, 0 attente, 139 pages,
64 pages de pile, 31 cadres IPC globaux avant/après.

Les deux shells finissent à zéro racine, handle, attente, page, pile et thread.
Le drainage final donne zéro endpoint, nœud, espace et cadre IPC. Le processus
Bob, après fermeture acquittée de sa session, attend le retrait de sa famille
supervisée (125) : il n'est pas présenté comme une sortie normale 0. Les services
bloc et stockage terminent à 0, avec retrait réussi et arrêt SMP normal.
Ces points de mesure n'affirment pas un maximum physique de toute allocation
intermédiaire ni un délai garanti de restitution du disque.

Réserves conservées : seuil mémoire instrumenté AArch64 de **301 cadres** et
reproductibilité des compilations entre environnements. Aucun seuil n'a été
relevé. La réserve de reproductibilité n'est pas levée par ces constructions.

Le diff depuis b88a613 est vide pour `components`, `novaOS/kernel`, `novaOS/abi`
et `novaFS`. Les modifications invitées proviennent exclusivement du générateur
QEMU. Les neuf contrôles ABI et les quatre ELF historiques ne sont pas rejoués
ici ; aucune nouvelle qualification de ces suites n'est revendiquée.

## Applicabilité x86_64

La preuve historique P9/512 porte sur l'arbre invité
`e7ddc8ecea2220ce205a443dcd01de4203f6dbb8` et le banc
`a7f86df67f28882d4ae17345d46ab177ff3ac182` : marge 996 ms, selon le
[rapport conservé](novaauth-administration-r2-effecteurs-v0.24.0.md).
Depuis, le déclenchement de Bob, le rendez-vous avec la confirmation d'Alice,
les sondes et la collecte des statuts ont changé. **Cette preuve ne suffit pas
à qualifier P9/512 sur le candidat courant : un rejeu ciblé est nécessaire.**
Il n'est pas exécuté, conformément au mandat.

Les deux succès x86_64/P9-4096 acceptés restent archivés à leurs révisions
[documentées](novaauth-administration-r2-stabilisation-banc-v0.24.0.md).
Ils démontrent le protocole de révocation avec les binaires alors testés, mais
n'exercent pas le nouveau marqueur QEMU. Une affirmation sur les quatre profils
avec les **binaires finaux exacts** nécessiterait aussi sa validation x86_64.
Cette campagne ne déclare donc pas le candidat globalement qualifié sur quatre
profils. Seize fichiers de résultats/traces/autorités historiques pertinents ont
été relus contre leurs manifestes, sans relancer ces invités.

## Preuves et suite autorisée

Archive durable :
`/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/aarch64-b88a613`.
Artefacts privés exacts : `OS-worktrees/artefacts-prives-administration-0240-r2/`
avec les répertoires `aarch64-b88a613` et `aarch64-3515674`.
Aucune clé ou ELF embarquant des secrets de laboratoire n'est ajouté à Git.
Les preuves antérieures, les échecs, les images initiales et finales sont conservés.

[Synthèse et mesures](validation/novaauth-r2-aarch64-20261122.json) ·
[Manifeste des preuves](validation/novaauth-r2-aarch64-20261122.sha256).

Le travail s'arrête à ce compte rendu. Les neuf scénarios supplémentaires et les
opérations R2 restantes ne sont pas lancés. Les utilitaires restent en pause.
Aucune fusion, aucun tag, aucune activation ordinaire ni clôture v0.24.0.

## Verrou, durées et vérification des archives

Horodatages UTC, Europe/Paris = UTC+1. Aucun conflit de planification observé.
Chaque essai a acquis atomiquement le verrou commun après vérification des
processus, puis libéré son propre verrou après nettoyage.

| Essai | Acquisition UTC | Libération UTC | Attente s | Banc s |
|---|---|---|---:|---:|
| P9/512 — 01 | 16:03:02.288576+00:00 | 16:04:40.406565+00:00 | 0.000597 | 98.071 |
| P9/512 — 02 | 16:06:20.121623+00:00 | 16:06:57.663629+00:00 | 0.000349 | 37.498 |
| P9/4096 — 01 | 16:07:22.840026+00:00 | 16:08:58.483083+00:00 | 0.000376 | 95.598 |
| P9/512 — 03 | 16:13:52.060264+00:00 | 16:14:29.475549+00:00 | 0.000355 | 37.370 |
| P9/4096 — 02 | 16:15:25.815613+00:00 | 16:15:54.355578+00:00 | 0.000392 | 28.494 |

Attente totale **0.002069 s**, exclue du budget ; banc **297.032 s** ; préparation, développement, diagnostic et rapport hors intervalles du banc **965.899 s**. Les deux compilations totalisent **45.048 s**, déjà incluses dans la préparation. Travail actif au relevé d'archivage : **1262.931 s**, sous les 3 600 s autorisées. Le manifeste public et la publication suivent ce relevé.

Artefacts finaux SHA-256 :

- `aarch64.iso` : `9ba0ff28a6137b11ff453410447d7bc539d5f202ce175f0ecd0f44736630778c`.
- `aarch64-kernel.elf` : `9a25729b854ce31f82f4503136ef50bef2f0945a8af0744cf52720376d37d405`.
- `aarch64-registre.elf` : `8d5ffa24f316fa6123297d9d1a411b9549b1f2198dbb6d53f726ea168dd69e2a`.

Archive privée `aarch64-b88a613` : 51 fichiers relus deux fois ; manifeste `5e081130558ebd6cfc1dae0b5cdfd9224b07f6157082997a0eefd0e21d8cf24c`.

Archive privée `aarch64-3515674` : 39 fichiers relus deux fois ; manifeste `59ab6d0a7af87224043037b750faf6736770057cca4a51d25f5678cb19ccb83e`.

Archive de qualification : **250 fichiers vérifiés par double lecture SHA-256**. Empreinte du manifeste public : `89c11d95b86f15359e94951faa4f7225c8be8cecc48cca2d8244e2ecc00b8c9f`. La copie versionnée du manifeste est identique à celle de l’archive durable.
