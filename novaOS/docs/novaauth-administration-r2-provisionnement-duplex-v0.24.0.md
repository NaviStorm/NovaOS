# NovaAuth v0.24.0 — Provisionnement de Bob et qualification duplex R2

Point de passage du 22 novembre 2026, branche
`codex/novaauth-administration-0240-phase1`, depuis
`33d87d8470b533eecf117054a6095bab2a8fd1a2`.
**R2 reste incomplet : ce rapport ne qualifie pas ses mutations administratives.**

## Contrat effectivement exécuté

Le provisionneur de laboratoire publie explicitement le paquet signé pour Bob
avec `provision_package_for_user`, exclusivement sous `administration-lab`.
L'entrée historique conserve sa signature, ses refus et son propriétaire.
Bob reste non administrateur, avec une portée `SELF` seulement ; Alice possède
ses propres installation, facteur, instance et session. Aucun droit VFS général
ni droit d'installation n'est remis à Bob.

Le manifeste signé historique atteste le paquet, l'architecture, la version et
les octets. Le destinataire est lié par la transaction authentifiée du
provisionneur et le catalogue du titulaire, pas par un nouveau champ du
manifeste. Le registre isolé exige sa tête indépendante fraîche et résout le
catalogue de l'identité réellement authentifiée. Le chargement contrôle ensuite
l'objet, sa révision et les octets attestés. La signature est vérifiée lors de
la publication ; elle n'est pas présentée comme une nouvelle signature relue
par chaque appel `SPAWN`.

La fixture supplémentaire installe `alice-seulement` uniquement pour Alice.
Depuis le vrai `nsh` de Bob, sa résolution et celle de `paquet-non-autorise`
doivent retourner `DENIED=5`, sans enfant. Bob exécute ensuite son propre
`admin-sonde --r2-refus` ; Alice exécute `alice-seulement --self` après la
fermeture de Bob. Les octets proviennent de deux PID enfants distincts.

Le collecteur appartient au banc hôte : sondes GDB en lecture, liées aux
instances, sessions et PID. Il ne délègue aucun handle aux utilisateurs.
Alice n'obtient pas les sorties de Bob et Bob n'obtient pas celles d'Alice.
Les deux conversations d'authentification restent privées et indépendantes.

## Résultats et limites de couverture

Les quatre profils du témoin de fermeture au commit
`4481e46aa6a84821a5adbaf11f82985c0404cf7a` réussissent : AArch64 et x86_64,
P9/512 et P9/4096. Bob ferme sa session avant la consultation autorisée d'Alice,
puis l'arrêt supervisé restitue les ressources observées.

Les essais des installations étrangères utilisent les invités du commit
`d995e0e04d9c53f622e549ca678042e550982c85` et le banc
`a4c16821d62d50d4f07379600036bcfbe9d63f2d`, puis le banc instrumenté
`18e54276296e97eed74eb9ab39570df39eb5624f` pour AArch64/P9-4096.
Les modifications intermédiaires du
banc et du relecteur hôte ne changent pas les binaires invités. Leur matrice
et les durées figurent dans l'archive, avec les échecs précédents conservés.

| Profil | Essai réussi dans `installation-d995e0e` | Exécution hôte | Attente verrou | Résultat |
| --- | --- | ---: | ---: | --- |
| AArch64/P9-512 | `r2-duplex-aarch64-512-essai-02` | 23,940 s | 0,000313 s | Bob/Alice 0, sorties exactes, drainage |
| AArch64/P9-4096 | `r2-duplex-aarch64-4096-essai-04` | 69,598 s | 0,000409 s | Bob/Alice 0, sorties exactes, drainage |
| x86_64/P9-512 | `r2-duplex-x86_64-512-essai-02` | 22,359 s | 0,000343 s | Bob/Alice 0, sorties exactes, drainage |
| x86_64/P9-4096 | `r2-duplex-x86_64-4096-essai-04` | 71,207 s | 0,000370 s | Bob/Alice 0, sorties exactes, drainage |

Ces quatre passages constituent 187,105 secondes d'exécution, hors attente.
Ce ne sont pas des fréquences d'échec : les essais antérieurs exerçaient des
révisions ou protocoles distincts. Leurs résultats négatifs restent archivés.
QEMU 11.1.3, TCG, deux vCPU, 256 Mio, aucun réseau invité. Les deux authentifications
coexistent ; les lancements de cette recette sont séquentiels.

Le processus Bob vérifie huit réponses de 128 octets, puis en restitue huit
extraits de 64 octets : consultation propre, annuaire d'Alice refusé, requête
d'ajout administratif directe refusée, consultation propre inchangée,
métadonnées privées noyau refusées, contexte forgé refusé, rejeu de trame
refusé et dernière consultation propre inchangée. Les statuts sont
`[0,5,5,0,5,3,3,0]`. Son stdout de 512 octets et celui d'Alice de 64 octets
sont comparés intégralement ; les deux stderr sont vides et les deux sorties
de processus attendues sont `0`. Le refus d'une trame administrative ne vaut
pas preuve de révocation d'un ticket `EXEC_TICKET`.

Les cinq tests Rust de distribution, dont le nouveau test Bob sur les deux
géométries et les deux architectures de manifeste, passent **sur l'hôte** :
faux signataire, signature altérée, destinataire inconnu/substitué, refus du
provisionnement par Bob et absence de modification des politiques. Les huit
tests de sessions et les 28 contrôles Python passent également sur l'hôte.
Ces résultats ne sont pas rebaptisés tests invités de publication : l'entrée
de provisionnement appartient ici à la fixture hors ligne et n'est pas un
port d'installation exposé à Bob.

Les neuf contrôles ABI passent avec Apple Clang ; la régénération est conforme,
avec 48 sélecteurs. Un premier passage échoue dans les en-têtes C++ de LLVM
Homebrew 23.1.6 pour la cible nue. Le journal négatif et les versions sont
conservés ; aucune assertion n'a été modifiée. Les quatre ELF v0.20.0 archivés
passent sur les sources QEMU `4481e46`, sans recompilation des applications.
Les ajouts suivants concernent le parcours Bob, la fixture et le banc, pas
les contrats historiques exercés par ces ELF.

## Ressources et persistance

Les mesures de chaque acteur restent à quatre racines maximum. Le cinquième
appel est refusé sans allocation conservée. Sur le témoin x86_64/P9-512,
Bob passe de 3 racines/4 handles à 4/5 pendant la consommation puis revient
à 3/4 ; Alice passe de 1/8 à 2/9 puis revient à 1/8. Les 59 endpoints et
124 nœuds reviennent au niveau précédant chaque ticket. Les cadres IPC
passent de 68 à 69 ; le relevé avant le ticket suivant revient à 68.

Les enfants restituent racines, handles, attentes, pages, pile et threads.
L'arrêt supervisé fournit ensuite `endpoints=0 nodes=0 espaces=0 cadres_ipc=0`.
Ce dernier contrôle est une restitution à l'arrêt global, pas une mesure
de plateau de tous les services conservés en activité. Les compteurs du
signataire et du registre du rapport de récupération concernent cette
récupération, distinctement des relevés de lancement.

Le relecteur hôte ouvre P9 et le témoin d'autorité locale **en lecture seule**.
Il compare les trois objets et le fichier synthétique aux références initiales.
La tête indépendante et le nombre de mutations administratives sont aussi
contrôlés dans le journal durable : aucune mutation administrative attendue
dans cette recette de refus. La récupération et le tmp peuvent modifier
l'image globale ; une comparaison brute de tout P9 n'est pas l'oracle.

## Diagnostics conservés

- Démarrage initial sur un mauvais mode d'ISO : refus NovaFS `CORRUPT` ; le
  profil correct est `novaauth-sessions`.
- Panique du vérificateur : les mots de démarrage duplex dépassaient la borne
  du lecteur historique. Lecture privée bornée, uniquement dans l'export R2.
- Enrôlement de la mauvaise époque de route dans le banc : refus sûr ; la
  fixture utilise désormais l'époque de la reprise approuvée.
- La table de sessions doit être commune aux deux routes pour éviter des
  identifiants dupliqués. Chaque route garde son contexte d'authentification.
- Fermeture Bob tardive : le vérificateur était arrêté avant son acquittement.
  Bob ferme désormais pendant que NovaAuth reste disponible ; Alice est
  ensuite vérifiée séparément.
- Collecte : lecteur série concurrent retiré, événement de fermeture raccordé
  à la destruction réelle, silence entre sondes distingué d'une panne et
  diagnostic explicite. L'ancien timeout AArch64 sans diagnostic complet
  ne reçoit pas rétrospectivement une cause certaine.
- Ancien parcours à sept lancements, x86_64/P9-4096 : `NSHEXARG` est refusé à
  73 732 ms pour une échéance de 73 348 ms, statut `5`. Les cinq assertions
  sont maintenant regroupées dans un même ELF et canal, sans prolonger le bail.
- Avec les deux refus de paquet supplémentaires, la consultation d'Alice
  réussit mais la commande de contrôle suivante peut dépasser son bail. Le
  protocole final lit `$?` et demande `exit` explicitement dans la même ligne.
  Les statuts shell/natifs, les octets et la restitution restent obligatoires.
- AArch64/P9-4096, essai 02 : observation hôte interrompue après 40 secondes
  sans résultat final de Bob. L'attente hôte passe à 80 secondes, sans changer
  TLS (10 s), les appels IPC, le bail (60 s) ni les statuts attendus.
- **Réserve du collecteur SMP :** l'essai 03 refuse une route répétée avant
  collecte des sorties. La cause exacte n'est pas établie. L'essai 04 ajoute
  CPU/PC et empreintes des relevés, conserve le refus des doublons et réussit.
  Ce succès ne prouve pas la disparition du défaut intermittent. Aucun contenu
  de session supplémentaire n'est écrit par ce diagnostic.
- **Réserve ouverte :** l'essai supplémentaire lançant Alice pendant le
  parcours Bob (`x86_64/4096`, `essai-02` de `installation-d995e0e`) termine
  Bob sur `TIMED_OUT=8`, sans enfant Bob, tandis qu'Alice réussit. La trace donne
  `attente owner=11 maintenant=10497 echeance=10903`, puis
  `refus owner=11 statut=8 maintenant=10903 echeance=10903` et une annulation
  terminale `kind=3 value=2`. Il s'agit de l'échéance du ticket pendant son
  admission, distincte du bail de session. La cause du délai cumulé sur le
  chemin partagé reste à analyser ; cet essai n'est pas déclaré qualifié et
  le parcours séquentiel ne le remplace pas.

## Suite R2, non acquise par ce point

Les six façades ELF existent mais ce banc exécute `admin-sonde`, pas les six
commandes administratives. Les préparations/confirmations et recherches
d'issue de mutation native restent à raccorder et qualifier ; Bob est déjà
refusé côté serveur avant ce raccord. Restent notamment : facteur préattesté
de création, protection transactionnelle du dernier administrateur dans
l'invité, acquittements réels des effecteurs, révocation du ticket Bob avant
expiration, réactivation avec nouvelle authentification, mutations concurrentes
et réponses perdues. Les anciens objets ne doivent jamais être réactivés.

La [matrice de sécurité](novaauth-administration-r2-securite-matrice-v0.24.0.md)
conserve ces lacunes. Aucune suppression définitive ni suppression de données
n'est ajoutée. Les réserves historiques de 301 cadres AArch64 instrumentés et
de reproductibilité inter-environnements restent ouvertes. Aucun changement
des sources du noyau ordinaire, des sélecteurs ABI ou de NovaFS P9 dans ce lot.
Aucune activation ordinaire, fusion ni tag ; les utilitaires restent en pause.

## Preuves

Archive durable :
`/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/`.
Les ISO, noyaux et configurations contenant les secrets de laboratoire restent
dans les archives privées sœurs, avec leurs copies exactes et empreintes.
Les résultats positifs et négatifs sont conservés, sans écrasement.

Chaque fichier `*-coordination.json` conserve acquisition/libération du verrou,
attente et exécution séparées, commande et empreintes des artefacts. Aucun
verrou tiers n'a été forcé. Le temps de diagnostic historique hors banc n'a
pas été chronométré continûment : aucune durée précise n'est inventée.
Les 558 preuves R1 et les sept manifestes R2 antérieurs ont été relus sans écart.

Le manifeste `SHA256SUMS-provisionnement-bob-20261122` couvre 1 282 fichiers
(272 327 991 octets), relus intégralement, sans écart ni lien symbolique.
SHA-256 : `462377a4cc19f92108fdadf5bb4f3d4d0c6ab6b6e6daa3126a206d1b5f622409`.
Le manifeste privé `artefacts-prives-administration-0240-r2/SHA256SUMS-installation-d995e0e`
couvre 179 fichiers exacts ; SHA-256 :
`1aec06ef317e0dd1ce4faa6bd6feefe014a8e57ba1aacfe76202c96f4072b995`.
Les secrets restent dans cette archive privée, hors Git et hors diffusion.

La recherche des 343 clés privées de laboratoire connues, en brut, hexadécimal
et base64, ne trouve aucune correspondance dans les nouveaux blobs Git et
les preuves publiques examinées (1 315 objets/fichiers). Ce contrôle ciblé
ne constitue pas une preuve universelle d'absence de secret.
