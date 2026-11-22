# NovaAuth v0.24.0 — R2 : isolation réciproque et collecte du banc

Référence de départ : `0e0d6ca41e2c8c1b4d5bffb1ec1d51806f539516`.
La MOA/MOE a approuvé les conversations duplex et une collecte distincte en
qualification QEMU. Sa précision suivante impose une isolation **réciproque** :
Bob n'accède pas davantage aux sorties, à la console ou aux données d'Alice.
Le collecteur appartient exclusivement au banc ; il ne délègue aucun droit aux
sessions. Les utilitaires restent en pause.

## Frontière retenue

Le collecteur Python s'exécute sur l'hôte de qualification. Une sonde privée,
incluse uniquement dans l'export `--administration-r2`, lui permet de lire les
trames **déjà livrées** par le chemin IPC natif. Elle ne consomme pas les messages
à la place du destinataire, ne fabrique pas d'acquittement et ne modifie ni
ordonnancement, ni mémoire, ni résultat IPC depuis GDB.

La sonde est un point d'observation noyau, pas une primitive publique. Son
symbole `nova_r2_output_observe` n'est relié à aucun sélecteur. Aucun port IPC,
handle de collecte, droit CONSOLE ou accès fichier n'est remis à Alice ou Bob.
Le débogueur hôte est une autorité de laboratoire privilégiée ; cette observation
n'est ni une interface d'exploitation ni une garantie contre un hôte compromis.

Les deux shells gardent chacun le traitement de leurs propres flux selon leur
contrat de lancement. Ce traitement ne doit pas être confondu avec le collecteur
de preuve externe. Le futur mode Bob doit drainer ses seuls flux enfants sans
les rendre sur le terminal d'Alice. Il reste à raccorder et qualifier : la sonde
ne donne pas à Bob le droit de console actuellement exigé par `attach_display`.
La garde terminal actuelle demeure donc intacte à cette étape.

Le collecteur n'offre aucune méthode distante accessible à une session. Les
fichiers de preuve demeurent sur l'hôte, hors des volumes invités. Aucune lecture
des preuves par un des shells ne fait partie du raccord. Les essais emploient
uniquement les contenus synthétiques du banc.

## Identités et point d'observation

La liaison d'un flux est construite à partir des objets du noyau :

- PID et identité complète de l'instance `nsh` destinataire ;
- PID et identité complète de l'enfant ;
- identifiant de session authentifiée ;
- référence générationnelle de l'endpoint ;
- direction stdout ou stderr.

La sonde est enregistrée **après** l'établissement de `session_origin` par le
lanceur. `owns_session` et `exec::session_child` restent obligatoires. Le nœud
émetteur appartient à l'enfant et possède seulement SEND ; le destinataire
possède une feuille RECEIVE dérivée de cette même racine. Ni argv ni un PID
fourni dans le contenu d'une sortie ne choisissent cette liaison.

Une première tentative d'enregistrement avant `session_origin` était trop tôt.
Le contrôle automatique a refusé la proposition de retirer `session_child`.
La garde est conservée ; seul le point d'observation a été déplacé. Aucun refus
de sécurité n'a été contourné.

`traffic::deliver` invoque la sonde après validation de l'adresse destinataire,
copie utilisateur réussie, consommation de la file et achèvement de l'envoi.
Un SEND refusé, bloqué ou abandonné n'est pas compté comme un octet livré. Le
collecteur exige des séquences strictement consécutives et rejette toute
substitution de session, d'instance, de PID, d'endpoint ou de direction.

Une erreur de sonde invalide la preuve ; elle n'autorise ni ne refuse un lancement
à la place du système. La fin d'une observation ne prouve pas la collecte du
processus : le WAIT natif et les compteurs de ressources restent nécessaires.

## Encodage de la sonde

Tous les entiers sont non signés, en petit-boutiste. Taille fixe : 272 octets,
stockés temporairement dans la pile noyau ; ce n'est pas une enveloppe IPC ni
une métadonnée de liaison du transport de l'autorité.

| Offset | Taille | Champ |
| ---: | ---: | --- |
| 0 | 8 | `NXR2OUT1` |
| 8 | 2 | Version 1 |
| 10 | 2 | Taille 272 |
| 12 | 1 | Événement : 0 liaison, 1 livraison, 2 retrait d'observation |
| 13 | 1 | stdout = 1, stderr = 2 |
| 14 | 2 | Nombre d'octets utiles, 1 à 128 pour une livraison ; zéro sinon |
| 16 | 8 | PID du shell destinataire |
| 24 | 8 | PID enfant |
| 32 | 32 | Identité complète du shell |
| 64 | 32 | Identité complète de l'enfant |
| 96 | 32 | Identifiant de session |
| 128 | 8 | Référence générationnelle d'endpoint |
| 136 | 8 | Séquence par flux : liaison 0, incrément de un ensuite |
| 144 | 128 | Octets bruts ; bourrage nul au-delà de la longueur |

Un événement 255 invalide explicitement l'observation. Les versions, tailles,
longueurs ou bourrages invalides sont refusés. Aucun échappement de tabulation,
NUL, octet non UTF-8 ou contrôle terminal n'est appliqué au contenu archivé.
Le collecteur ne rend pas ces octets dans un terminal utilisateur.

## Budget et limites

La sonde ne crée aucun processus, racine, endpoint ou handle IPC. Le budget de
la [topologie à deux sessions](novaauth-administration-r2-deux-sessions-v0.24.0.md)
n'est donc pas augmenté par un collecteur invité supplémentaire.

Le registre de sonde est borné à huit flux, correspondant à deux routes avec
deux enfants et deux sorties chacun. Une assertion de compilation borne sa
représentation à 1 536 octets ; une trame de 272 octets est construite sur la pile
pour chaque observation. Ces bornes ne remplacent pas les mesures de cadres
pendant la qualification. Les références périmées sont retirées avant réemploi
d'un emplacement ; aucune référence d'endpoint n'est recyclée par la sonde.

Le collecteur hôte accepte au maximum huit flux actifs, 256 liaisons par parcours
et 1 Mio d'octets de sortie. La saturation échoue explicitement, sans troncature
ni qualification positive partielle. Ces limites du banc ne modifient aucun
budget NovaOS. Les journaux et données déjà reçus sont conservés.

GDB lit les registres d'arguments, puis exactement 272 octets de mémoire. Le
collecteur n'utilise aucune commande d'écriture mémoire ou registre. Les points
d'arrêt peuvent modifier les durées observées : distinguer passages ordinaires
et instrumentés, conserver les échéances natives et ne pas augmenter les délais
pour accepter un passage instrumenté. Aucune mesure de performance de production
ne peut être déduite de cette collecte.

## Recettes bloquantes pour les quatre profils

1. Authentifier les deux instances et relever leurs identités et sessions
   distinctes depuis les routes noyau ; ne pas accepter un nom déclaré par argv.
2. Émettre des témoins binaires différents sur stdout et stderr depuis chaque
   session. Comparer chaque flux octet par octet, avec sa propre liaison.
3. Présenter les références de sortie d'Alice depuis Bob, puis celles de Bob
   depuis Alice. Exiger le refus natif et zéro octet étranger livré.
4. Vérifier le refus des accès console et fichiers croisés. Le droit administratif
   d'Alice ne lui confère pas de droit de lecture des fichiers privés de Bob.
5. Vérifier que Bob ne possède aucun port de console d'Alice et qu'aucun octet de
   Bob n'apparaît dans le flux console présenté à Alice. Les métadonnées de
   qualification doivent rester distinctes des sorties utilisateur.
6. Corrompre les liaisons et séquences du collecteur : le banc doit refuser la
   preuve, sans requête de réparation ou nouvelle capacité envoyée à l'invité.
7. Faire révoquer Bob par Alice et présenter son ticket avant expiration : exiger
   REVOKED, aucun enfant interdit et maintien des opérations légitimes d'Alice.
8. Fermer chacune des sessions dans les deux ordres, puis vérifier l'autre
   session, les décisions serveur et la restitution réelle des ressources.
9. Rejouer les recettes R1/R2 affectées, les neuf contrôles ABI et les quatre ELF
   historiques. Les modifications des quotas ou attentes historiques restent
   interdites.

## Résultats de cette étape

- Neuf tests hôtes du collecteur réussis : séparation des quatre flux,
  conservation binaire, liaisons étrangères, rejeux, séquences manquantes,
  malformations, saturation et commandes GDB exclusivement en lecture.
- Compilation des deux noyaux QEMU R2 réussie avec la sonde et l'authentification
  duplex. Les sources ordinaires du noyau, l'ABI et NovaFS P9 ne sont pas modifiées.
- Les neuf contrôles du registre ABI réussissent sur les sources et l’export.
  Le premier passage dans l’export manquait de l’historique Git (quatre erreurs
  de préparation conservées). Le rejeu utilise cet historique en lecture seule ;
  aucune assertion n’a été modifiée.
- Aucun essai QEMU ni test de deux sessions invitées exécuté dans cette étape.
  Aucune qualification R2 complète, mesure IPC dynamique ou compatibilité ELF
  historique supplémentaire n'est revendiquée.

Les réserves historiques des 301 cadres AArch64 instrumentés et de la
reproductibilité entre environnements restent ouvertes. Le raccord à deux routes,
le mode Bob sans console, les mutations R2 et leurs effecteurs restent à terminer
avant la campagne obligatoire. Aucun besoin d'élargir les droits de collecte n'a
été établi. Aucune fusion, aucun tag et aucune activation ordinaire.

## Références et conservation des preuves

Code : `36f013703059905442fd5a67baaa2c261ff8f3fb`.
Les 527 fichiers Rust comparés dans les composants compilés sont identiques
entre l'export préparatoire utilisé pour les deux constructions et l'export
recréé depuis ce commit. Cela établit l'identité des sources exportées comparées,
pas la reproductibilité de compilations entre environnements.

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/collecte-exclusive-01`.

Manifeste : `SHA256-collecte-exclusive-01.txt`, **36 fichiers relus** ; SHA-256 :
`fc1bea6681582589b41d60891186c91a1b906a96d4a043624be7903682c011ca`.
Les deux binaires exacts restent dans l'archive privée durable indiquée par
`artefacts-prives.json`, avec correspondance SHA-256 vérifiée ; ils ne sont pas
publiés dans Git. Les traces de l'échec de préparation ABI sont conservées.

Aucun verrou QEMU acquis, aucune attente de verrou ni exécution QEMU à compter
pour cette étape. La matrice des quatre profils reste non exécutée pour la
nouvelle collecte. Les preuves et qualifications antérieures sont conservées.
