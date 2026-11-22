# NovaAuth v0.24.0 — Raccord administratif R1, qualification QEMU

État au 22 novembre 2026. Base `5c7f1bfa313a3859ccc0830660438e76d598acb1` ; sources invitées `45e7c0b5389492544dbc6a7e239cfa0755f7cd05`. Branche `codex/novaauth-administration-0240-phase1`. Ce bilan concerne le raccord R1 et la sonde privée `admin-sonde`, pas la livraison des commandes d'administration ni la qualification globale de NovaAuth.

## Contrat expérimental et frontière de confiance

NXADMIN1 est un **nouveau contrat de démarrage expérimental** de 80 octets, suivi des arguments canoniques. NXEXEC01 reste un contrat distinct de 64 octets. L'absence de nouveau sélecteur n'est pas utilisée comme preuve suffisante de compatibilité. Voir les [formats et capacités](novaauth-administration-phase1-prealables-v0.24.0.md#r1--route-administrative-delf-approuvée-pour-qemu).

L'installation attestée porte une portée administrative explicitement provisionnée. Le noyau remet avant toute instruction une feuille SEND/RECEIVE, sans GRANT/MANAGE, liée à l'enfant, son installation, sa session et son titulaire. Le vérificateur reçoit l'autre feuille ; le courtier ne reçoit ni requête ni droit administratif. L'enfant ne dispose d'aucun canal d'ouverture de fichier.

Pour **chaque requête complète**, le vérificateur contrôle le format NXACMD01 et relit les métadonnées immuables de la route noyau. Il vérifie le bail et l'époque puis produit un contexte privé. Le registre vérifie une tête authentifiée fraîche, le titulaire, le rôle et l'intersection des portées de l'installation. Le client ne peut pas injecter NXACTX01/NXASCP01 sur cette route. Le contexte du registre est consommé une seule fois. La possession de la feuille seule ne permet pas de consulter un autre compte.

Deux conversations disposent de réassembleurs séparés. L'attachement du canal se fait avant publication et première instruction de l'enfant ; un enfant provisoire peut déjà avoir été alloué, et doit être retiré si l'attachement échoue. La saturation par une troisième route et l'injection d'échec d'allocation ne sont pas qualifiées par la recette à deux conversations. Les séquences sont consommées dès la première trame ; les files restent bornées. Une réponse interrompue ne rend pas la séquence réutilisable. Le client conserve une issue UNCERTAIN et ne réémet aucune mutation automatiquement. **Les mutations administratives et leurs recherches d'issue restent refusées dans ce point de passage**, en attendant R2 et ses effecteurs.

## Recette exercée

Tous les fichiers, identités, paquets et clés sont synthétiques ; les secrets de laboratoire restent hors Git. Chaque essai indépendant utilise une nouvelle copie de son P9 initial. Les mêmes noyaux et registres attestés servent aux deux géométries de chaque architecture.

| Témoin | Résultat exigé |
| --- | --- |
| Consultation de soi, consultation de Bob hors portée, consultation de soi | 0, DENIED=5, 0 ; vues de 64 octets comparées, stderr vide |
| Trois demandes sur **le même canal** | Soi autorisé, Bob refusé, soi toujours autorisé ; aucune extension de portée |
| Rejeu d'une séquence consommée | BAD_ARGUMENT=3, aucune nouvelle opération métier |
| Trame avec champ réservé non nul | BAD_ARGUMENT=3 |
| Injection d'un contexte privé dans le canal client | BAD_ARGUMENT=3 avant transfert au registre |
| Lecture des métadonnées noyau par l'enfant | DENIED=5 |
| Perte de la seconde partie d'une réponse | UNCERTAIN=22, feuille fermée, aucune répétition |
| Deux conversations réellement coexistantes | Deux PID ; 0/0 et pipeline 0 ; producteur bloqué sur neuf trames dans un pipe de huit avant la seconde requête complète |
| Expiration du bail inchangé de 60 s | Ancienne feuille REVOKED=10, seconde utilisation refusée, enfant collecté |
| Nouvelle authentification explicite | Nouvelle session, nouveau PID, consultation autorisée ; pas de réactivation du bail expiré |
| Ressources | Tous les enfants restitués, racines ≤4, retour des compteurs après drainage |

La révocation exercée ici est celle des délégations à l'expiration du bail et à la fermeture de l'instance. Elle ne remplace pas la future qualification de désactivation/réactivation d'un utilisateur, du retrait durable d'époque ou des origines de droits de groupe.

## Résultats et reproductibilité

Les empreintes et rapports détaillés sont référencés dans la [matrice de validation](validation/novaauth-administration-r1-v0.24.0.json). Les essais ne sont pas déclarés réussis sur la seule base des compilations ou tests hôtes.

| Profil | Parcours R1 | Enfants collectés | Connexions TLS successives | Temps du banc |
| --- | --- | --- | --- | --- |
| x86_64/P9-512 | Réussi | 13 | 233 | 109,351 s |
| AArch64/P9-512 | Réussi | 13 | 234 | 152,463 s |
| x86_64/P9-4096 | Réussi | 13 | 257 | 257,917 s |
| AArch64/P9-4096 | Réussi | 13 | 257 | 288,524 s |

Les valeurs 512 proviennent de l'ordonnancement initial, celles 4096 des groupes réauthentifiés explicitement. Les binaires invités sont identiques entre géométries pour une même ISA.

Sur les profils 512, la recette initiale a réussi avec son ordonnancement initial. Sur x86_64/P9-4096, la première tentative a refusé une demande à 68 304 ms pour un bail expirant à 66 320 ms. Le refus sûr est conservé. Le banc suivant demande une nouvelle authentification **entre groupes indépendants**, jamais au milieu d'une conversation ou d'un pipe. Aucun délai invité n'est augmenté ; l'essai d'expiration reste distinct. La borne de 360 s du processus hôte limite l'exécution du banc, pas les opérations TLS ou les baux de sécurité.

## Compatibilité des contrats et des artefacts

Les neuf contrôles ABI sur l'export R1 réussissent. Le registre public et ses 48 sélecteurs ne changent pas. Onze tests hôtes couvrent les codecs, dont les assertions historiques NXEXEC01 ; seul le décodage des arguments commun a été extrait, avec les mêmes bornes et règles. Les en-têtes NXEXEC01 et NXADMIN1 se refusent mutuellement.

Les noyaux ordinaires ont été reconstruits depuis la base et les sources invitées R1, dans le même chemin, avec deux répertoires de compilation indépendants, Rust 1.99.1 stable, release/offline/locked et aucune feature QEMU :

| Architecture | SHA-256 identique base/candidat |
| --- | --- |
| x86_64 | `ce9233480afb4de06033f9233be65b9fbc51070ed779e521ac845afc39d15117` |
| AArch64 | `dc7ad2524e704eeee52bf22f7136a2ec4c5c61cbd76ed81f2c84e9f6f364a7d3` |

Leur contenu ne contient pas NXADMIN1. L'export explicite est nécessaire pour inclure le module privé ; sa simple présence dans les sources ne l'active pas. L'identité binaire est démontrée ici dans un environnement fixé, pas entre toolchains ou environnements différents.

La compatibilité des quatre ELF v0.20.0 a réussi séparément avec les artefacts archivés, sans recompilation de ces applications ; les résultats figurent dans la matrice associée. Le profil de ces témoins ajoute `thread-probe` aux sources invitées R1. Il ne s'agit pas d'une qualification de production.

## Défauts diagnostiqués et corrections

- Le banc Python retournait un résultat d'échec sans propager son code au processus. Le point d'entrée utilise désormais SystemExit(main()). Les JSON négatifs initiaux sont conservés.
- Le shell du profil R1 fermait le ticket du producteur avant la confirmation durable de sa création. WOULD_BLOCK interrompait alors l'admission du lecteur. Le profil réutilise l'attente bornée de fermeture déjà existante, sans répéter SPAWN ni contourner le drainage.
- Le témoin de pipe utilisait un tampon plus petit que les 128 octets requis pour recevoir une trame IPC. Sa correction conserve les assertions historiques et bloque réellement le producteur avant la fin de sa requête, au lieu de seulement rapprocher les deux commandes dans le temps.
- Le banc hôte cessait de servir à la 128e connexion TLS. La recette R1 étendue emploie une borne explicite de 256 connexions, puis 320 pour les quatre groupes supplémentaires réauthentifiés, sans modifier la borne historique des autres recettes. Les compteurs et l'épuisement sont enregistrés : le rejeu à 256 a atteint sa borne après la sortie réussie du dernier enfant, avant la collecte. Les 64 échanges supplémentaires sont une enveloppe hôte de 16 par groupe ; une seule connexion est servie à la fois. Ce compteur de parcours n'est pas un quota IPC ni un délai de sécurité.
- Même après cette correction, le banc armait son timeout de négociation sur un socket préconnecté, **avant ClientHello**. L'attente de 60 s faisait fermer le relais : signataire op=2, CLOSED=11 ; registre REFUSED=12 ; vérificateur DENIED=5. L'attente inactive est désormais distincte et interruptible par l'arrêt global du banc ; les horodatages sont archivés. La limite TLS invitée de 10 s et celle de la négociation hôte restent inchangées.
- La validation du client n'était pas une frontière de confiance suffisante : le vérificateur décode désormais NXACMD01 lui-même avant tout transfert sur le port du registre. Le témoin d'injection de contexte exerce ce refus.

- L'analyseur de trace acceptait le préfixe `sortie=1` avant réception du `0` final de `sortie=10`. Il attend maintenant une fin de ligne. Le témoin rejoue chaque coupure possible de cette vraie ligne ; la trace négative initiale reste archivée.

## Ressources et portée des mesures

L'analyseur [administration_r1_resources.py](../user/novaauth/autorite_qemu/administration_r1_resources.py) relit les PID et restitutions du vrai invité. Il exige zéro racine, handle, attente, page enfant, pile et thread après chaque sortie. Les fermetures de tickets pendant un pipe sont des points intermédiaires : le contrôleur et le lecteur peuvent encore exister. Le retour au niveau de référence est exigé après drainage et avant le prochain lancement indépendant. Toutes les valeurs intermédiaires restent archivées.

Au repos entre lancements : nsh possède deux racines et huit handles ; le total observé est de 49 endpoints, 102 nœuds et 15 processus. Les deux fermetures intermédiaires du pipe gardent temporairement d'autres ressources ; le prochain lancement indépendant retrouve cette référence. Les enfants administratifs culminent à trois racines, les services à quatre au maximum. Le témoin historique de cinquième racine est refusé.

| Pages résidentes observées pendant R1 | x86_64, deux géométries | AArch64, deux géométries |
| --- | ---: | ---: |
| NovaFS | 170 | 167 |
| Vérificateur NovaAuth | 34 | 30 |
| Registre isolé | 136 | 129 |
| Signataire serveur | 65 | 57 |

Ces valeurs restent constantes dans les points R1 capturés. Les 58 cadres IPC avant lancement et 59 au point de fermeture ne sont pas confondus : la page de collecte se draine après fermeture, et les lancements indépendants suivants retrouvent 58. Le dernier échantillon reste à 59 ; ce rapport n'annonce pas une restitution physique instantanée à ce point. Le cache NovaFS a déjà atteint son niveau résident avant les mesures R1 ; sa rétention historique n'est pas attribuée aux enfants administratifs.

Les pages résidentes d'un service ne sont pas confondues avec des pages actives, ni déclarées rendues à l'allocateur physique. Les mesures n'effacent pas la réserve historique des **301 cadres AArch64 instrumentés**. La compatibilité fonctionnelle, l'identité SHA-256 des artefacts et la reproductibilité entre environnements sont trois constats distincts.

## Preuves, coordination et conservation

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase1-prealable-5c7f1bf`. Le manifeste `SHA256-r1-final.txt` couvre **558 fichiers**, relus intégralement ; SHA-256 `15aaac3e6c4dc653e55eb7dbbb284be8c5aade3eaa83a5bb5103c00c3b2893b8`. Les 35 preuves du premier point de passage restent concordantes. Les **242 fichiers privés**, comprenant les artefacts exacts et les secrets de laboratoire nécessaires à la reproduction, sont conservés dans des répertoires protégés hors Git et vérifiés par un manifeste privé distinct. Les noyaux exacts ont été comparés à leurs copies durables ; les registres archivés sont effectivement inclus dans ces noyaux.

Le verrou commun est acquis/libéré pour chaque campagne, sans campagne concurrente détectée. Les heures et durées de chaque passage, positif comme négatif, figurent dans `r1-coordination-campagnes.json`. Attente totale : 0.006 s ; exécution cumulée des bancs conservés : 2380.309 s. Le dernier chiffre inclut la construction des quatre profils de compatibilité. Le diagnostic hors banc n'a pas été intégralement chronométré ; aucune durée n'est inventée. Les quatre compilations ordinaires représentent séparément 54.825 s de compilation.

Le contrôle ciblé des six clés privées de laboratoire connues (brut, hexadécimal, base64) ne retrouve aucune occurrence dans les archives publiques ni les nouveaux objets Git. Les clés, ISO et noyaux de qualification contenant les signataires ne sont pas publiés. Les preuves négatives ne sont ni supprimées ni réétiquetées réussies.

## Restant à réaliser

R2 reste incomplet : publication des mutations d'administration, notifications et acquittements des effecteurs, admissions liées à l'époque utilisateur et recettes de désactivation/réactivation. Les bibliothèques et tests hôtes préparatoires ne constituent pas cette qualification invitée. Les façades useradd/usermod/groupadd/id/whoami/groups restent à raccorder et qualifier conformément à ARC-026.

Conserver l'interdiction de désactiver/supprimer l'administrateur actuel, l'absence de commandes de suppression de compte/groupe, et la conservation des données, propriétaires et protections. Une réactivation permettra uniquement une nouvelle authentification avec les droits actuels ; elle ne rétablira aucun ancien objet révoqué.

Aucune activation ordinaire, fusion, création de tag ou reprise des utilitaires parallèles. Autorités, signataires et provisionneurs restent des composants de laboratoire. UTM et matériel physique ne sont pas qualifiés par cette campagne.
