# NovaAuth v0.24.0 — Témoin R2 consolidé sur quatre profils

22 novembre 2026. **Le témoin Alice/Bob est qualifié sur les quatre profils sur
le candidat fonctionnel `c80bdcb580b77d3ce6ae47f0c05645ccd4212859`**, avec deux nouvelles réussites indépendantes
x86_64/P9-512 et réutilisation justifiée des trois preuves c1084f9. Cette
conclusion ne qualifie ni les neuf scénarios supplémentaires, ni R2 dans son
ensemble, ni la distribution ordinaire.

## Références et périmètre

- Branche unique : `codex/novaauth-administration-0240-phase1`.
- Départ publié vérifié : `dbb20cc4d92c002be3ed06c52e9e8483bd483cc4`.
- Référence fonctionnelle : `c1084f9fdf3f5c36030c5b0fd85119c23f26c81d`.
- Sondes de diagnostic : `f5ba0978848159d5bf95f6008cbf8e98c83c936e`.
- Correctif final : `c80bdcb580b77d3ce6ae47f0c05645ccd4212859` ; arbre `8d4bdbd7b54f0d5187d3b89b406afc7533f3262c`.
- `main` lu sur origin : `5c7f1bfa313a3859ccc0830660438e76d598acb1` ; aucune fusion.

Le commit de livraison ajoute ce rapport, la matrice et le manifeste. Les
archives précédentes restent intactes, notamment le
[rapport de consolidation antérieur](novaauth-administration-r2-consolidation-v0.24.0.md).

## Cause démontrée et chronologie

Le dépassement historique de **1 864 ms** n'était pas une preuve de révocation
avant expiration. Un seul essai diagnostique supplémentaire, avec les sondes
f5ba097, reproduit un dépassement de **1 812 ms**. Il localise le coût dans les
consultations fraîches successives de l'autorité, transportées par fragments
IPC pour les secteurs 512. Le rendez-vous ne perd plus d'événement.

Temps monotones invités en millisecondes ; ticks du noyau convertis à 250 Hz :

| Étape, essai diagnostique | Instant / durée |
|---|---:|
| Appel de préparation du ticket Bob | 49 972 |
| Retour de la référence du ticket Bob | 58 960 |
| Contrôle supplémentaire NSHLIVE de Bob | 58 960 → 60 080, soit 1 120 |
| Revalidation d'Alice au rendez-vous | 60 080 → 61 208, soit 1 128 |
| Revalidation d'Alice à Confirm | 61 248 → 62 368, soit 1 120 |
| Contexte et liaison administratifs vérifiés, sonde 20 | 63 512 |
| Présentation Bob / échéance du ticket | 64 640 / 62 828 |
| Début de l'échange de mutation / retour, sondes 21–22 | 65 664 → 76 676 |
| Échéance du bail Alice | 70 984 |
| Contrôle de session lors de la restitution, refus natif 5 | 77 596 |

La durée de ticket reste **5 000 ms**. Son âge à la remise est estimé à 1 132 ms
à partir de son échéance et de cette durée ; ce n'est pas une nouvelle sonde
au point de création. La préparation complète prend 8 988 ms, dont une partie
précède cette création. Il ne faut donc pas soustraire toute cette préparation
de la durée du ticket.

**Admission et restitution sont distinctes.** Dans le diagnostic, la confirmation
a passé ses contrôles avant l'échéance d'Alice ; la publication et les
acquittements se poursuivent ensuite. Le journal indépendant contient une seule
mutation admin-change, ses effets et subject-retired, alors que la restitution
au shell échoue après expiration de la session. La trace ne localise pas à elle
seule la conversion finale vers le statut 3 de l'ELF ; le refus de session 5
est observé séparément, après retour de la mutation. Ce résultat n'est ni annulé ni
répété automatiquement. Le statut shell 126 et l'absence de drainage complet
restent archivés comme échec ; le reçu durable seul ne suffit pas à qualifier
le parcours. L'instant exact de linéarisation distant n'est pas mesuré par les
sondes invitées : le journal prouve son ordre, sans mélanger les horloges.

## Correction ciblée du transport du banc

Le profil `--administration-ticket-vivant` utilise désormais, **seulement sur
x86_64 et avec secteurs 512**, la voie PAGE déjà négociée et déléguée pour les
deux pages volatiles hors P9. Il conserve tous les contrôles de fraîcheur,
session, capacité et admission. Aucune préparation exigeant l'authentification
n'a été déplacée avant celle-ci. Aucun contrôle n'est supprimé ni mis en cache.

- Même cellule canonique de 512 octets, dont 448 octets utiles maximum.
- Transit dans une page existante de 4 Kio, complément nul vérifié ; aucun
  changement du format P9, des adresses, des identités ou de la cryptographie.
- Voie activée uniquement si le handle existe et FEATURE_PAGE est négocié.
  Sinon le chemin sectoriel historique reste utilisé, sans huit accès de repli
  aux pages de transport.
- Un tampon de pile de 4 Kio, sans cache persistant ni allocation de tas.
- Adaptateur hôte compilé explicitement avec `--cfg r2_page_512`. Il accepte
  la page complète aux deux seules adresses existantes, conserve le plafond
  de charge utile et rejette les autres tailles/adresses. Sans cette option,
  son comportement historique reste testé.
- Les sondes supplémentaires 30–36 sont retirées du candidat final ; les
  sondes historiques 20–22, les oracles et le rendez-vous c1084f9 sont inchangés.

Il s'agit d'une réduction du nombre d'échanges IPC de fragmentation, pas d'une
augmentation de durée de bail. Les échéances de 60 s, 5 s et 10 s TLS sont
inchangées. Les traces NBD démontrent des transferts de 4 Kio aux offsets
2 097 152 et 2 101 248 ; le disque P9 conserve ses secteurs 512.

## Matrice consolidée

Les instants et marges sont uniquement ceux de l'invité. Les trois lignes
c1084f9 sont des **preuves antérieures applicables**, pas de nouveaux essais
sur c80bdcb.

| Profil (essai) | Commit réellement testé | Résultat | Présentation / échéance, ms | Marge, ms | TLS initial, ms |
|---|---|---|---:|---:|---:|
| x86_64/P9-512 (page01) | `c80bdcb580b77d3ce6ae47f0c05645ccd4212859` | Réussi | 20400 / 23640 | 3240 | 2888 |
| x86_64/P9-512 (page02) | `c80bdcb580b77d3ce6ae47f0c05645ccd4212859` | Réussi | 20160 / 23448 | 3288 | 2852 |
| x86_64/P9-4096 (01) | `c1084f9fdf3f5c36030c5b0fd85119c23f26c81d` | Réussi | 20516 / 23792 | 3276 | 2940 |
| aarch64/P9-512 (01) | `c1084f9fdf3f5c36030c5b0fd85119c23f26c81d` | Réussi | 14100 / 18036 | 3936 | 3332 |
| aarch64/P9-4096 (01) | `c1084f9fdf3f5c36030c5b0fd85119c23f26c81d` | Réussi | 10092 / 14416 | 4324 | 2736 |

Pour chacun des deux nouveaux essais :

- Alice PID 5 et Bob PID 11, identités et sessions distinctes.
- Ticket de Bob vivant et non consommé, refus natif **10 (REVOKED)** avant
  échéance ; aucun enfant ni admission distante de Bob.
- Trois exécutions de l'ELF administratif par Alice, PID 21, 22 et 23 : statuts natifs et shell 0,
  stdout 134, 70 et 70 octets conformes ; stderr vide.
- Une seule mutation, effets acquittés 7, reçu terminal de 128 octets et
  signature Ed25519 de 64 octets vérifiée. Époque durable **RETIREE**.
- Journal : admissions ELF d'Alice aux séquences 6, 8, 13 ; créations 7, 9, 14 ;
  mutation 10, effets 11 et retrait 12.
- Le refus local de Bob précède la confirmation distante : il n'est jamais
  présenté comme l'acquittement définitif à lui seul.
- Confirmation : sondes 20/21/22 à 20 100 / 20 668 / 26 948 ms pour l'essai 1,
  et 19 868 / 20 420 / 26 632 ms pour l'essai 2. Les baux d'Alice sont encore
  valides à la restitution ; la consultation explicite `--outcome` réussit.

Les deux essais utilisent les mêmes binaires et genèses vérifiées, mais des
copies neuves des disques, une autorité indépendante et des remises propres
à chaque parcours. Aucun état de la première exécution n'est réutilisé.
Deux succès ne constituent pas une garantie de marge sous toute charge.

## Applicabilité des trois autres profils

La comparaison des sources générées porte sur 711 entrées invitées Rust,
manifestes et fichiers de compilation : 709 sont identiques. Les deux seules
différences sont les ajouts à novafs/device.rs et storage/medium.rs dans
l'export de qualification. Après exclusion exacte de ces deux ajouts, les deux
fichiers sont identiques à c1084f9.

- **AArch64** : les deux ajouts sont exclus par cfg(target_arch="x86_64").
- **x86_64/P9-4096** : le nouveau corps n'est pas exécuté (size==512 est faux) ;
  le chemin PAGE/4096 existant reste identique.
- NovaAuth, son registre, les sessions, les contrôles, les oracles du banc et
  les composants ordinaires n'ont aucun changement fonctionnel entre les deux
  candidats. Les anciens binaires et l'ancien adaptateur des trois preuves
  restent conservés.

Ces éléments justifient la réutilisation sans trois campagnes supplémentaires.
Ils établissent une applicabilité fonctionnelle, **pas une identité SHA-256
avec des binaires recompilés dans un autre chemin ou environnement**. Les
empreintes originales de chaque profil restent dans la matrice JSON. La réserve
historique de reproductibilité demeure ; aucune nouvelle compilation ordinaire
ni nouvelle qualification de production n'est revendiquée.

## Ressources et compatibilité

Pic mesuré : **quatre racines maximum par acteur**, cinquième demande refusée.
Après fermeture : Alice et Bob à zéro racine, handle, attente, page, pile et
thread ; drainage global à zéro endpoint, nœud, espace et cadre IPC. Le collecteur
reste exclusivement au banc. Bob est arrêté par la supervision après fermeture
de sa route ; ce statut 125 est distingué d'une terminaison normale.

Signataire avant/pendant/après : [4,6,0,68,64,31] ; registre avant/après :
[4,7,0,145,64,31] (racines, handles, attentes, pages, pile, cadres IPC globaux).
NovaFS présente 186 puis jusqu'à 218 pages résidentes, pile 64 : même rétention
de 32 pages que le parcours x86_64/P9-4096 déjà qualifié ; le service est ensuite
drainé. Ce compteur n'est pas assimilé à une allocation de cache supplémentaire
par le correctif. Le tampon ajouté est sur la pile déjà réservée.

L'adaptateur conserve deux pages (8 192 octets) et 56 octets de métadonnées
sérialisées ; supplément natif 56, ou 76 avec réouverture, sous la borne 256.
Aucun handle, service, quota ou sélecteur n'est ajouté. Le diff des sources
ordinaires kernel, abi, novaFS, components, user/novafs et user/storage est vide.
Le registre conserve **48 sélecteurs**.

Les neuf contrôles ABI et les quatre ELF historiques ne sont pas rejoués dans
ce mandat ciblé ; leurs sources et preuves restent préservées. Réserves conservées :
**301 cadres instrumentés AArch64** et **reproductibilité entre environnements**.

## Tests hôtes et artefacts

32 tests de diagnostic réussis, puis **33 tests du banc final**, **7 tests de
l'adaptateur historique** et **7 avec PAGE/512**. Assertions historiques conservées.
Les ajouts couvrent octets exacts, padding, feature/handle absents, mauvais
contexte, tailles et adresses interdites, propagation d'erreur et absence de
modification du volume témoin. Les tests historiques de rejeu, saturation et
réouverture restent réussis. Ces tests hôtes ne remplacent pas les essais invités.

QEMU 11.1.3, TCG, deux vCPU, 256 Mio, réseau désactivé. Les commandes complètes,
versions, options, SHA initiaux/finals, sources du lanceur de campagne et
horodatages sont archivés. ISO vérifiée novaauth-sessions, noyau embarqué exact.
Rust/Cargo 1.99.1, compilation release/locked/offline ; avertissement historique
llvm-objcopy conservé dans le journal, compilation terminée avec code 0.

| Artefact x86_64/P9-512 final | SHA-256 |
|---|---|
| x86_64.iso | `54f5408d1eb08cc3f52c5a811e9bd8c36b52dfc71b67ceb724c213653beb9190` |
| x86_64-kernel.elf | `a5098ce7ce66809a5eded2b502ea434262bd7647177d5162c9abc0c4eb488b35` |
| x86_64-registre.elf | `2ddf743288c3bc656826b0c118f6db04a452675b38a953107bf9f9da73b95206` |
| identites.p9 | `1619a2e0fdee69f63e999a10dff4c31d3f24f2dfef96a8ef00a6cd3d268c8728` |
| autorite.raw | `105acaa8a0f3cbc84af09633c21a227b1622fa5a742e4d2cbccbfc87551c523a` |

Reproduction : exporter le commit final par service/profile.py avec
`--administration-ticket-vivant --source-ref c80bdcb580b77d3ce6ae47f0c05645ccd4212859`. Compiler le transport
hôte transport_qemu/adaptateur.rs avec `rustc --edition=2024 -O --cfg r2_page_512`
et fournir ce binaire par NOVA_ADAPTATEUR. Les scripts archivés preparer.py,
compiler-c80bdcb.py et executer-c80bdcb.py précisent les commandes réelles,
la genèse, les chemins privés et le verrou ; aucun secret n'est exporté dans Git.

## Verrou et budget

22 novembre 2026, Europe/Paris (UTC+01). Code 1 : diagnostic négatif conservé ;
code 0 : succès. Aucun essai concurrent incompatible détecté.

| Code | Acquisition | Libération | Attente, s | Exécution banc, s |
|---|---|---|---:|---:|
| 1 | 18:02:28.783353 | 18:03:49.270314 | 0.000362 | 80.440 |
| 0 | 18:12:09.810716 | 18:12:45.635568 | 0.000340 | 35.779 |
| 0 | 18:13:09.591004 | 18:13:44.798102 | 0.000420 | 35.159 |

Attente totale **0.001121 s**, exclue du budget. Exécution effective du banc
**151.379 s**. Développement, diagnostic, préparation et vérification hors banc
au relevé : **1486.331 s** ; actif total **1637.710 s**, sous 3 600 s.
Ce relevé précède le scellement final et la publication. Les horloges hôtes ne
sont jamais soustraites des horodatages invités. Verrou libéré après les trois
exécutions, sans suppression ni forçage du verrou d'un autre chantier.

## Preuves et réserves de portée

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/finalisation-512-dbb20cc`.
**165 fichiers de preuve**, positifs et négatifs, vérifiés par
SHA256SUMS : `18852fdabe5ea86fcff03bd8b9e29614ab046b457f2052b642e2819b4ede91d9`.

- [Matrice détaillée](validation/novaauth-r2-quatre-profils-20261122.json).
- [Manifeste des nouvelles preuves](validation/novaauth-r2-quatre-profils-20261122.sha256).
- 261 preuves antérieures revérifiées, manifeste
  `5069aa58e8149d9df951f1105452545062eda95f1f5f58f36fceadf11a1f5936`.
- Deux manifestes privés antérieurs (28 et 79 fichiers) vérifiés.
- Nouveaux artefacts privés : manifestes séparés dont les SHA figurent dans
  la matrice. Aucune clé, ISO contenant des clés ou sauvegarde privée ajoutée
  à Git. Recherche brute/hex/base64 de 23 secrets connus : aucune occurrence
  dans les archives ; ce contrôle a une portée bornée.

La réserve temporelle de ce témoin x86_64/P9-512 est levée dans les conditions
QEMU mesurées. Les neuf scénarios de sécurité et les autres opérations R2 restent
hors de cette campagne. Aucune fusion dans main, aucun tag, aucune activation
ordinaire. Les utilitaires restent en pause.
