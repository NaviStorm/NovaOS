# NovaAuth R2 — Quatre fonctions indépendantes et retrait sélectif proposé

23 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Référence : `82349ed9cc35a7b1bd601642cb8e0738ed1ab919`.
**R2 reste ouvert : le retrait d'appartenance est seulement conçu, pas implémenté.**

## Retrait d'appartenance : décision attendue

Le contrat détaillé est dans `novaOS/docs/novaauth-r2-retrait-groupe-contrat-v0.24.0.md`.
Il propose une provenance stable sujet/groupe/révision, une réservation durable de
l'opération, le blocage local, RETRAIT_EN_COURS, le drainage par instance d'effecteur
et RETIREE seulement après publication et tous les acquittements. Les capacités
directes et issues d'autres groupes restent utilisables. Les réponses perdues se
consultent par identifiant ; une nouvelle appartenance ne réactive pas les anciennes
capacités. Dernier administrateur contrôlé dans la transaction faisant autorité.
Le budget de quatre racines reste une contrainte à démontrer pour ce nouveau raccord.
Aucun code de retrait, nouveau sélecteur, droit ou mécanisme de suppression n'est ajouté.
La réservation et le reçu privés proposés nécessitent validation avant implémentation.

## Exécutions nouvelles

52 parcours QEMU réussis, 13 par profil ; deux échecs préalables conservés avec
leur diagnostic. Ce total ne transforme pas les 36 S1–S9 ni les 48 parcours
fonctionnels antérieurs en nouvelles exécutions. 60 démarrages appartiennent aux
52 parcours réussis, dont quatre premiers invités volontairement coupés.

| Fonction / recette | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| Même facteur, deux créations concurrentes | réussi | réussi | réussi | réussi |
| Authentification réelle de Carol après création/reprise | réussi | réussi | réussi | réussi |
| Six ELF administratifs depuis scripts et `nsh -c` | réussi | réussi | réussi | réussi |
| Perte du reçu durable, réconciliation puis redémarrage | réussi | réussi | réussi | réussi |
| Coupure avant admission durable, reprise sans succès inventé | réussi | réussi | réussi | réussi |
| Retrait sélectif d'appartenance | non implémenté | non implémenté | non implémenté | non implémenté |

La matrice JSON contient les 54 résultats, leurs commits de banc, arbres des
binaires invités, commandes, statuts, effets persistants et références de preuves.
Chaque essai indépendant commence sur une copie neuve ; son redémarrage conserve
le disque obtenu dans ce parcours et l'état de l'autorité externe.

### Concurrence du facteur

Deux ELF `admin-sonde` distincts, PID 21 et 22 dans les traces, préparent des
créations `carol` et `caroline` utilisant la même référence publique. Le pipe
synchronise les premiers fragments IPC : les deux PREPARE sont effectivement en
vol. Les deux préparations réussissent ; une seule confirmation vaut OK(0), l'autre
DENIED(5), son intention ayant été remplacée dans le contexte borné de la même
session Alice. Les deux sondes sortent à 0, ce qui signifie que la recette a été
exécutée, et ne signifie pas que les deux mutations ont réussi. Pipeline à 0,
stderr vide, 160 octets binaires contrôlés. `whoami` Alice réussit ensuite.

La relecture indépendante vérifie une seule génération supplémentaire, une seule
entrée de mutation signée et une seule attribution du facteur à un utilisateur
non administrateur. Aucune autre permission, installation ni donnée privée ne
change. Ce témoin couvre deux clients de la même session, pas deux administrateurs
distincts ni toutes les permutations possibles d'ordonnancement.

### Carol

`useradd` inscrit le facteur public préprovisionné, sans clé privée dans le registre.
Après capture native et arrêt complet, l'administrateur indépendant approuve une
nouvelle incarnation. La seconde route est authentifiée par le signataire isolé
muni du facteur Carol. Le sujet reçu par le vrai nsh correspond exactement à
l'identité créée et relue dans P9, pas à un nom supposé par le banc.

Carol reste non administratrice et membre de `base`. Les trois tentatives sur les
installations `whoami`, `admin-sonde`, `alice-seulement` rendent DENIED(5), sans
enfant. Aucun paquet ou droit Alice/Bob n'est hérité. La route Alice conserve ses
consultations légitimes. Ce profil emploie les deux routes QEMU existantes ; il ne
constitue pas un mécanisme de connexion ou d'enrôlement de production.

### Scripts et nsh -c

Les vrais fichiers `/travail/r2-*.nsh` sont préprovisionnés sur la fixture P9
synthétique ; leurs variables de confirmation sont exportées comme données.
Les 32 nouveaux parcours de mutation/refus couvrent création d'utilisateur,
renommage, création de groupe et refus de désactiver le dernier administrateur,
dans les deux modes et sur les quatre profils. Les huit parcours de consultation
couvrent `whoami`, `id` et `groups` dans les deux modes.

PREPARE/CONFIRM et les consultations passent par les mêmes ELF NXADMIN1 et les
mêmes vérifications NovaAuth. Le refus administratif vaut DENIED(5), sortie ELF3 ;
les mutations autorisées valent 0. Les contenus stdout/stderr sont comparés
exactement. Cela ne prétend pas couvrir chaque option de chaque commande ni le
retrait d'appartenance encore refusé. Les recettes FR/EN et man précédentes sont
conservées ; les six ELF métier sont identiques à ceux déjà qualifiés.

### Persistance et pannes

Pour la perte du reçu, l'autorité a publié la mutation avant que le banc supprime
sa réponse. L'invité rend UNCERTAIN(22), sortie ELF20. Une consultation suivante
réconcilie le même plan ; la capture native est produite seulement après activation
vérifiée. Après arrêt complet et récupération 2-sur-2, `useradd --outcome` retrouve
le même résultat avec statut 0. Une seule mutation signée existe et le triplet de B
est identique au triplet confirmé de A. Aucune seconde confirmation n'est envoyée.

Le cas de coupure est distinct : QEMU s'arrête avant le traitement durable de
PUBLISH (RPC50). Aucun statut de confirmation n'est observé et aucune mutation
administrative n'existe. Après récupération sous une nouvelle incarnation,
OUTCOME conserve UNCERTAIN(22)/ELF20, `id carol` rend DENIED(5)/ELF3 et `whoami`
réussit. La tête, les trois objets et les données témoins restent inchangés.

Ces recettes ne couvrent pas une coupure dans chaque fenêtre entre décision
externe, publication P9 et sauvegarde. En particulier, la perte de reçu ici testée
est réconciliée dans A avant son arrêt. L'absence d'une sauvegarde correspondant à
une nouvelle tête durable reste un refus sûr et une dépendance de récupération ;
aucune reconstruction arbitraire n'est introduite.

## Corrections et échecs conservés

- `e34ccfb3b46504f51d3b735e237eec62b9580b2b` : capture native également après le
  chemin de réconciliation. Le premier essai `creation-perte01` avait la mutation
  durable mais aucune capture, donc aucune reprise qualifiée. La correction
  réutilise le plan déjà vérifié et les destinataires existants ; aucun second submit.
- `d211b6e84e3914ab38c0db9e4e6d791e21593182`, puis `a653cf5173cc9caf4bd0fdd08c2f6f6659c8ed6e` :
  fragments initiaux triés par numéro, tailles et nombre bornés avant allocation.
  L'essai `creation-coupure01` plaçait fragment-10 avant fragment-2 ; le registre a
  correctement refusé ce contenu cryptographique. Les quatre rejeux corrigés passent.
- `2f7e950509020156416947bb7192a2703bae4df9`, `09e6cdb33b020b2cb09eb27ad2f632346f19b897`,
  `8f09da47b121a934a27edc09ef45c3e676c2532c` : modes, coupure et scripts du banc.
- L'export préparatoire rejeté pour indentation, l'assemblage de comparaison des
  registres rejeté pour empreintes différentes et les sept erreurs hôtes dues à
  NOVAOS_TMPDIR absent sont conservés. Aucun n'est compté comme succès QEMU.

## Sources, binaires et contrôles

L'arbre invité `a440095b2862b3360db429ed417b18f32abc13ba` est exactement celui de
`e34ccfb3b46504f51d3b735e237eec62b9580b2b`. Les modifications suivantes ne concernent
que banc hôte, fixture et documentation. Les ISO, noyaux, registres, sources de
sonde, commandes de compilation et leurs SHA sont répertoriés. Les quatre lectures
`nsh -c` initiales utilisent les artefacts antérieurs `fonctions-08`, avec leurs
propres empreintes ; elles ne sont pas présentées comme exécutées sur les nouveaux
noyaux. Les mutations `nsh -c` sont, elles, exécutées avec les nouveaux artefacts.

Le profil Carol est reconstruit au même chemin que la base : registre strictement
identique sur chaque architecture, signataire du second facteur et sonde différents.
L'essai préparatoire avec des chemins de compilation différents avait produit des
ELF de registre différents, contenant ces chemins. La réserve de reproductibilité
entre environnements n'est donc pas levée.

73/73 tests hôtes administratifs passent, puis 7/7 tests ciblés passent après la
borne finale de fragments. Neuf contrôles ABI réussissent. Les sources du noyau
ordinaire, de l'ABI, de NovaFS, de nsh et du cœur NovaAuth sont inchangées depuis
82349ed. Les transformations de récupération/Carol restent opt-in QEMU. Aucun
rejeu inutile des 36 S1–S9, des 20 essais d'audit ou des quatre ELF historiques.
Les anciens résultats gardent leur périmètre et leurs artefacts, sans prétention
d'identité binaire de l'ensemble de la nouvelle image. Les 3 466 preuves de la
campagne précédente et son archive ont été relues et leurs SHA confirmés.

## Ressources et temps

Les 60 démarrages des parcours réussis respectent quatre racines maximum par
acteur et refusent la cinquième sans allocation résiduelle. Les 56 sorties normales
ont les compteurs du signataire et du registre identiques avant/après et un drainage
final de famille. Les quatre arrêts forcés ne sont pas des collectes invitées :
le banc vérifie leur arrêt complet, le drainage hôte et la reprise de B ; il ne
prétend pas avoir observé une libération native dans une VM coupée.

Les tuples mesurés sont (racines, handles, attentes, pages, pile, cadres IPC globaux).
Signataire : (4,6,0,59 ou 68,64,31) ; registre : (4,7,0,140 ou 146,64,31), selon
image/profil. Ne pas confondre pages mappées de l'image, mémoire active et cadres
IPC. Les tableaux par VM conservent valeurs, pics et capture intermédiaire.
Les 60 échanges TLS horodatés durent 2 632 à 3 320 ms invités, avec au moins
6 680 ms de marge sur 10 s. Les sondes DEBUG sont bornées : ceci n'est pas une
mesure exhaustive de chaque échange de la vie du service.

`temps.json` conserve les acquisitions/libérations UTC et durées monotones hôtes.
L'attente d'un second lanceur R2 se superpose au banc déjà actif : elle est publiée
séparément mais n'est pas déduite deux fois du budget. Aucun essai concurrent,
aucun verrou tiers supprimé. Le budget de 90 minutes est compté conservativement
en temps mural actif ; compilation, diagnostic et rédaction sont séparés des bancs.
Aucune soustraction entre horloge hôte et horloge invitée.

## Limites et suite

Conserver la réserve instrumentée AArch64 de 301 cadres, la reproductibilité entre
environnements, et les limites de laboratoire (facteurs, signataires, autorité,
gardiens de récupération et transport QEMU). Les tableaux ne qualifient pas la
distribution ordinaire. Aucun secret connu n'est présent dans les preuves contrôlées ;
la recherche exacte brut/hex/base64 n'est pas une garantie universelle d'absence.

La prochaine décision concerne le contrat de retrait sélectif. Après validation,
il faudra implémenter ce seul raccord, démontrer son budget puis qualifier les
droits directs/G/H, les opérations en vol, les acquittements et les reprises sur
quatre profils. Aucun effacement de données, retrait de privilège administratif
supplémentaire ni développement d'utilitaire n'est autorisé par cette livraison.
Aucune fusion, aucun tag ; la clôture R2 n'est pas proposée.
