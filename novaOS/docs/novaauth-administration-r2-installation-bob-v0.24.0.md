# NovaAuth v0.24.0 — Installation du témoin non administrateur R2

État : raccord ciblé autorisé par la MOA/MOE, implémentation privée R2 ; qualification invitée en cours. Base de travail `33d87d8470b533eecf117054a6095bab2a8fd1a2`, branche `codex/novaauth-administration-0240-phase1`.

## Constat dans les interfaces

`novaOS/user/novaauth/src/distribution.rs::Registry::publish_package` exige que
`holder.owner` soit actif **et administrateur**, puis publie l'installation dans
les paquets de ce même titulaire. `View::executable` cherche exclusivement dans
les paquets du titulaire authentifié. L'installation d'Alice ne rend donc aucun
ELF accessible à Bob, conformément à l'isolation attendue.

`Registry::provision_install` atteste une identité d'installation pour un
propriétaire explicite, mais ne publie pas les entrées de paquet nécessaires à
la résolution par nom. Les témoins Bob historiques supervisés ne démontrent pas
le lancement générique d'un paquet sous une identité non administrative.

Le provisionneur de la nouvelle image jetable a rencontré `Denied` en tentant
`publish_package` avec le grant de Bob. Ce refus n'est pas un défaut du contrôle
d'accès. Bob n'a pas été promu administrateur, même temporairement. L'image
incomplète et le diagnostic sont conservés ; aucune recette QEMU n'a démarré sur
cette image.

## Raccord privé autorisé

Ajouter une entrée **privée de provisionnement QEMU** distinguant :

- l'administrateur authentifié autorisant la publication ;
- le titulaire stable actif destinataire de l'installation, ici Bob ;
- la clé publique de distribution explicitement admise ;
- le paquet signé, son architecture et ses octets vérifiés ;
- la tête attendue et l'identifiant unique de la transaction.

L'entrée historique `publish_package` conserve ses arguments, ses refus et ses
effets. La nouvelle entrée est réservée au provisionnement du volume jetable,
absente des commandes natives et des canaux administratifs R1/R2. Elle réutilise
la validation de signature, la publication transactionnelle et l'attribution
d'une identité d'installation non réutilisable du moteur existant.

L'autorité d'installation appartient au provisionneur ; Bob reste sans capacité
administrative ni droit de publier des paquets. La création vise exclusivement
son catalogue et ne copie ni politique d'Alice, ni accès VFS, ni session, ni
capacité. La portée du canal de consultation `NXADMIN1` de Bob reste `SELF` ;
les mutations sont refusées côté NovaAuth.

Une cible inactive, une autorité non administrative, une tête obsolète, une
signature invalide, une architecture incompatible, un nom occupé ou une réponse
incertaine entraînent les refus et la réconciliation historiques. Aucun repli
vers l'installation d'un autre utilisateur n'est permis.

## Qualification à joindre

1. Refus de la publication historique par Bob, conservé comme témoin négatif.
2. Publication explicite pour Bob par le provisionneur autorisé, sans changement
   des attributs administratifs, groupes ou droits privés d'Alice et Bob.
3. Refus si Bob se désigne lui-même comme autorité de publication.
4. Refus d'une cible, signature, tête ou architecture substituée.
5. Résolution puis lancement réel par Bob dans sa propre session ; octets
   exécutés attestés et admission distante obligatoire.
6. Administration par Alice, révocation du ticket Bob avant expiration et
   absence de création de processus après acquittement effectif.
7. Quatre profils QEMU, comptage des ressources et contrôles historiques.

Ce raccord n'ajoute aucun sélecteur, aucune capacité publique, aucun accès au
volume général, aucune commande d'installation ni activation ordinaire. Son
activation reste limitée au provisionnement du volume jetable R2.

## Portée de la signature et destinataire

Le manifeste de distribution historique atteste le paquet, sa version, son
architecture et les octets ; il ne contient pas de nom de compte. Ce format
n'est pas modifié. Le provisionneur authentifié choisit explicitement l'identité
stable destinataire dans `provision_package_for_user`, avec sa tête attendue.
La transaction lie le catalogue et l'identité d'installation à ce destinataire.
Le chargeur du registre isolé ne résout ensuite que le catalogue du titulaire
réel de la session, sous tête indépendante fraîche. Présenter le nom ou les
octets d'un paquet d'Alice ne donne donc aucun accès à Bob. La vérification
cryptographique historique reste conservée à la publication, puis l'empreinte
et l'objet attesté sont contrôlés au chargement.

L'entrée privée est compilée uniquement avec `administration-lab` et n'est
appelée que par la fixture du banc, pas par NXADMIN1 ou une commande native.
Elle ne délivre pas de capacité d'installation au destinataire. Les refus
précédent toute publication du registre ; le reçu de la même opération ne peut
pas être réaffecté à un autre destinataire.

## Validation intermédiaire du moteur

Le test `provisionnement_explicite_bob_sans_promotion_ni_heritage` exerce les deux
architectures de manifeste et les deux secteurs P9 dans le moteur hôte : Bob
non administrateur, paquet d'Alice inaccessible, faux signataire, signature
altérée, destinataire inconnu, substitution du destinataire à identifiant de
transaction constant, absence de mutation après refus, installations et
résolutions séparées. Les quatre autres tests historiques de distribution et
les huit tests d'époques de session réussissent. Ces résultats ne sont pas des
preuves QEMU et ne qualifient pas la révocation effective des deux processus.
