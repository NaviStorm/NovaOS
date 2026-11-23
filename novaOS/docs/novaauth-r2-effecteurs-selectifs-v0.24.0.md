# NovaAuth R2 — Effecteurs sélectifs : livraison intermédiaire

23 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Base `3036827597d7761abc39c7e9b515a2a2da40a5ad` ; code validé :
`735605b217837df91321db71cdee58d4ec7581ab`.

**Le retrait sélectif reste incomplet et non qualifié dans l’invité.**
La commande `RemoveMember` reste refusée. Aucun reçu terminal RETIREE, aucun
acquittement VFS/EXEC invité et aucune restitution invitée ne sont revendiqués.
Les quatre recettes sur quatre profils sont **NON EXÉCUTÉES**, avec les causes
détaillées ci-dessous. Cette livraison ne propose pas la clôture R2.

## Composants implémentés et contrôlés

Le profil explicite `profile.py --membership-r2` ajoute uniquement dans la copie
QEMU les éléments suivants ; les profils historiques ne l’activent pas.

- Fermeture VFS sur la conversation privée existante registre/NovaFS. Le noyau
  inscrit l’identité complète de l’instance NovaFS aux deux extrémités. Aucun
  nouvel endpoint, root IPC ou droit public n’est ajouté par ces transformations.
- Trame privée `NXMGVFS1` de 128 octets : opération, sujet, groupe, révision
  d’appartenance, position durable et instance attendue. La réponse `NXMGACK1`
  est liée à la trame exacte par SHA-256 et à cette instance ; aucun timeout
  n’est transformé en succès. La position est strictement entre 0 et 2^63.
- Méthodes de `Directory` qui conservent la provenance de chaque racine et de ses
  descendants. La fermeture retire G, préserve Direct/H et les autres sujets,
  puis refuse un nouvel octroi de l’ancienne provenance. La révision initiale 0
  est valide pour un groupe et n’est jamais assimilée à une permission directe.
- Annulation d’un PREPARE VFS sans publication, conservation de sa référence et
  du réveil exact pendant la saturation IPC. Une reprise consulte le même retrait.
  Un réveil remplacé, un lease sans provenance ou une autre instance interdisent
  l’acquittement. Les publications P9 synchrones ne sont pas annulées ici.
- Inspection EXEC privée `NXMGEXE1`, sous la sérialisation noyau existante, sur la
  route du vérificateur déjà authentifiée. Elle vérifie sujet/session/instance,
  refuse une image en construction ou une provenance de fichier non résolue et
  laisse les tickets indépendants intacts. Le résolveur actuel n’autorise que
  les installations personnelles du titulaire : cette inspection ne démontre
  pas la révocation d’un ticket dépendant de G. Le retour n’est pas un reçu durable.
- Activation explicite des RPC privés 54 à 60 sur le TLS existant. Une base sans
  le schéma sélectif est refusée ; aucune migration implicite ne découle d’une
  requête. La fixture initialise ce schéma seulement avec l’option dédiée.

Ces contrôles ne créent aucune permission sur la foi d’un identifiant fourni par
le CLI. Ils n’altèrent ni les baux, ni les capacités administratives, ni NXADMIN1,
NXEXEC01 ou les contrats historiques SPAWN. Les sources ordinaires du noyau,
l’ABI, NovaFS P9 et les composants CLI sont identiques à la base de cette livraison.
Les transformations du noyau et du service VFS existent uniquement dans l’export
QEMU explicite. L’identité des sources n’est pas une preuve de reproductibilité
binaire entre environnements.

## Résultats effectivement obtenus

| Contrôle | Environnement | Résultat |
|---|---|---|
| Autorité : retrait, époques, mutations administratives | Python hôte | 40/40 |
| Effecteurs VFS, inventaire EXEC, layout | Darwin ARM64 natif | 15/15 |
| Mêmes tests natifs | Darwin x86_64 sous Rosetta | 15/15 |
| Reçus/protocole sélectif : 6 existants + 4 nouveaux | ARM64 natif | 10/10 |
| Mêmes tests de protocole | x86_64 sous Rosetta | 10/10 |
| Registre ABI et 48 sélecteurs | Sources de la branche | 9/9 |
| Registre ABI et 48 sélecteurs | Export QEMU exact du commit | 9/9 |
| Compilation complète de l’export expérimental | AArch64 nue | réussie |
| Compilation complète de l’export expérimental | x86_64 nue | réussie |
| Quatre ELF historiques dans l’invité | Aucun nouveau démarrage | non rejoués |

Le harnais VFS compile les types et méthodes réels de `Directory` ; seul le retour
IPC est simulé. L’inspection EXEC utilise le code réel avec des structures Core
minimales : elle ne teste pas les courses de l’ordonnanceur. Les tests de
l’autorité utilisent des faits simulés. Aucun de ces résultats n’est assimilé à
un essai invité. Le harnais régénéré depuis le commit est identique au harnais testé.

Les statuts observés par les tests natifs comprennent REVOKED(10), DENIED(5),
WOULD_BLOCK(7), REFUSED(12), LIMIT(6) et CANCELLED(9). Direct/H restent utilisables
après la fermeture de G dans ces tests. La réponse perdue ne provoque ni deuxième
retrait local ni deuxième notification après acquittement.

Échecs préparatoires conservés : génération initiale du harnais au mauvais niveau
`pub(super)`, imports Python depuis un mauvais répertoire, et quatre erreurs ABI
sur l’export dépourvu d’historique Git. Les deux premiers ont été corrigés dans
l’infrastructure ; le dernier est résolu en donnant explicitement accès en lecture
au dépôt historique via GIT_DIR/GIT_WORK_TREE, sans changer les attentes des tests.
Les sondes de layout emploient une erreur de type volontaire pour faire afficher
la taille par rustc ; elles ne sont ni échecs fonctionnels ni essais QEMU. Les
copies de source ont été restaurées et leurs empreintes revérifiées après mesure.
La compilation émet aussi l’avertissement historique `rust-objcopy`/libLLVM.dylib
absent ; cargo termine avec statut 0. Aucune configuration du Mac n’a été modifiée.

## Ressources mesurées et limites

| Élément | Avant | Après | Différence |
|---|---:|---:|---:|
| Directory natif | 1 256 octets | 2 048 | +792 |
| Option<Route>, chaque cible nue | 2 648 octets | 2 712 | +64 |
| Deux routes noyau | — | — | +128 |

Le sous-total statique recensé vaut 2 960 octets : inventaire 1 536, corps 248,
deux buffers 256, VFS supplémentaire 792 et deux routes supplémentaires 128.
Il reste inférieur à la cible de conception de 4 Kio ; ce n’est pas une mesure
de toute la mémoire active. Les piles, copies temporaires, TLS/RPC, pages physiques,
pics et restitution invités restent à mesurer. La table VFS à 12 entrées refuse
la treizième avec LIMIT sans perte des droits existants dans le harnais natif.
Le plafond de quatre racines est conservé dans les sources ; il n’a pas été
mesuré sur ce nouveau parcours. La réserve AArch64 de 301 cadres demeure ouverte.

## Blocages établis et ordre de poursuite

1. La décision fraîche `View.permission` n’est pas encore transmise aux octrois
   VFS vivants du profil isolé. `membership_grant` est testé nativement mais
   n’est pas appelé par le chemin de données invité. Celui-ci reste un chemin
   de lecture directe ; réutiliser `policy_directory` et son autorité locale
   serait incorrect.
2. Le registre n’associe pas encore chaque demande VFS locale de 64 bits à son
   identifiant d’admission durable de 16 octets, son origine et son effecteur.
   Le fait local « PREPARE annulé » ne permet donc pas encore d’envoyer FINISH
   pour une admission identifiée. Un compteur de handles nul ne suffit pas.
3. L’orchestrateur administratif n’enchaîne pas encore réservation, fermeture,
   publication atomique du triplet, relecture P9, issues individuelles, ACK de
   chaque instance et consultation fraîche RETIREE. `membership_close` et
   l’inspection EXEC ne sont pas appelés par CONFIRM ; la liste d’opérations
   autorisées dans `administration_mutations.rs` refuse toujours RemoveMember.
4. La fermeture VFS conservée en mémoire permet la reprise du même retrait,
   mais son passage à l’opération suivante et la reconstruction après redémarrage
   restent à raccorder aux réservations signées. Un autre retrait reçoit
   WOULD_BLOCK ; aucune perte de l’état local n’est présentée comme drainage.
5. Après ces raccords, exécuter les quatre recettes ci-dessous, observer les
   vrais acquittements, les droits Direct/H/G et les ressources invitées. Une
   perte de réponse exige QUERY par identifiant, jamais un second CONFIRM.

Ce sont des travaux d’implémentation restants dans le contrat approuvé, pas une
restriction de plateforme. Aucune interdiction de sécurité n’a été contournée.
Le manque de preuve interdit de rendre COMPLETE ; il n’autorise aucune réduction
des exigences de drainage ni d’allongement des délais.

## Matrice QEMU de ce candidat

| Recette | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
|---|---|---|---|---|
| Q1 : Direct/G/H, refus Bob, nouvelle appartenance | non exécutée | non exécutée | non exécutée | non exécutée |
| Q2 : opérations en vol, réveil et drainage | non exécutée | non exécutée | non exécutée | non exécutée |
| Q3 : retraits/admissions concurrents | non exécutée | non exécutée | non exécutée | non exécutée |
| Q4 : acquittements perdus, interruption et reprise | non exécutée | non exécutée | non exécutée | non exécutée |

Aucun verrou QEMU acquis, aucune attente du verrou, aucune campagne concurrente.
Exécution QEMU : 0 seconde. Le fichier `temps.json` sépare les validations hôtes
instrumentées, les compilations, les sondes et le temps actif total. Le reliquat
non instrumenté contient développement, diagnostic, rédaction et les premières
commandes de prévalidation ; il n’est pas présenté comme une mesure précise de
chaque catégorie. Aucun horodatage invité n’est mélangé aux durées hôtes.

Les 71 preuves et l’archive de la précédente livraison ont été relues et leurs
SHA-256 vérifiés. Les 52 parcours QEMU historiques conservent leur périmètre et
leurs artefacts ; ils ne qualifient pas le profil sélectif ajouté. Les utilitaires
restent en pause, les worktrees parallèles sont préservés. Aucun tag, aucune fusion
ni activation de production. La réserve de reproductibilité demeure ouverte.

## Preuves

Les résultats positifs et négatifs, les commandes exactes et les hashes des sources
sont publiés dans [le dossier de validation](validation/novaauth-r2-effecteurs-selectifs-v0.24.0/preuves.json).
La matrice contient 16 résultats NON_EXECUTE, sans PID, statut invité ou mesure
inventés. Aucun binaire de laboratoire ni clé privée n’entre dans les preuves.

Les journaux bruts conservent les espaces et lignes terminales émis par les outils.
`git diff --cached --check` les signale ; ils ne sont pas réécrits après scellement.
Le contrôle des sources et des documents hors journaux réussit.
