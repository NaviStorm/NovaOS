# NovaAuth R2 — Extension ciblée du témoin Alice/Bob

## Verdict du 22 novembre 2026

**Arrêt au premier échec : x86_64/P9-4096 ne qualifie pas le témoin.** Un seul
essai a été exécuté. Aucune nouvelle tentative, aucun démarrage AArch64 ni aucune
campagne supplémentaire n'a suivi cet échec. Ce commit publie uniquement le
diagnostic, la matrice et les références des preuves ; aucun correctif fonctionnel.

| Profil | Essais de cette étape | Résultat |
| --- | ---: | --- |
| x86_64/P9-512 | 0 | Acquis antérieur accepté, non rejoué ; refus REVOKED avec 996 ms de marge |
| x86_64/P9-4096 | 1 | ÉCHEC avant émission du ticket vivant ; révocation non exercée |
| AArch64/P9-512 | 0 | NON EXÉCUTÉ : arrêt au premier échec |
| AArch64/P9-4096 | 0 | NON EXÉCUTÉ : arrêt au premier échec |

L'acquis 512 reste celui du [rapport précédent](novaauth-administration-r2-effecteurs-v0.24.0.md).
Son refus est observé après effet local et avant le reçu durable final, vérifié
ensuite : ne pas présenter ce résultat comme une tentative après réception de ce reçu.

## Identité du parcours

- Commit de référence local et distant : `a079427cb31274b5235e206735448231f3a6900e`.
- Arbre des sources invitées : `e7ddc8ecea2220ce205a443dcd01de4203f6dbb8`.
- Arbre du banc corrigé : `a7f86df67f28882d4ae17345d46ab177ff3ac182`.
- ISO x86_64 réutilisée à l'identique : `4fd9df53602629bf04b12894da84d391e6e6131745585a172302e26085c8e781`.
- Noyau : `11cb329d6bb9e38bde2ef8f7e9f27050d7cfa65efb9d8ca69e1366f6bcc0ed66`.
- Registre : `2f0cb73249f1da2d1375819942934ffc4673f772801b11bfc18a1b1af2a6cb9c`.
- Genèse P9/4096 : `43c92266917eac0e41231c8997de2812a34223a219eb9ad49588b349586d7dd1`.
- Ancien disque d'autorité locale : `105acaa8a0f3cbc84af09633c21a227b1622fa5a742e4d2cbccbfc87551c523a`.
- QEMU 11.1.3, TCG, deux vCPU, 256 Mio, `-nic none`, secteurs 4096.

Les artefacts et la genèse ont été relus contre leurs manifestes antérieurs,
puis copiés dans les archives de cette étape. Aucune recompilation. Les différences
entre l'arbre invité et la référence sont documentaires et dans le banc hôte ;
entre l'arbre du banc et la référence, seul le rapport documentaire diffère.
Le banc `administration_commandes_qemu.py` est inchangé. Le wrapper ne change que
les chemins, la référence et la géométrie ; aucune réauthentification additionnelle.
La commande complète, les arguments et empreintes sont dans la
[coordination](validation/novaauth-r2-extension-ticket-20261122-coordination.json).

## Première divergence et diagnostic

| Événement invité | Temps monotone, ms | Résultat |
| --- | ---: | --- |
| Défi Bob émis / vérifié / limite | 12368 / 13368 / 13868 | Authentification réussie, défi valide |
| Début / échéance du bail Bob | 14412 / 74412 | Bail inchangé de 60 secondes |
| Défi Alice émis / vérifié / limite | 51232 / 52320 / 52732 | Authentification réussie, défi valide |
| Début / échéance du bail Alice | 53400 / 113400 | Bail inchangé de 60 secondes |
| Admission du ELF de préparation Alice | 64196 | PID 22 ; préparation réussie |
| Décision NSHEXEND pour Bob | 78680 | DENIED (5), bail expiré depuis 4268 ms |

Deux faits distincts sont établis :

1. L'émission du ticket vivant est refusée après expiration du **bail de session**
   de Bob. Ce n'est ni une expiration du ticket ni une révocation qualifiée :
   le ticket du témoin n'a jamais été émis. Bob PID 11 termine au statut 5.
2. Le banc perd la synchronisation du marqueur `NAR2 Bob preparation terminee emission ticket`.
   Il apparaît ligne 376 de `invite.serial`, dans la sortie de la préparation Alice,
   avant `NXR_0__FIN` ligne 456. Le banc l'attend ensuite, depuis un curseur déjà
   avancé au-delà. L'attente de 40 secondes expire. La confirmation saisie sans LF
   n'est pas exécutée. Cette dépendance à l'ordre temporel passait sur le parcours 512.

La seule commande terminée est `usermod --disable bob --prepare` : PID 22,
statuts natif et shell 0, stdout exact de 134 octets, stderr vide. L'enfant Bob
PID 21 antérieur appartient au témoin négatif préalable ; ce n'est pas un enfant
créé par l'ancien ticket demandé. Aucune tentative SPAWN du ticket vivant ni
aucun reçu durable de désactivation n'ont donc pu être vérifiés dans cet essai.

L'échange initial TLS observé dure 9692 ms (740–10432), sous sa limite de 10 secondes
avec 308 ms de marge ; le parcours 512 accepté durait 3452 ms à cette étape.
La cause du surcoût global 4096 n'est pas établie. Aucune attribution automatique
à la charge de l'hôte. Les défis eux-mêmes ont été vérifiés avant expiration.
Les délais TLS, défis, baux et tickets n'ont pas été modifiés.

La prochaine tentative doit être précédée d'une revue de cette chronologie et
de la synchronisation du banc, avec une hypothèse précise assurant un bail valide
avant émission. Aucun changement de protocole ou d'oracle n'est appliqué ici.

## Persistance et ressources

La relecture indépendante de l'autorité conserve la génération 23, zéro mutation
administrative et les époques ACTIVE d'Alice (1) et Bob (2). Le triplet P9 reste
identique à la genèse, et le TSV synthétique conserve ses douze octets exacts.
Le changement d'empreinte de l'image P9 entière lié à sa récupération transactionnelle
initiale ne démontre pas une modification des privilèges.
Cette relecture hôte complète les traces invitées et ne remplace pas la recette VFS.

Le pic observé des racines IPC reste à quatre ; le refus LIMIT de la cinquième
demande est présent. Registre : 145 pages ; signataire : 66 pages. Les restitutions
locales de Bob PID 11 et du ELF de préparation PID 22 donnent zéro racine, handle,
attente, page, pile et thread. Les autres services restent vivants à ces instants.
Le banc termine ensuite QEMU après son timeout : **aucun drainage global final
invité n'est démontré**. Ne pas transformer cette terminaison hôte en preuve
d'absence globale de fuite. Les mesures détaillées sont dans le
[diagnostic](validation/novaauth-r2-extension-ticket-20261122-diagnostic.json).

## Coordination et durée

Verrou commun acquis le 22 novembre 2026 à **11:56:16.043187 CET**, libéré à
**11:58:18.575562 CET**. Attente : **0,000386458 s** ; exécution effective du banc :
**122,485097375 s**. Aucun conflit de planification ni campagne concurrente lancée.
Le verrou a été libéré malgré l'échec. Aucune restriction de plateforme n'a
empêché l'essai.

Le budget actif de 30 minutes commence à 11:55:34.089602 CET, hors attente.
À la rédaction du présent rapport : 611.997 secondes actives, dont 489.512 secondes
hors banc (lecture, diagnostic, archivage et préparation Git, comptés de manière
conservatrice). Aucun développement fonctionnel. Le bilan final de publication
et les durées jusqu'à publication sont archivés séparément dans
`bilan-extension-ticket-a079427.json`, à côté du manifeste, sans modifier les
preuves scellées.

## Archives et empreintes

Racine durable publique :
`OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/extension-ticket-a079427/`.
Le sous-répertoire du parcours est
`r2-commandes-x86_64-4096-desactivation-essai01/`.
Les traces originales, images, résultats négatifs, sorties exactes, outils et
références sont conservés. [Matrice publiée](validation/novaauth-r2-extension-ticket-20261122-matrice.json).

- Manifeste public `SHA256SUMS-extension-ticket-a079427` : **49 fichiers**,
  2645350 octets, SHA-256 `2bc6fd0cef106ac5a098d294753701af556ac5d0a239508df3c59f748d83f95b`.
  [Copie publiée](validation/novaauth-r2-extension-ticket-20261122.sha256).
  Les chemins du manifeste sont relatifs à la racine publique ci-dessus.
- Artefacts privés : `OS-worktrees/artefacts-prives-administration-0240-r2/extension-ticket-a079427/`.
  Manifeste homonyme dans le répertoire parent : **21 fichiers**, 25396100 octets,
  SHA-256 `5224830a56d3650576143ebd37f1379873cd5ab106cae4021f3d1585adf125de`.
  Les secrets de laboratoire et leurs artefacts restent hors Git.

Deux lectures SHA-256 concordantes ont vérifié tous les fichiers de ces deux
manifestes. Les archives des campagnes antérieures restent conservées.

## Portée et suites non exécutées

Aucune validation globale R2. Aucun autre scénario de sécurité, aucune opération
administrative supplémentaire, aucun rejeu ABI ou ELF historique dans cette
étape ciblée. Aucun changement du noyau ordinaire, de l'ABI v1 à 48 sélecteurs,
du format P9 ou des quotas. Les réserves historiques de 301 cadres AArch64
instrumentés et de reproductibilité entre environnements restent ouvertes.

Publication documentaire uniquement sur `codex/novaauth-administration-0240-phase1`.
Aucune fusion, aucun tag, aucune activation ordinaire. Utilitaires en pause.
Les deux profils AArch64 attendent la résolution du premier échec et un nouvel
accord de poursuite ; aucune extension de qualification n'est revendiquée.
