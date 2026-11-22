# NovaAuth R2 — Consolidation du témoin Alice/Bob

22 novembre 2026. **Trois profils sont qualifiés sur le candidat corrigé ;
x86_64/P9-512 reste non qualifié. La consolidation des quatre profils n'est
pas acquise.** Aucun élargissement aux neuf scénarios de sécurité R2.

## Références et applicabilité des preuves

- Branche : `codex/novaauth-administration-0240-phase1`.
- Départ vérifié directement sur origin : `c7e5238d55efc89b6e38b430597a521edb806907`.
- Candidat fonctionnel effectivement testé : `c1084f9fdf3f5c36030c5b0fd85119c23f26c81d`.
- Arbre : `bdf470b5d0428178a3feccdc97a0d62819b72423`.
- Main relu au départ : `5c7f1bfa313a3859ccc0830660438e76d598acb1`.
- Le commit de livraison ajoute uniquement ce rapport et ses preuves ; aucun
  nouveau code après le candidat fonctionnel ci-dessus.

Les manifestes antérieurs sont intégralement relus : 250 preuves AArch64,
39 artefacts privés AArch64, 143 preuves de stabilisation x86_64/P9-4096 et
347 preuves du témoin historique, chacune contre son SHA-256. Cela vérifie
leur intégrité, sans étendre la portée des scénarios exécutés.

| Profil historique | Référence des preuves | Applicabilité au candidat final |
|---|---|---|
| x86_64/P9-512 | arbre invité `e7ddc8ecea2220ce205a443dcd01de4203f6dbb8`, arbre banc `a7f86df67f28882d4ae17345d46ab177ff3ac182` | Le succès ancien avec marge 996 ms ne couvre pas les modifications de synchronisation ; rejeu obligatoire effectué et échoué. Ces références sont des arbres, pas des commits. |
| x86_64/P9-4096 | invité `48fd525746e5b6dfceb3fbfe7cb8767d068e7614`, banc `20a9decfb3865132bd96cb441c2c9b92dc86721d` | Deux succès conservés ; nouveau marqueur et nouveau rendez-vous non couverts, remplacés ici par un rejeu réussi. |
| AArch64/P9-512 | invité `35156744e89ad3868fe0d06e3af0217e2ca4ced1`, contrôle de trace `c6716de119d45e4fbe65185726206254f89b821c` | 1 008 entrées de compilation identiques au départ c7e5238 ; la correction fonctionnelle c1084f9 impose ensuite un rejeu, réussi. |
| AArch64/P9-4096 | mêmes sources invitées, banc `c6716de119d45e4fbe65185726206254f89b821c` | Même justification ; rejeu c1084f9 réussi. |

La comparaison des sources générées et les empreintes exactes sont archivées.
Les tests hôtes ne sont jamais assimilés aux exécutions dans l'invité.

## Défaut corrigé dans le banc

Le premier rejeu x86_64/P9-512 sur c7e5238 obtient le ticket de Bob, mais
l'ELF de confirmation d'Alice attend encore la réponse à sa seconde préparation.
Le rendez-vous maintient cette réponse en attente pendant l'authentification
et la préparation de Bob, dans la même fenêtre client bornée de 20 secondes.
La trace n'atteint pas l'entrée Confirm. Le client produit **UNCERTAIN natif 22**,
statut ELF/shell 20 ; l'autorité ne contient aucune mutation administrative.
Le statut IPC interne antérieur à la conversion en 22 n'est pas enregistré :
il serait abusif de présenter TIMED_OUT comme directement observé.

Le correctif c1084f9 sépare deux événements privés corrélés :

1. `NXR2AU01`, émis après la première préparation autorisée d'Alice, déclenche
   l'authentification de Bob.
2. `NXR2GO01`, émis seulement lorsque l'ELF de confirmation d'Alice est résident,
   déclenche la préparation et l'émission du ticket de Bob.

Les deux événements partagent les mêmes instances, opération et empreinte de
confirmation ; le second doit correspondre exactement au premier. Le banc
attend l'authentification effective de Bob avant de lancer la confirmation.
Il ne crée ni session ni capacité à la place de NovaAuth. Toutes les validations
côté serveur, les durées de bail, le ticket de 5 s, les échéances TLS et les
attentes IPC bornées sont conservés. Aucun secret ou contenu de ticket n'est
ajouté aux événements.

32 tests hôtes réussissent en 1,360 s. Ils couvrent notamment le corps Rust
réel généré : un seul déclenchement d'authentification, deuxième étape manquante,
rejeu, substitution d'instance/opération/empreinte, réserves, taille et tag.
Les tests existants des événements, sorties, statuts, rendez-vous et transport
conservent leurs attentes. Ce succès hôte ne remplace pas la qualification QEMU.

## Matrice exécutée sur le candidat fonctionnel

| Profil | Commit exact (abrégé ici, complet ci-dessus) | Résultat | Présentation / échéance ticket, ms invitées | Marge | TLS initial |
|---|---|---|---:|---:|---:|
| x86_64/P9-512 | `c1084f9` | **Échec** | 64 236 / 62 372 | **−1 864 ms** | 5 672 ms |
| x86_64/P9-4096 | `c1084f9` | Réussi | 20516 / 23792 | 3276 ms | 2940 ms |
| aarch64/P9-512 | `c1084f9` | Réussi | 14100 / 18036 | 3936 ms | 3332 ms |
| aarch64/P9-4096 | `c1084f9` | Réussi | 10092 / 14416 | 4324 ms | 2736 ms |

Les trois réussites utilisent chacune un seul essai du candidat c1084f9, sans
répétition après succès. Sur chacun :

- Alice PID 5 et Bob PID 11 ont des instances et sessions distinctes ; les deux
  authentifications réussissent au statut natif 0.
- Bob reçoit son ticket non consommé puis obtient réellement **REVOKED (10)**
  de SPAWN, avant expiration. Aucun enfant ni admission d'enfant pour Bob.
- Alice exécute trois vrais ELF usermod, PID 21/22/23 : préparation, confirmation,
  consultation du même identifiant. Statuts natifs et shell 0 ; stdout exact
  134/70/70 octets, stderr vide. Une seule mutation durable.
- L'invité vérifie le reçu RETIREE ; la relecture indépendante de la base vérifie
  aussi sa signature Ed25519, les trois acquittements (masque 7) et l'époque retirée.
- Les trois statuts sont des lignes noyau complètes, liées au PID d'Alice ;
  aucune reconstruction d'un marqueur manquant ou intercalé n'est admise.
- Les deux sessions ferment ; chaque shell rend roots/handles/waits/pages/pile/
  threads à zéro. Après arrêt supervisé, endpoints, nodes, espaces et cadres IPC
  globaux sont à zéro. L'arrêt du témoin Bob supervisé (125) n'est pas présenté
  comme une terminaison normale à 0.

Empreintes des reçus terminal + signature :

- x86_64/P9-4096 : `c4d0e05613e5badad30e72e7374d0c0f768c357fb6d41c902104b8c132e73e98`.
- aarch64/P9-512 : `3cd75b1ed125a428e167efb323c3ffe07d1dfc70626bb3bcbc26b8b878a752cc`.
- aarch64/P9-4096 : `c1a687d7c6c39372e77607a0bfb3bc631286f8f40e4922d72353da58096a0dc5`.

## Réserve x86_64/P9-512 : chronologie démontrée

Toutes les valeurs suivantes sont de l'horloge monotone invitée. Les ticks
noyau sont à 250 Hz, convertis par multiplication par quatre. Aucune
soustraction entre horloge hôte et invitée.

| Événement | Temps invité (ms) |
|---|---:|
| Authentification Alice : vérification / fin | 9 912 / 11 856 |
| Échéance du bail Alice | 70 884 |
| Premier ELF Alice créé | 22 960 |
| Authentification Bob : vérification / fin | 29 424 / 31 252 |
| Échéance du bail Bob | 90 332 |
| ELF de confirmation Alice créé | 45 064 |
| Image de Bob scellée | 52 928 |
| Échéance du ticket Bob | 62 372 |
| Entrée effective Confirm | 63 080 |
| Présentation SPAWN par Bob, retour 10 | 64 236 |
| Début publication du triplet | 65 248 |
| Retour de publication | 76 156 |
| Nouvelle vérification NSHLIVE1 d'Alice : refus natif 5 | 77 052 |

**Le ticket a déjà expiré à l'entrée Confirm (708 ms de dépassement).**
Son refus REVOKED 1 864 ms après échéance ne constitue donc pas le témoin exigé.
L'absence d'enfant et la révocation observées sont conservées comme résultats
partiels, jamais comme réussite avant expiration.

Un second obstacle existe : la publication revient 5 272 ms après échéance du
bail d'Alice. Sa commande ne rapporte pas le résultat attendu ; la trace indique
une sortie processus 3, des flux stdout/stderr vides et un statut shell 126.
Ces trois nombres ne sont pas assimilés au statut natif de NovaAuth : le refus
NSHLIVE1 observé est 5. La cause exacte de la sortie processus 3 n'est pas
identifiée par ces seules traces.

La mutation a néanmoins eu lieu une seule fois : le reçu RETIREE est signé,
les trois effecteurs sont acquittés et l'époque est retirée. **Aucune mutation
n'a été répétée.** La consultation finale par identifiant et le drainage global
ne sont pas atteints dans ce parcours. L'arrêt de QEMU ne prouve pas leur succès.
Le signataire et le registre retrouvent leurs compteurs locaux de référence,
mais cela ne suffit pas à revendiquer le drainage global des deux sessions.

Le temps restant entre la réception du ticket, la validation de présence
NSHLIVE et l'entrée Confirm n'est pas entièrement horodaté. Une prochaine
correction devra localiser ce coût, ainsi que celui de la publication sur
P9/512, et le réduire sans supprimer de contrôle ni prolonger un délai.
Une instrumentation minimale aux limites de ces étapes serait discriminante.
Les observations ne justifient ni de mettre l'échec sur le compte de la charge
hôte, ni d'augmenter les baux. Aucun nouvel essai 512 identique n'est lancé.

## Artefacts, ressources et compatibilité

QEMU 11.1.3, TCG, deux vCPU, 256 Mio, réseau désactivé. Chaque essai emploie une
copie neuve de sa genèse P9 et de l'ancien disque d'autorité, génération 23,
et une autorité indépendante propre au parcours. Les commandes intégrales,
SHA initiaux/finals et états durables sont conservés. Les genèses ont été
vérifiées contre le manifeste antérieur avant copie.

Les ISO sont vérifiées avant démarrage : profil unique `novaauth-sessions`,
SHA du noyau embarqué identique à celui annoncé. Rust/Cargo 1.99.1,
LLVM 23.1.1, release/locked/offline, feature `novaauth-sessions-lab` ; cibles
`x86_64-unknown-none` et `aarch64-unknown-none-softfloat`. Les sources générées
exactes et binaires sont archivés dans les répertoires privés de qualification.

| Artefact final | SHA-256 |
|---|---|
| x86_64.iso | `57a427de800668557c639fb7aefa259d542fea07441d972a59ed152c12024688` |
| x86_64-kernel.elf | `7143e97731e56a15e2e2b3b6ca3a221085aafaf7842c364b8a3a9c576d4b753a` |
| x86_64-registre.elf | `5b816927d57ba230ad32b11201447b586c0bb15541187ccdb7fd5e14ea4e3135` |
| aarch64.iso | `3f7c23b0ba47b690771cdea42885557ca3fabc7c7b7ed8897df766d5e040c0bd` |
| aarch64-kernel.elf | `a9017df70c12ecbaa1e8ab8d4c05ae6781ffc3de63ac055a09212e9b78432f13` |
| aarch64-registre.elf | `261ee8f87b73390b98882e4a8ac9c248d284096fc9b0dd51254012d77e90c970` |

Pic : quatre racines maximum pour chaque acteur ; la cinquième est refusée.
Les tableaux détaillés par PID sont dans la matrice JSON. Compteurs résidents
[roots, handles, waits, pages, pile, cadres IPC globaux] :

| Architecture | Signataire avant/pendant/après | Registre avant/après |
|---|---|---|
| x86_64 | [4, 6, 0, 68, 64, 31] | [4, 7, 0, 145, 64, 31] |
| AArch64 | [4, 6, 0, 59, 64, 31] | [4, 7, 0, 139, 64, 31] |

Les résidents persistants sont distincts des ressources temporaires rendues
au drainage. Aucun nouveau budget n'est introduit. Réserves historiques
conservées : **301 cadres instrumentés AArch64** et **reproductibilité des
compilations entre environnements**. Compatibilité fonctionnelle, identité
SHA-256 et reproductibilité restent des notions distinctes.

Le diff depuis c7e5238 ne change ni `novaOS/kernel`, ni le registre ABI, ni
NovaFS, ni les composants ordinaires. Le registre conserve 48 sélecteurs.
Les générateurs modifiés ne s'exécutent que dans l'export explicite
`--administration-ticket-vivant` ; aucun raccord n'est activé dans la distribution
ordinaire. Les neuf contrôles ABI et les quatre ELF historiques ne sont pas
rejoués dans cette campagne ciblée ; leurs sources et preuves antérieures sont
préservées. Aucune nouvelle qualification de production n'est revendiquée.

## Verrou et temps

Heures Europe/Paris, UTC+01, le 22 novembre 2026. Code 0 = succès du banc ; code 1 = échec.

| Profil (code) | Acquisition | Libération | Attente, s | Exécution banc, s |
|---|---|---|---:|---:|
| x86_64/512 (1) | 17:24:40.987254 | 17:25:46.144657 | 0.000378 | 65.112 |
| x86_64/512 (1) | 17:32:26.489483 | 17:33:46.314261 | 0.000412 | 79.777 |
| x86_64/4096 (0) | 17:40:08.208087 | 17:40:43.794353 | 0.000361 | 35.541 |
| aarch64/512 (0) | 17:41:45.106721 | 17:42:22.778179 | 0.000354 | 37.626 |
| aarch64/4096 (0) | 17:42:50.570582 | 17:43:18.891232 | 0.000321 | 28.277 |

Aucune campagne incompatible détectée. Le verrou a été pris et libéré cinq fois,
sans forçage, après nettoyage des descendants hôtes. Attente totale :
**0.001825 s**, exclue du budget. Exécution : **246.334 s**.
Préparation, développement, diagnostic et rapport hors banc au relevé :
**1349.671 s** ; actif total **1596.005 s**, sous 2 700 s.
Ce relevé précède le scellement des preuves et la publication. Les durées hôtes
n'ont jamais servi à calculer les marges invitées.

## Archives et suite autorisée

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/consolidation-c7e5238`. Les preuves positives et négatives sont conservées.
Les secrets et artefacts qui peuvent incorporer des clés de laboratoire restent
hors Git. Les manifestes privés sont vérifiés séparément ; seul leur digest est
reporté. Le manifeste de preuves public et la matrice sont versionnés :

- [Matrice consolidée](validation/novaauth-r2-consolidation-20261122.json).
- [Manifeste SHA-256](validation/novaauth-r2-consolidation-20261122.sha256).

**Profil restant à qualifier : x86_64/P9-512**, pour la présentation avant
expiration, la fin de commande d'Alice, la consultation du résultat et le
 drainage final. Les trois autres profils disposent désormais de preuves sur
le même candidat fonctionnel c1084f9. Ne pas remplacer cette réserve par
l'ancien succès 512. Aucun des neuf scénarios supplémentaires, aucune fusion,
aucun tag ni activation ordinaire. Les utilitaires restent en pause.
