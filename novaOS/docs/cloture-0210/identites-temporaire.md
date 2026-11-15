# Identités d’instance et temporaire privé

## Périmètre des preuves

Les archives I et B couvrent respectivement le cycle de vie et les sources du BootID. Le candidat d’intégration ABI/tmp `3839f1a2525e7ad977b16b4a6b4aeacf4880bc63` contient le correctif du retrait de NovaFS `ccb50dde8c90ae5d1f8233cefe78b1530bf58305`. Les archives I, T et N identifient les noyaux de cette qualification. Le candidat cache `4a3d3079bad8a8ef074601df5afdf9c6d5faacfb` ajoute notamment le suivi des baux des services supervisés et la remontée des erreurs de purge au gestionnaire. Son archive C couvre les interactions du temporaire avec le cache et la session VFS. Les expériences de coupure de la phase 2 gardent la portée de leur révision d’origine. [Intégration ABI/tmp](../integration-abi-tmp-v0.21.0.md) · [Qualification du cache](../cache-prive-phase4-v0.21.0.md)

**Q** désigne QEMU 11.1.3 TCG sur AArch64 et x86_64. **Q-SMP** couvre 1, 2 et 4 vCPU sur chacune. **H** désigne le moteur testé sous Darwin ARM64 natif et x86_64 sous Rosetta. Pour le stockage, les colonnes indiquent les secteurs logiques en octets. Chaque lettre de la colonne « Archive » renvoie au fichier et à son SHA-256 dans la dernière table.

## Phase 1 — Identités et cycle de vie

| ID | Exigence | Contrat | Code | Test | Architecture | Résultat | Archive | Observation et limite |
|---|---|---|---|---|---|---|---|---|
| I01 | Identités d’application et d’instance | [Instances][d1] | [Manifeste][install], [registre][instances] | [Sonde invitée][it] | Q-SMP | Validé avec limites | I | Un identifiant d’installation embarqué est commun aux lancements du même programme. Chaque admission reçoit son incarnation. L’enrôlement d’une installation dynamique reste hors du contrat. |
| I02 | BootID opaque de 128 bits | [BootID][db] | [Source du noyau][boot], [trame][wire] | [Redémarrages][rt] | Q | Validé avec limites | B, I | Relu à l’identique dans le démarrage courant. La fraîcheur entre démarrages dépend d’une source de confiance. Un hôte qui rejoue la même source ou un instantané avec RAM peuvent reproduire l’époque. |
| I03 | Générations et refus d’une incarnation périmée | [BootID][db] | [Registre][instances], [trame][wire] | [Familles][it], [redémarrages][rt] | Q-SMP, Q | Validé | I, B | Le recyclage d’un slot change l’incarnation. À numéro local et génération égaux après redémarrage, le droit valide accepte uniquement le triplet du démarrage courant. |
| I04 | Processus enfants | [Instances][d1] | [Création][spawn], [principal et travailleurs][sample] | [Collecte][it] | Q-SMP | Validé avec limites | I | Premier enfant collecté avec 41 pendant la vie du principal. Un enfant actif est retiré à la fin du principal. Le contrat qualifie un enfant suivi à la fois. |
| I05 | Threads de l’instance | [Instances][d1] | [Sonde noyau][iprobe], [sonde utilisateur][sample] | [Familles et restitution][it] | Q-SMP | Validé | I | Un thread partage l’identité de son processus. La sonde observe le calculateur sur le CPU cible et vérifie le retour des TCB à leur niveau de départ. |
| I06 | Capacités initiales | [Instances][d1] | [Admission][manager], [politique][install], [délégation][spawn] | [Principal sans droits de stockage ni d’administration][sample], [lancement][it] | Q-SMP | Validé | I | Le gestionnaire applique le manifeste. Le noyau valide les droits du lanceur. Le témoin refuse le bootstrap IPC et la supervision non autorisés. |
| I07 | Révocation de la famille | [Instances][d1] | [Supervision][sup], [témoin][sample] | [Arrêt sur le dernier CPU][it] | Q-SMP | Validé | I | Après arrêt, deux droits IPC dérivés échouent, la racine extérieure fonctionne et l’ancien droit de suivi ne permet plus l’arrêt. |
| I08 | Arrêts normaux, forcés et faute | [Instances][d1] | [Supervision][sup], [registre][instances] | [Quatre modes de fin de famille][it] | Q-SMP | Validé | I | 24 fins de famille sur six profils. Sont exercés la sortie du principal, l’arrêt autorisé en cours de calcul, la faute utilisateur et la perte injectée du gestionnaire. |
| I09 | Cortex-A72 ordinaire sans RNDR | [BootID][db] | [Relais UEFI][relay], [lecture de l’identité][boot] | [Ancienne et nouvelle identités, source absente][rt] | Q AArch64 Cortex-A72 | Validé avec limites | B, I | Après arrêt normal et reset QMP, puis après SIGKILL et relance, ancienne identité refusée, nouvelle acceptée avec le même droit, enfant collecté avec 41. Sans graine qualifiée, refus explicite. Le Cortex-A72 physique reste à qualifier. |

La source observée sur Cortex-A72 est `UEFI-QEMU-RNG-SEED`. Le relais mesure la graine de 32 octets fournie dans le DTB par QEMU et dérive l’identité avec SHA-256. Le banc a comparé le résultat à une lecture de la mémoire physique avant EFI. Les profils AArch64 `max` et x86_64 `max` utilisent respectivement RNDR et RDRAND. L’intégrité de l’hôte, du firmware et du relais entre dans l’hypothèse de confiance. [Rapport et modèle de défaillance][db]

Sur le noyau qualifié avec le correctif de phase 2, les compteurs de cadres libres après restitution rejoignent ceux du début de la sonde. Pour 1, 2 et 4 vCPU, AArch64 retrouve respectivement 62 151, 62 117 et 62 068 cadres et x86_64 retrouve 62 704, 62 647 et 62 565 cadres. Les TCB reviennent à 2, 3 et 5 sur les deux architectures. Ces compteurs mesurent les ressources restituées après collecte. Le pic de mémoire de l’application reste hors de cette mesure. [Archive I][ai]

## Phase 2 — Temporaire privé et retrait de NovaFS

| ID | Exigence | Contrat | Code | Test | Architecture | Résultat | Archive | Observation et limite |
|---|---|---|---|---|---|---|---|---|
| T01 | Répertoire initial vide par instance | [Admission][dt] | [Courtier][broker], [création][tmp] | [Lancement et réadmission][tt] | Q-SMP, 512/4096 | Validé | T, C | Vue créée avant l’entrée du principal P9. Après réutilisation effective du numéro local et de la génération dans un autre boot, la nouvelle vue est vide. |
| T02 | Isolation entre instances | [Confinement][dt] | [Courtier][broker], [vue privée][tmp] | [Deux principaux du même manifeste][tt] | Q-SMP, 512/4096 | Validé | T | Même nom relatif dans les deux vues, contenus séparés. Les portes du serveur et les accès sans capacité restent refusés. |
| T03 | Partage entre threads | [Portée du droit][dt] | [Courtier][broker], [vue][tmp] | [Partage de vue][tt] | Q-SMP, 512/4096 | Validé | T | Le thread de la même instance retrouve l’objet créé dans la vue du processus. |
| T04 | Délégation explicite aux enfants | [Délégation][dt] | [Portes tmp][twire], [courtier][broker] | [Travailleur ordinaire et travailleur délégué][tt] | Q-SMP, 512/4096 | Validé | T | Le travailleur ordinaire est privé de la capacité tmp. La porte dédiée autorise la vue de la famille après vérification du droit et de l’identité. |
| T05 | Nettoyage à la destruction | [Retrait][dt] | [Retrait du bail][broker], [purge][store] | [Sortie, arrêt forcé, faute][tt] | Q-SMP, 512/4096 | Validé avec limites | T | Une fois le retrait terminé, seul le sceau du registre subsiste. Le temps dépend de la taille de l’arbre. Snapshots et copies brutes peuvent conserver des octets détachés. |
| T06 | Récupération des orphelins | [Reprise][dt] | [Vérification et collecte][store] | [SIGKILL et remontage][tt], [injections][ht] | Q-SMP et H, 512/4096 | Validé avec limites | T, F | 12 arrêts du processus QEMU avec une écriture synchronisée et 12 reprises. Le lecteur indépendant retrouve les marqueurs puis leur retrait. Le modèle de coupure hôte conserve l’alimentation. |
| T07 | Quarantaine en cas d’appartenance incertaine | [Registre et marqueurs][dt] | [Validation des OID et sceaux][store] | [Substitution, marqueurs incomplets et limites][ht] | H, 512/4096 | Validé avec limites | F | Le service conserve les entrées ambiguës et protège la sentinelle extérieure. Deux préparations invérifiables peuvent bloquer de nouvelles admissions. |
| T08 | NovaFS P9, compatibilité P8 et intégrité publique | [Profils][dt], [qualification][dq] | [Implémentation P9][store], [vue publique][ns] | [P8/P9][tr], [CLI][cli], [oracle][oracle] | Q-SMP (tmp P9) et Q (P8), 512/4096 | Validé | T, C | 48 démarrages du tmp P9. Les 16 démarrages historiques P8 incluent les sessions de lecture à génération constante. Format incompatible refusé avant écriture. L’oracle compare chemins, types, OID et octets publics. |
| T09 | Échec certain, incertitude et résultat déjà acquitté | [États NEXT/COMPLETE][dn] | [Courtier et retrait][broker] | [Arrêts et acquittements][nt] | Q-SMP | Validé | N | Avant remise, `CLOSED`. Après remise sans preuve du résultat, `UNCERTAIN`. Après acquittement, résultat conservé. Dans les 30 cas, la deuxième requête en attente reçoit `CLOSED` et toute nouvelle demande est refusée. |
| T10 | Ticket périmé et absence de double écriture | [Retrait et redémarrage][dn] | [Association des tickets][broker], [service du tmp][tmp] | [Réinstallation et comptage][nt], [images avant et après reprise][dn] | Q-SMP pour le courtier, Q à 1 vCPU pour le disque, 512/4096 | Validé avec limites | N, T | Ancien acquittement rejeté même pendant une nouvelle opération. Quatre disques avant effet sont en G51 et vides. Quatre après effet sont en G52 avec le seul octet 0x42 et une publication F1/F2. Les huit remises en route ont réussi. La réinstallation du témoin est pilotée. |

La matrice hôte F comprend 312 injections F1/F2 et huit interruptions de préparation. Le service borne les ressources à 32 vues actives ou en retrait, 32 handles par vue et 64 Kio de transfert préparé par vue. Ces valeurs expriment des plafonds. La lecture seule de l’oracle exclut les horodatages d’accès de la comparaison du catalogue public. Les essais n’établissent aucune garantie contre une coupure électrique physique ou la restauration intégrale de la VM. [Contraintes du temporaire][dt] · [Résultats du stockage][dn]

## Archives relues

La lecture de chaque archive a permis de recalculer son SHA-256 et celui de chaque fichier répertorié dans son index. L’index B est [conservé hors de l’archive](../validation/bootid-a72-v0.21.0-redemarrages.json). Les cinq autres contiennent leur index. Le nombre indiqué exclut l’index.

| Archive | Fichiers vérifiés | SHA-256 |
|---|---:|---|
| [I — Instances et restitution][ai] | 19 | `292ae348acd0a615dc44f532abfc2ee7e09e3db3ae21ef169efe562e29921014` |
| [B — BootID Cortex-A72, AArch64 max et x86_64 max][ab] | 14 | `6afe34d14cf59123e70c5eec011c5e45abfc72d302f8f0d18cf1bcd203327b5b` |
| [T — Stockage, P8 et retrait réel du service][at] | 392 | `9f48df6ddac87dc09401c3bbc2ade5edea65fd5247fc543cb59ad6d8c99a0b0e` |
| [F — Pannes du stockage, récupération et quarantaine][af] | 151 | `58c2caf755f43e720fe4e7b054299152b626f1a6db16aa291a0e1fbb60e20135` |
| [N — Attentes et retrait du courtier][an] | 32 | `db9bad7ea89b91aab6f46be312d284787c46aa06c96535d582a6bc260525c063` |
| [C — Cache et régressions du temporaire sur le candidat][ac] | 510 | `6034653cb7c2182cb3a6010db909e1ed9ed1f0086b04510f5ccc12f0051b3bf2` |

[d1]: ../instances-phase1-v0.21.0.md
[db]: ../bootid-phase1-v0.21.0.md
[dt]: ../contrat-tmp-prive-v0.21.0.md
[dq]: ../tmp-prive-phase2-v0.21.0.md
[dn]: ../retrait-service-tmp-v0.21.0.md
[install]: ../../abi/installation.rs
[instances]: ../../kernel/src/sched/process/instances.rs
[spawn]: ../../kernel/src/sched/process/ipc/spawn.rs
[wire]: ../../abi/instances.rs
[manager]: ../../user/instance_manager.rs
[sample]: ../../user/instance_sample.rs
[iprobe]: ../../kernel/src/sched/process/ipc/instance_probe.rs
[sup]: ../../kernel/src/sched/process/supervision.rs
[boot]: ../../kernel/src/boot_id.rs
[relay]: ../../boot/relais-bootid/main.rs
[it]: ../../tests/instances.py
[rt]: ../../tests/instances_reboot.py
[broker]: ../../kernel/src/sched/process/tmp.rs
[twire]: ../../abi/tmp.rs
[tmp]: ../../user/novafs/files/temporary.rs
[store]: ../../user/novafs/files/tmp_store.rs
[ns]: ../../user/novafs/files/namespace.rs
[tt]: ../../tests/tmp_prive.py
[ht]: ../../tests/tmp_prive_stockage.rs
[tr]: ../../tests/tmp_prive_refus.py
[cli]: ../../tests/tmp_prive_cli.py
[oracle]: ../../tests/tmp_prive_oracle.rs
[nt]: ../../tests/tmp_attentes.py
[ai]: ../validation/tmp-fin-service-v0.21.0-instances.tar.gz
[ab]: ../validation/bootid-a72-v0.21.0-journaux.tar.gz
[at]: ../validation/tmp-fin-service-v0.21.0-p9.tar.gz
[af]: ../validation/integration-abi-tmp-v0.21.0-p9.tar.gz
[an]: ../validation/tmp-fin-service-v0.21.0-qemu.tar.gz
[ac]: ../validation/cache-phase4-v0.21.0-preuves.tar.gz
