# NovaOS v0.24.0 — Finalisation fonctionnelle R2, état du 23 novembre 2026

Branche `codex/novaauth-administration-0240-phase1`. Référence d'audit
`46d04ff29e9c22e26c8d4261ba4c742c132eec67`. Commits fonctionnels :
`a67df6eabeeaa03af20a6cfdfd6ec0d3292b571d` (consultation groupes),
`6b015750763f4ae3eca1691148b58b9185304508` (appartenance et locale),
`47541062f105b36833c342492bf2f019e8b42231` (catalogue, reçus et corrections).
Le commit de banc `a02b89d28e4b1b650153fbd8d00b513d5b44a5e7` étend la reprise aux trois mutations sans retrait ;
aucun binaire invité n'est modifié pour cette extension.

## Résultat et périmètre

**R2 reste ouvert.** Les fonctions ci-dessous sont exécutées dans les invités
QEMU, avec vrais ELF depuis nsh, confirmations explicites, vérifications côté
serveur, comparaison des flux exacts et relecture indépendante des trois objets.
Elles ne constituent ni un enrôlement ni une récupération de production.

| Fonction | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| Consultation groupe | réussi | réussi | réussi | réussi |
| Création et reprise | réussi | réussi | réussi | réussi |
| Renommage et reprise | réussi | réussi | réussi | réussi |
| Ajout appartenance et reprise | réussi | réussi | réussi | réussi |
| Création groupe et reprise | réussi | réussi | réussi | réussi |
| Reçu perdu et réconciliation | réussi | réussi | réussi | réussi |
| Facteurs refusés | réussi | réussi | réussi | réussi |
| FR/EN et man | réussi | réussi | réussi | réussi |

La création concerne Carol non administratrice, avec facteur préprovisionné et
groupe `base`. `useradd --prepare`, `--confirm` et `id carol` réussissent avec
statut 0. Aucun rôle, installation, droit de dossier ou session d'Alice n'est
copié. La nouvelle identité et son époque correspondent à la génération publiée.
La possession effective du facteur par Carol n'a pas été authentifiée dans cette
campagne : inscription et connexion restent deux garanties distinctes.

`groups --group base` et `--group-id` restituent le même groupe et ses membres.
`usermod --add-group equipe-r2 bob` ajoute uniquement l'appartenance prévue.
Le renommage `bob` vers `robert` conserve identifiant, époque, facteurs, droits,
appartenances et données privées. `LANG=fr; groups man` et `LANG=en; man groups`
rendent les références exactes ; les aides FR/EN des six ELF restent testées sur
hôte. Cela ne qualifie pas encore tous les messages d'erreur localisés dans l'invité.

## Catalogue public et sécurité

Format canonique `NAR2FAC1` : en-tête 32 octets et au plus 16 clés Ed25519 de
32 octets, soit 544 octets maximum. Le catalogue livré contient deux clés
publiques (Alice, déjà attribuée, et Carol), 96 octets ; SHA-256
`46d85e0db0d4f58082b92ed70b58546939be2cd9aaa8456b3285686cb248f5cf`.
L'attestation repose sur l'image mesurée du registre et l'approbation indépendante
de son incarnation, avec empreinte attendue incorporée. Ce n'est pas une signature
autonome d'un fichier fourni par le CLI. Le domaine, la taille, les réservés,
les clés faibles/invalides et les doublons sont contrôlés. La référence CLI seule
ne confère aucun droit. PREPARE et CONFIRM vérifient les capacités administratives,
le catalogue et l'unicité dans le même moteur transactionnel.

Dans les quatre invités : facteur inconnu → **DENIED(5), CLI3** ; facteur Alice
déjà attribué → **CONFLICT(24), CLI4** ; `whoami` Alice réussit ensuite. Relecture
P9 : aucune mutation, aucune attribution, aucun changement du TSV privé synthétique.
Le champ historique `statut_natif` du banc désigne le code de sortie ELF ;
`statut_novaAuth`, quand présent, conserve le code serveur distinct.

Les limites 0/1/16 clés, dépassement 17, encodages invalides, mauvais domaine,
empreinte, clés faibles et doublons passent les tests hôtes. Deux plans concurrents
pour le même facteur : une seule publication, refus de la tête obsolète et du
facteur déjà utilisé. **Ce dernier test est hôte ; la concurrence d'attribution
reste à qualifier dans les quatre invités.** Le refus du dernier administrateur
n'est pas assoupli et aucune suppression définitive n'est exposée.

## Reçus persistants et reprise

Les types privés historiques 1–4 sont conservés ; 5/6/7 distinguent renommage,
création de groupe et ajout d'appartenance sans retrait. Champs de retrait nuls
canoniques. Outcome vérifie une consultation HISTORY authentifiée, le principal
initial, les capacités actuelles, le défi frais et la tête P9 courante authentifiée.
Un reçu d'une génération future ou d'une tête divergente ne devient pas durable.
L'ancien UPDATE générique reste conservateur : aucun succès durable n'est inventé
à partir de ce type ambigu. Les désactivations exigent le reçu terminal RETIREE
signé, lié à l'opération et à l'époque, avec les trois effecteurs acquittés.

Perte volontaire du reçu de création de groupe : **UNCERTAIN(22), CLI20**, puis
réauthentification explicite et Outcome du même identifiant → COMPLETE(0).
Signature, journal durable et remontage P9 démontrent une seule mutation.

Le profil explicite `--capture-administrative` réutilise la capture native
chiffrée Pending.backup après publication, avec les deux destinataires déjà
approuvés. Aucun secret supplémentaire n'est confié au CLI, NovaFS ou au signataire.
Un échec de capture ne retire pas un reçu déjà durable ; le banc interdit alors
la reprise. Ce profil n'est jamais activé implicitement.

Recette de redémarrage : mutation native et capture acquittée dans A, fermeture et
drainage, arrêt complet, retrait durable de l'ancienne incarnation, approbation
indépendante de B, récupération 2-sur-2 depuis la capture produite par A,
nouvelle authentification d'Alice, Outcome du même identifiant, lecture de la vue
modifiée et `whoami`. Le triplet avant/après est identique, une seule mutation
administrative persiste. Aucune création ou confirmation n'est répétée en B.
Ce scénario est un redémarrage nominal avec l'état courant, **pas** une nouvelle
preuve de restauration d'un état obsolète suivie d'un contrôle VFS privé.

## Exécution, ressources et compatibilité

48 parcours nouveaux réussis, 5 parcours en échec conservés.
Un parcours de reprise comprend deux démarrages QEMU ; les tests hôtes ne sont
pas comptés comme invités. QEMU11.1.3, TCG, 2 vCPU, 256 Mio, réseau désactivé,
P9/512 et P9/4096. Les commandes, versions, ISO, noyaux, registre, disque initial,
statuts, chronologies et hashes de banc figurent dans chaque preuve.

Sur les parcours réussis : racines ≤4 pour chaque PID, cinquième demande LIMIT,
ressources signataire et registre avant/après identiques ; capture native également
bornée dans les reprises. Les enfants et sessions rendent racines/handles/attentes/
pages/pile/threads à zéro. Drainage final : endpoints/nodes/espaces/cadres IPC à zéro.
Les images résidentes de services et caches ne sont pas présentées comme des pages
rendues avant leur terminaison. Les tableaux par phase restent dans la matrice.

64 échanges TLS horodatés : 2608–3348 ms ;
échéance absolue inchangée de 10 000 ms. Le quota DEBUG peut omettre des mesures ;
aucune prétention de chronométrage exhaustif. Baux de session inchangés.

Tests hôtes : 20 clients par ISA ; catalogue 2 et registre 8 par ISA ; test
concurrent d'attribution supplémentaire par ISA ; récupération 3 + historique 4 +
admission 4 par ISA ; autorité 19 tests ; collecteur 6, certificats 1, capture 13 ; événements/reprise 21 tests additionnels avant les douze reprises.
ARM64 Darwin natif et x86_64 Rosetta sont distingués des invités. Les **9 contrôles
ABI réussissent**, les 48 sélecteurs ne changent pas. Aucun fichier du noyau
ordinaire, de l'ABI source ou du moteur/format NovaFS n'est modifié depuis l'audit.
Les quatre ELF historiques et les 20 essais de l'audit sont préservés, non rejoués. Les 118 preuves d'audit et 2 207 preuves des campagnes de sécurité sont reverifiées par SHA-256.

Les 36 cellules S1–S9 restent des acquis de leurs candidats et périmètres acceptés.
Les gardes d'identité, de capacité et les barrières d'époque ne changent pas.
La reprise S8 de groupe est rejouée sur le nouveau type privé 6. L'ajout des chemins
Create/AddMember et du résultat persistant est couvert par les essais ciblés ;
les preuves historiques ne sont pas réétiquetées en qualification globale du
nouveau candidat. Les futurs tests de concurrence/enrôlement et de retrait par
origine restent explicites.

Arbres de binaires : `417600568b6493e7d91758558aab4dcbdde361bb` pour les premiers
parcours anglais ; `fea21d86c6042ee2c1e41fbec61e7092c52fa8a3` pour la locale
corrigée ; `3aa6d9c11026fed10ad3d83a22d58364892ecfbc` pour capture/reprise.
Ce dernier ne diffère de 4754106 que dans deux fichiers de banc hôte. Les premiers
résultats restent bornés à leurs chemins : la correction locale ne change pas
l'anglais, le nouveau profil de capture est opt-in. Les attestations et dates des
certificats propres à chaque image sont conservées ; aucune identité binaire
entre ces images n'est revendiquée. Les copies privées sont hachées et conservées.

## Défauts rencontrés et corrigés

1. Disque auxiliaire de fixture de 25 blocs mal aligné pour `cache=none` Darwin :
   28 blocs ; aucun changement du format P9. Premier essai sans amorçage conservé.
2. Champ locale écrit au mauvais offset du bloc de lancement (8 au lieu de 56) :
   sortie française incorrecte, corrigée et comparée octet par octet.
3. Certificats éphémères expirés à 23:28:53 UTC : refus TLS sûr. Nouveau jeu privé
   de même durée 24 h, puis préflight de validité avant campagne. Pas d'allongement
   du délai TLS ni de désactivation des vérifications.
4. Arrêt de pas GDB AArch64 retournant le même PC : diagnostic conservé ; trois
   pas maximum sur le même CPU, pile inchangée si PC stagne. Jamais de déplacement
   artificiel de PC ni d'événement absent accepté. Tests hôtes dédiés ; les succès
   ultérieurs n'ont pas reproduit ce double arrêt, sa cause interne QEMU reste ouverte.
5. Capture native non routée quand le drapeau VFS était absent : ouverture du
   routage existant pour le seul profil de capture administrative explicite.
   Les authentifications et bornes CaptureStore restent inchangées. L'échec
   `creation-08` avait A réussi mais aucune reprise : le code global1 prévaut ;
   le banc garde désormais succès=false jusqu'au résultat de B.

Les erreurs de scripts de vérification (champs JSON vides omis par sérialisation,
chemin du manifeste et fichier d'extraction déjà présent) ont été corrigées sans
modifier les états ou les attentes de sécurité. Les résultats négatifs sont archivés.

## Temps et verrou

Du 2026-11-22T22:42:53+00:00 au 2026-11-23T00:02:28.210030+00:00 (borne de consolidation) :
attente du verrou **0.025335 s**, exécution des bancs
**1571.257 s**, travail actif mural hors attente
**4775.185 s**. Reste hors banc (développement, diagnostic,
compilation et rédaction/Git) : **3203.927 s**.
La réflexion peut se poursuivre pendant les bancs : ce reste n'est pas une mesure
CPU exclusive. Acquisition/libération UTC de chaque essai et durées monotones
hôtes sont archivées séparément. Aucun verrou tiers supprimé, aucune campagne
concurrente détectée. La borne finale de publication est fournie dans le reçu adjacent.

## Restant avant clôture

- Retrait d'appartenance : raccorder les effecteurs par origine avant de lever
  REFUSED ; préserver les droits directs indépendants et le drainage durable.
- Attribution concurrente du même facteur dans les quatre invités ; création par
  Bob sans droits, variantes non autorisées et authentification du nouvel utilisateur.
- Modes scripts/nsh -c des nouvelles commandes, erreurs FR/EN invitées complètes.
- Coupures avant confirmation durable et reçus absents/altérés/obsolètes après
  reprise dans l'invité ; les tests hôtes ne remplacent pas ces recettes.
- Reprise depuis disques obsolètes après nouvelles mutations avec lecture VFS
  autorisée puis refus du droit révoqué ; ancien Outcome UPDATE ambigu reste réservé.

Conserver les réserves **301 cadres instrumentés AArch64** et **reproductibilité
entre environnements**. Aucun seuil relevé, aucune garantie de production déduite.
Ni suppression de compte/groupe ni effacement automatique des données.
Les utilitaires restent en pause ; aucune fusion dans main, aucun tag.

## Preuves

Matrice : [résultats détaillés](../../validation/novaauth-r2-fonctions-20261123.json).
Manifeste : [SHA-256](../../validation/novaauth-r2-fonctions-20261123.sha256).
L'archive publique exclut les clés, noyaux et ISO privés. Elle conserve les
traces, résultats, états synthétiques et captures chiffrées ; les binaires privés
restent inventoriés à leurs emplacements durables. Le reçu de scellement donne
les nombres de fichiers, les SHA-256 et la relecture de chaque membre de l'archive.
