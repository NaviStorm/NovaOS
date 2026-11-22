# NovaOS v0.24.0 — Audit de clôture NovaAuth R2

Audit du 22 novembre 2026, branche `codex/novaauth-administration-0240-phase1`.
Référence et sources éprouvées : `43e5cf9c9bb69af94902d886a249df8d3979478d`.
Base fonctionnelle : `55838a0863c205087a136fd99ab76fe2d467dd6a`.
La livraison de cet audit ajoute uniquement documentation, matrice et manifeste.

## Décision

**C — Clôture refusée.** Les 36 cellules S1–S9 restent qualifiées dans leur
périmètre QEMU accepté. Les quatre ELF historiques réussissent sur le candidat.
Cela ne termine pas les opérations administratives demandées par R2.

Blocages fonctionnels établis dans les sources :

1. `useradd` existe comme ELF et comme grammaire, mais la création d’utilisateur
   retourne `REFUSED(12)` côté registre : le facteur attesté n’est pas raccordé.
2. `usermod --add-group` et `--remove-group` retournent également `REFUSED(12)`.
   Le moteur possède les mutations ; leur publication et le retrait des droits
   par origine ne sont pas raccordés à ces commandes.
3. La consultation d’un groupe par nom/ID n’a pas de façade native. `groups`
   consulte un utilisateur puis ses appartenances ; la vue privée GroupInfo
   n’est pas exposée par cette commande.
4. Le résultat administratif durable n’est pas récupérable intégralement après
   redémarrage ou éviction du cache local. `Outcome` vérifie le reçu distant,
   mais exige aussi une entrée dans `completed`, tableau volatile de deux
   éléments. À défaut, il conserve `UNCERTAIN(22)` : comportement sûr, mais
   récupération d’issue incomplète. S8 qualifie la perte de réponse **sans
   redémarrage du registre**, pas cette situation.
5. La locale de lancement reste nulle : les messages invités éprouvés sont
   anglais. Les aides françaises existent et passent les tests hôtes ; la
   propagation de la langue reste à raccorder. L’argument `commande man` est
   local à l’ELF ; `man commande` dans nsh consulte un catalogue où ces six
   commandes sont absentes.

Qualifications encore nécessaires : renommage invité réussi et conservation de
l’identité/propriété ; modes script et `nsh -c` des commandes administratives ;
persistance et reprise après interruption/redémarrage des nouvelles mutations ;
restauration après mutations R2, réauthentification effective et contrôle VFS.
Les tests hôtes et les recettes anti-rollback antérieures ne les remplacent pas.

Ces écarts étaient signalés dans les points de passage : ce ne sont pas de
nouvelles régressions produites par l’audit. Création et appartenances figurent
explicitement dans le mandat R2 ; aucun arbitrage de report n’a été trouvé.
La mention antérieure « mandat distinct » ne constitue pas une décision MOA/MOE
de les retirer des critères. Aucun nouveau développement de ces fonctions n’est
engagé par cet audit.

## A — Quatre ELF historiques

Les octets historiques sont relus et comparés à
[la référence SDK v0.20.0](validation/rust-sdk-phase2-v0.20.0.json), puis incorporés
sans recompilation dans quatre noyaux issus de l’export R2
`profile.py --administration-reactivation`, avec `thread-probe,novaauth-sessions-lab`.
Les oracles [threads.py](../tests/threads.py) sont inchangés. QEMU 11.1.3, TCG,
deux vCPU, 256 Mio ; Ring3 x86_64, EL0 AArch64. Ce contrôle ABI ne dépend pas d’un
volume P9 : il s’agit de quatre ELF, pas de seize essais sur géométries disque.

| Architecture / ELF | SHA-256 historique | Résultat actuel |
|---|---|---|
| AArch64 / sdk-example | `db2d14ec8a1f8d1ac22acc331ee4d3d90336a90adc8d50ca057c3129e129c3a8` | Sortie 0, restitution OK |
| AArch64 / threads-test | `fa1640df7589a2700f172acc43197c2904e6e2f833fb748048bc15984575cbd9` | Sortie 0 ; 61109 cadres libres avant/après |
| x86_64 / sdk-example | `1ad5ebf8b6a0d80b10d2df826500f39052fdc222fac418640b408b9657d78c6e` | Sortie 0, restitution OK |
| x86_64 / threads-test | `6073a78fb2ffa5c716e75ac942932728faa218d614c3dedf6f30c9f067f6b30c` | Sortie 0 ; 61624 cadres libres avant/après |

L’exemple vérifie somme 432, TLS 7/11 et refus du double join. La qualification
étendue exerce gardes, annulations, attente/jonction, recyclage TLS, notifications,
contexte FPU/SIMD/GPR, IPC, délégation/révocation et destruction des attentes.
Deux migrations, au moins deux préemptions par thread, quatre blocs restitués et
trois TCB résiduels attendus dans les deux essais étendus. Les demandes d’ouverture
sans droits sont refusées. Aucun effet persistant P9 n’est produit ou revendiqué.

Les preuves anciennes `4481e46` ne suffisaient pas seules à couvrir les dernières
modifications des profils : ce rejeu ciblé lève cette lacune. Les noyaux/ISO QEMU
contenant des secrets de laboratoire restent privés, hors Git et archive publique.

## B — Les 301 cadres AArch64

Analyse des quatre traces historiques de
`preuves-autorite-invite-1e9fec9/final`, recopiées et hachées. Pas de nouvelle
campagne instrumentée répétée pour réobtenir la même mesure.

| Mesure au repos AArch64 | Ordinaire | Instrumenté | Différence |
|---|---:|---:|---:|
| Image résidente | 1110 | 1697 | +587 |
| Tables de pages | 146 | 147 | +1 |
| Résident hors vidéo | 1292 | 1880 | +588 |
| Vidéo en RAM | 469 | 469 | 0 |
| Total retenu | 1761 | 2349 | +588 |
| Écart au seuil 2048 | −287 | **+301** | |

`ipc_repos`, `ipc_reprise` et `ipc_detruits` sont strictement égaux dans chaque
trace. IPC, processus utilisateurs, DMA et préparation ELF reviennent à zéro ;
le chargeur est entièrement restitué. Les 588 cadres supplémentaires sont
entièrement attribués à l’image et à une table. Le compteur
[mémoire](../kernel/src/memory/mod.rs) comptabilise réellement ces pages : ce
n’est pas une erreur d’affichage ni 301 pages perdues après chaque transaction.

**Réserve maintenue.** Le profil instrumenté dépasse réellement son oracle
`retenu <= 2048` ; l’absence de croissance observée ne transforme pas cet échec en
succès. L’audit ne relève pas le seuil et n’attribue pas ce coût historique exact
à tous les futurs profils R2. Une réduction démontrée ou une décision MOA/MOE
sur le critère du profil instrumenté reste nécessaire. Les nouveaux essais IPC
ordinaires retrouvent 1761 cadres AArch64 et 1373 x86_64, stables après destruction.

## C — Reproductibilité et compatibilité

Expérience contrôlée : deux exports identiques du commit 43e5cf9, chemins A/B
distincts, même Mac ARM64, Rust **1.99.1**, LLVM **23.1.1**, Cargo verrouillé/offline,
release, mêmes cibles et aucune feature de laboratoire pour les noyaux ordinaires.
Quatre applications reconstruites depuis la même archive SDK
`6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`.

| Artefacts comparés | A contre B, chemins distincts | A contre A2, reconstruction neuve au même chemin |
|---|---|---|
| Deux noyaux ordinaires | 2 SHA différents | 2 SHA identiques |
| Quatre ELF SDK | 4 SHA différents | 4 SHA identiques |

Les anciennes constructions sont déplacées dans leurs propres répertoires de
conservation, pas supprimées. Aucun remappage de chemins, stripping additionnel,
normalisation de binaire ou modification de sécurité n’est appliqué.

- Les deux noyaux ont une `.text` identique entre A/B. `.rodata`, `.symtab` et
  `.strtab` diffèrent ; les segments chargés ne sont donc **pas** identiques.
- 52 des 54 ELF incorporés comparés sont identiques. Les deux `storage-media.bin`
  diffèrent, y compris leur `.text` et leur disposition de code. Leur construction
  Cargo reçoit notamment un chemin absolu de script de liaison. Le changement
  contrôlé de contexte de chemin suffit à reproduire l’écart ; l’attribution
  interne exacte de chaque variation à LLVM/édition de liens reste ouverte.
- Les exemples SDK conservent leur `.text`. Les témoins étendus ont deux
  instructions AArch64 et six instructions x86_64 différentes dans les routines
  de diagnostic de panique/indexation, avec adresses de chaînes relocalisées.
  Les chaînes et identifiants de compilation, dont les chemins absolus SDK,
  diffèrent dans les sections de données/symboles. Les désassemblages intacts sont
  archivés. Une annotation de symbole différente seule n’est pas comptée comme
  une instruction changée.

Contrôles fonctionnels justifiés par ces écarts : **8/8 ELF reconstruits**, **4/4
IPC ordinaires**, **4/4 lectures P9/512** avec les services ordinaires A/B.
Les huit témoins SDK conservent les oracles historiques. Les IPC passent les
bornes, refus et restitutions existants. La lecture synthétique `NovaOS\n` rend
statut 0 et la fermeture P9 est confirmée ; IPC, processus et pages utilisateur
reviennent à zéro. Ce dernier contrôle vérifie le rendu texte attendu du client
historique, pas une nouvelle recette de flux binaires administratifs. Il ne
qualifie pas exhaustivement toutes les opérations NovaFS ni toutes ses pannes.

Distinctions : compatibilité établie dans ces essais ; identité binaire observée
seulement dans le contexte identique testé ; reproductibilité inter-chemins et
inter-environnements **non acquise**. Le contexte Homebrew Rust 1.100.0/LLVM23.1.6
utilisé par certains anciens tests hôtes n’est pas assimilé à Rust1.99.1. Aucune
nouvelle toolchain n’est installée. L’avertissement historique `rust-objcopy` /
`libLLVM.dylib` absent est conservé ; les builds terminent à 0. Le Mac n’est pas
reconfiguré. Ni ces succès ni les SHA ne qualifient une distribution de production.

## D — Contrats, administration et révocation

Sources décisives :
[commandes](../../components/exec-client/src/admin_cli.rs),
[port des mutations](../user/novaauth/service/src/bin/common/administration_mutations.rs),
[plans](../user/novaauth/src/administration.rs),
[moteur transactionnel](../user/novaauth/src/lib.rs),
[profil NXADMIN1](../user/novaauth/service/administration_profile.py).
La matrice détaille les trente recettes de conception, en séparant leur portée
initiale des exclusions MOA/MOE ultérieures.

| Opérations | État constaté |
|---|---|
| `whoami`, `id`, `groups` | Exécutées dans les invités ; vues utilisateur/appartenances, sorties exactes et contrôle serveur. |
| `groupadd` | Préparation, confirmation et publication durable qualifiées ; une mutation et un groupe ; contrôle positif S7/S8. |
| `usermod --disable/--enable` | Qualifiées S9, ancienne époque fermée, reçu terminal, nouveau SID authentifié ; anciens objets restent refusés. |
| `usermod --rename` | Routé au même moteur ; succès invité complet encore à qualifier. |
| `useradd`, ajout/retrait d’appartenance | Façades compilées ; opérations serveur encore refusées. |
| Résultat après perte de réponse | Qualifié S8 dans le même processus registre ; consultation après reboot/éviction reste incomplète. |
| Suppressions définitives, promotion multi-admin, récupération de production | Non exposées, hors autorisation de cette phase. |

**Dernier administrateur.** `State::apply` refuse conservativement Disable/Delete
pour **tout** utilisateur `admin`. Préparation et confirmation repassent par le
même moteur sur la tête courante, après contrôle de capacité. S7 établit le refus
sous les interleavings testés avec publication légitime concurrente. Aucun nom de
groupe ne crée de privilège. Cela protège l’administrateur actuel ; cela ne
qualifie pas une future procédure de transfert de rôle, de suppression de son
dernier facteur ou de récupération indépendante. Ces procédures restent à arbitrer.

**Révocation.** Blocage local, admission durable, acquittements des effecteurs et
reçu terminal sont distincts. S7/S9 conservent trois acquittements réels et les
issues des lancements admis. Un refus local avant réception du reçu ne prouve pas
à lui seul le drainage. Ni délai ni absence de réponse ne valent `RETIREE`.
Réactivation : nouvelle époque et nouvelle authentification, jamais réemploi des
anciens tickets/capacités. Les droits actuels sont conservés ; le changement des
appartenances pendant la désactivation n’est pas qualifié dans les invités.

**Données et récupération.** Aucune suppression implicite, réattribution d’identité
ou de propriétaire n’est introduite. NovaFS demeure l’unique écrivain P9 ; les
plans et reçus réutilisent son triplet atomique. La récupération chiffrée 2-sur-2
et les autorités restent de laboratoire. Elles ne constituent ni un mécanisme de
récupération administrative de production ni une gestion de facteurs perdus.
Aucune nouvelle procédure n’est ajoutée pour permettre une clôture artificielle.

## Matrice de sécurité conservée

Q/R = preuve invitée acceptée, relue et réutilisée dans sa portée ; aucun rejeu S1–S9
pendant cet audit. Les trois fichiers entre 55838a et 43e5cf9 sont exclusivement
le rapport précédent, sa matrice et son manifeste. Les sources fonctionnelles et
bancs n’ont donc pas changé depuis la comparaison d’applicabilité précédente.

| Cas | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| S1 auto-attribution | Q/R | Q/R | Q/R | Q/R |
| S2 registre persistant | Q/R | Q/R | Q/R | Q/R |
| S3 références Alice | Q/R | Q/R | Q/R | Q/R |
| S4 demande directe | Q/R | Q/R | Q/R | Q/R |
| S5 substitutions | Q/R | Q/R | Q/R | Q/R |
| S6 tickets | Q/R | Q/R | Q/R | Q/R |
| S7 concurrence | Q/R | Q/R | Q/R | Q/R |
| S8 UNCERTAIN | Q/R | Q/R | Q/R | Q/R |
| S9 réactivation | Q/R | Q/R | Q/R | Q/R |

Les statuts restent ceux du [bilan accepté](novaauth-r2-securite-finalisation-20261122.md) :
DENIED5, BAD_ARGUMENT3, TIMED_OUT8, REVOKED10, REFUSED12 et UNCERTAIN22 sont distincts.
Les refus ne sont pas changés en succès. Les neuf contrôles ABI, 62 tests Python,
18 tests client et huit tests registre antérieurs sont réutilisés, **non réexécutés**.
Leurs sources restent inchangées. Aucun nouveau sélecteur ni changement NXEXEC01,
P9, capacité, bail, règle de signature ou noyau ordinaire par cet audit.

## Exécutions, ressources et réserves

**20 nouveaux essais QEMU, 20 réussites** : quatre historiques, huit applications
reconstruites, quatre IPC ordinaires, quatre lectures stockage ciblées. Les nouveaux
résultats SDK/IPC ne sont pas des essais d’authentification R2. Les 36 cellules de
sécurité réutilisées conservent leurs propres binaires/commits et limites.

Les deux manifestes antérieurs sont relus : 1897 + 310 fichiers vérifiés. Les 205
empreintes des coordinations de la campagne finale sont revérifiées sur 102 fichiers
privés distincts, sans les publier. Pas de reprise des campagnes d’utilitaires.
Une assertion du script d’audit supposait à tort les trois documents sous
`novaOS/docs` : deux résident à la racine `validation`. Le script contrôle maintenant
les trois chemins exacts. Deux autres arrêts préparatoires sont archivés : chemin relatif du générateur
du contrôle de secrets, puis absence du suffixe .serial dans la liste d’archivage.
Ils sont corrigés par résolution du chemin et admission des quatre noms exacts
de traces déjà contrôlées. Aucun de ces incidents hôtes n’est un échec QEMU ni
un défaut du produit. Aucun correctif fonctionnel ni changement d’oracle historique.

Les ressources S1–S9 restent celles publiées : quatre racines maximum par acteur,
cinquième demande LIMIT, drainage final observé ; pas de nouvelle mesure R2 ici.
Les mesures IPC ordinaires nouvelles montrent zéro IPC au repos et retour exact
aux phases reprise/destruction. Les mesures détaillées et versions/outils sont dans
les fichiers de preuve. La borne AArch64 instrumentée et la reproductibilité
inter-environnements restent réservées ; aucun élargissement de budget mémoire.

## Suites requises, non engagées

1. Raccorder les opérations R2 manquantes à leurs mécanismes existants : facteur
   préprovisionné attesté, appartenances et effecteurs par origine, vue de groupe.
   Toute nouvelle autorité ou capacité devra être soumise avant implémentation.
2. Rendre les issues récupérables après redémarrage sans déduire un acquittement
   d’un simple reçu de publication ; conserver les garanties de drainage.
3. Terminer locale/man et qualifier les modes CLI, renommage, persistance,
   interruptions et restauration des mutations R2 sur les quatre profils.
4. Traiter les deux réserves sans réduire les contrôles ; production et
   récupération administrative restent des chantiers distincts sous validation.

Aucune phase suivante commencée ; aucune fusion, aucun tag, aucune activation
ordinaire. Les branches parallèles, leurs preuves et le worktree principal sale
sont préservés. Le seul correctif livré ici est documentaire : distinguer la
qualification S1–S9 de la clôture fonctionnelle R2.

## Durées et verrou QEMU

Heures de Paris le 22 novembre 2026 (UTC+1). Les fichiers de coordination conservent
également les heures UTC complètes et les jetons de propriété.

| Contrôle | Acquisition | Libération | Exécution mesurée |
|---|---|---|---:|
| Quatre historiques | 23:08:32.540 | 23:08:51.612 | 19,007 s |
| Huit SDK reconstruits et quatre IPC | 23:17:30.220 | 23:18:28.417 | 58,101 s |
| Quatre lectures P9 ciblées | 23:22:12.248 | 23:22:33.771 | 21,460 s |

Attente cumulée du verrou : **0,000294 s** ; aucun conflit de planification observé.
Acquisition atomique, contrôle des processus incompatibles, libération du seul
verrou possédé dans finally. Aucun verrou tiers supprimé. Aucun processus QEMU
ne reste actif à la clôture des essais.

À la borne d’analyse 23:26:12.227 Paris : travail actif mural **1571,227 s** ;
compilation cumulée **303,901 s** ; exécution QEMU cumulée **98,568 s** ;
développement produit **0 s**. Les scripts d’audit relèvent du diagnostic.
Les analyses ont parfois continué pendant les compilations : ces compteurs ne
s’additionnent pas. La fenêtre d’audit est mesurée, mais le temps de réflexion
concurrent n’est pas isolé ; le minimum hors compilation/exécution est
**1168,758 s**. Le reçu adjacent de publication ajoute le scellement, la rédaction
finale et Git au temps actif total, sans attribuer l’attente au budget d’essais.

## Preuves et publication

- [Matrice finale d’audit](../../validation/novaauth-r2-audit-cloture-20261122.json) :
  36 cellules S1–S9, 30 recettes administratives, résultats, artefacts, ressources,
  réserves et limites de réutilisation.
- [Manifeste public](../../validation/novaauth-r2-audit-cloture-20261122.sha256) :
  **118 preuves**, relues puis vérifiées après décompression des 119 membres
  de l’archive, manifeste compris.
- Manifeste SHA-256 : `c91a9b96df7e919fda665be2d90889ee5841e8666745e70384274f6bcffdbe33`.
- Archive SHA-256 : `d67f487bd94db98829fd111309b5d2a122b2715d6690ff87a23e3131cd60eedd`.
- Archive : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/audit-cloture-43e5cf9-20261122.tar.gz`
  (2013992 octets).
- Les **63 artefacts binaires** sont conservés physiquement et hachés dans les
  emplacements durables inventoriés par `artefacts-durables.json`, hors archive
  publique. Cela inclut les reconstructions ordinaires et les témoins, ainsi que
  les noyaux/ISO privés QEMU : aucun binaire de laboratoire n’est ajouté à Git.
- Recherche locale de 3314 représentations connues de secrets, issues de 1105
  fichiers de référence : aucune correspondance dans les fichiers contrôlés.
  Ce contrôle exact brut/hex/base64 n’est pas une preuve universelle d’absence
  de toute transformation de secret.
- Main distant vérifié : `5c7f1bfa313a3859ccc0830660438e76d598acb1` ; branche R2
  distante avant audit : `43e5cf9c9bb69af94902d886a249df8d3979478d`.
  Seule R2 reçoit le commit documentaire. Les références après push et le bilan
  temporel final figurent dans le reçu adjacent
  `audit-cloture-43e5cf9-20261122-publication.json`.
