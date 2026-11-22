# NovaAuth v0.24.0 — R2 : création avec facteur préprovisionné

Raccord QEMU approuvé par décision MOA/MOE du 23 novembre 2026 ; qualification en cours. Le CLI `useradd --factor-reference` existe,
mais, avant ce raccord, le registre n'avait aucun inventaire de facteurs attestés encore non affectés.
Les facteurs du triplet courant sont déjà associés à une identité ; une référence
libre reçue en argument ne constitue pas une attestation.

## Raccord minimal proposé

Le provisionneur de laboratoire fournit un catalogue de **clés publiques**, figé
avec les sources de qualification et incorporé au seul registre QEMU. Il ne
contient aucune clé privée et n'est pas modifiable par les ELF administratifs,
NovaFS ou le courtier. Le modèle de confiance de cette attestation est explicitement
celui du provisionneur et de l'image QEMU vérifiée, pas une autorité de production.

Format canonique : `NAR2FAC1` (8 octets), domaine (16), nombre de clés (u16 LE),
6 octets réservés nuls, puis exactement `nombre * 32` octets de clés Ed25519.
Borne : 16 clés, 544 octets au total. Rejeter les tailles, domaines, réservés,
clés invalides ou faibles et doublons. La référence est calculée avec le domaine
et le préfixe de facteur déjà utilisé par NovaAuth. Les SHA du catalogue public et
du registre compilé figurent dans les preuves. Aucune racine IPC supplémentaire.

Lors de PREPARE et CONFIRM, le registre doit vérifier la session et les capacités
actuelles d'Alice, l'appartenance de la référence au catalogue, l'absence d'usage
antérieur et le groupe demandé. Le plan existant produit une identité stable
neuve, sans privilège administratif, puis les trois objets sont publiés par NovaFS
avec le reçu durable de l'autorité. L'unicité transactionnelle du facteur reste
celle de State/apply. Aucun fichier privé n'est ouvert ou déplacé.

L'inscription ne constitue pas une authentification : l'utilisateur doit ensuite
prouver la possession de sa clé privée par le contrat actuel. Le catalogue ne
peut ni réactiver une session ni réattribuer une identité ancienne. Aucun ajout
ou remplacement dynamique du catalogue n'est autorisé par cette proposition.

## Recettes et limites

Créer et consulter une nouvelle identité depuis nsh ; référence inconnue,
facteur déjà utilisé, groupe absent et demande Bob refusés ; confirmations
concurrentes, réponse perdue, arrêt/reprise et absence de seconde identité.
Vérifier la conservation des données privées, des droits d'Alice et du dernier
administrateur. Exécuter sur les quatre profils QEMU.

Le catalogue constitue le raccord d'attestation approuvé pour remplacer le refus
initial par une création effective dans le profil de qualification. Il ne
s'agit ni d'une commande de gestion des facteurs, ni d'un enrôlement de production.
Le raccord des reçus persistants a reçu son autorisation distincte.
