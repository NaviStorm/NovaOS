# NovaAuth v0.24.0 — R2 : résultats administratifs persistants

Statut : raccord QEMU approuvé par décision MOA/MOE du 23 novembre 2026 ; qualification en cours. Référence :
`46d04ff29e9c22e26c8d4261ba4c742c132eec67`. Profil QEMU uniquement.

## Défaut observé dans les sources

`administration_mutations.rs` vérifie l'historique signé de l'autorité, puis exige
une entrée dans `completed`, table volatile limitée à deux résultats. Après
redémarrage ou éviction, une opération terminée reste `UNCERTAIN`. Supprimer ce
contrôle sans preuve de remplacement serait incorrect : le type historique
`UPDATE=4` ne distingue pas un renommage d'un futur retrait d'appartenance dont
les effecteurs doivent acquitter la révocation.

## Changement proposé

Conserver intégralement les formats et types historiques 1 à 4. Ajouter au champ
privé `kind` du corps PUBLISH de 209 octets : 5=renommage, 6=création de groupe,
7=ajout d'appartenance. Le champ reste un octet, offset 208 ; aucun changement
ABI publique, sélecteur, capacité, racine IPC ou format disque P9. Ces trois
opérations ne ferment aucune origine de droit. Leur sujet de retrait et époque
de retrait doivent être nuls ; le registre dérive le type du plan transactionnel,
jamais d'une valeur choisie par le CLI. L'autorité et le signataire rejettent les
valeurs et combinaisons non canoniques. UPDATE=4 conserve ses exigences antérieures.

Une consultation d'issue doit toujours :

1. Authentifier la session et vérifier les capacités administratives actuelles.
2. Relire une tête fraîche de l'autorité et vérifier son égalité avec le triplet
   P9 activé. Un état restauré ne suffit jamais.
3. Demander HISTORY avec l'identifiant initial et un défi frais, vérifier sa
   signature, le serveur/domaine et l'auteur authentifié d'origine.
4. Vérifier que la génération historique est incluse dans la tête courante ;
   à génération égale, exiger la même empreinte de triplet.
5. Pour les types explicitement non révocatoires, reconnaître le résultat durable
   sans dépendre de la table volatile. Cette reconnaissance ne répète aucune
   mutation et n'accorde aucun droit historique.
6. Pour une désactivation, exiger séparément le reçu terminal RETIREE correspondant
   au sujet, à l'époque, à l'opération et à la génération. Un reçu de publication,
   une réponse perdue ou un timeout ne démontrent pas le drainage.
7. Laisser UNCERTAIN pour tout cas dont l'activation ou les effecteurs ne sont pas
   établis, notamment UPDATE historique sans preuve suffisante.

## Qualification préalable à toute livraison

Tests hôtes des encodages, types inconnus, sujets/époques non canoniques,
signatures, auteurs, têtes divergentes et reçus trop récents. Préserver les
assertions historiques. Redémarrer les objets d'autorité sur la même base durable
et vérifier qu'une consultation ne crée aucune seconde mutation.

Sur chaque profil QEMU : mutation native, perte du reçu, arrêt complet,
redémarrage avec l'autorité conservée, reprise approuvée et nouvelle authentification,
consultation du même identifiant, vérification P9 et absence de seconde mutation.
Contrôle négatif : génération non activée, retrait sans acquittement terminal et
référence appartenant à un autre administrateur restent refusés ou UNCERTAIN.

## Historique de l’arbitrage

Le contrôle automatique d'approbation a refusé le changement, au motif qu'il
modifie le protocole privé et les critères de succès durables après redémarrage.
La décision MOA/MOE a ensuite autorisé ces types additifs et la preuve durable
de remplacement. Le code a été appliqué seulement après cette autorisation. Le raccord des
facteurs de création et celui des effecteurs d'appartenance restent des travaux
séparés ; cette proposition ne leur accorde aucune nouvelle autorité.
