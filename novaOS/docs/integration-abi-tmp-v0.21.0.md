# Qualification intégrée du registre ABI v1 et du temporaire privé P9

Le 15 novembre 2026, les deux architectures ont exécuté les appels du temporaire privé ainsi que les applications déjà distribuées avec le SDK de NovaOS v0.20.0. Le manifeste du noyau indique toujours 0.20.0, le SDK `nova-system` 0.2.0 et NovaFS 0.9.13. Cette qualification porte sur l’intégration des phases réalisées de v0.21.0. La clôture de la version relève d’une décision ultérieure.

## Filiation du candidat

| Référence | Commit |
|---|---|
| Base commune, phase 1 sur `origin/main` | `a6df93fb8687c68893680707f8763471d9f8a2b3` |
| Registre ABI qualifié, branche locale `codex/abi-registry-v1` | `df88f1cfeaf0940da876ea543584e59215abacd4` |
| Qualification du temporaire privé, `codex/tmp-prive-0210` | `35f7d9619f116f42491f001af4b1cf5bdc84b395` |
| Réunion des deux historiques, parents dans l’ordre du tableau | `b64414da768c4c74a77808b2ec83fce46a4bc1cf` |
| Synchronisation du registre sur `codex/integration-abi-tmp-0210` | `23f56d7b2705a00e4e00b21400b4b853afa9a82b` |

La branche distante du registre était absente lors de la vérification. Le commit local qualifié et le commit du temporaire figurent tous deux dans l’ascendance du candidat. Le commit `c72b8e9a7dc68f2c7f87f80f732e7973acd035f9` précise la transmission des statuts de service 1 à 255 par `TMP_CALL`. Ce changement du registre et de son contrôle conserve les octets générés pour Rust et C. L’intégration conserve leurs chemins d’exécution et leurs contrôles de droits.

## Contrat des cinq appels

Le [registre canonique](../abi/syscalls.toml) attribue les numéros. Le noyau et les clients Rust incluent le même [module de constantes générées](../abi/generated/tmp.rs). Les consommateurs C et C++ incluent le même [en-tête](../abi/generated/nova_abi.h). Les 41 entrées antérieures ont été comparées champ par champ avec le registre qualifié. Les deux portes matérielles aboutissent au dispatch commun. L’ensemble de ses numéros, plages incluses, correspond exactement aux 46 entrées.

| Numéro | Nom Rust | Constante C/C++ | Contrôle observé |
|---|---|---|---|
| `0x10076` | `tmp::CAP` | `NOVA_SYS_TMP_CAP` | Rend la capacité déjà attribuée au processus si son bail est actif. |
| `0x10077` | `tmp::CALL` | `NOVA_SYS_TMP_CALL` | Vérifie le bail et les accès à la trame fichier de 128 octets. |
| `0x10078` | `tmp::WORKER` | `NOVA_SYS_TMP_WORKER` | Exige le principal, son droit de création et son identité complète avant de déléguer sa vue au travailleur. |
| `0x10079` | `tmp::NEXT` | `NOVA_SYS_TMP_NEXT` | Le serveur NovaFS admis présente son droit de réception et reçoit une enveloppe de 168 octets avec son ticket. |
| `0x1007A` | `tmp::COMPLETE` | `NOVA_SYS_TMP_COMPLETE` | Le même serveur présente le ticket en vol et le statut de la réponse. |

`instances::WORKER` conserve le numéro `0x10071` et la macro `NOVA_SYS_WORKER`. Le lanceur ordinaire conserve sa sémantique. La délégation explicite du temporaire utilise `tmp::WORKER`. Le registre documente à part les trois opérations internes du courtier, `CREATE`, `REMOVE` et `IO`. L’index généré donne pour chaque appel la signature, les statuts de retour et le lien vers son traitement. [Index des appels et des opérations de service](registre-abi.md) · [Contrat de révocation](contrat-tmp-prive-v0.21.0.md)

## Compatibilité des binaires

Chaque ELF a été relu et son SHA-256 comparé à la qualification v0.20.0 avant construction du noyau de test. Le banc `thread-probe` embarque ces octets dans le catalogue du noyau intégré et vérifie la copie. Les quatre applications ont été lancées telles quelles sous QEMU 11.1.3 TCG, avec deux vCPU. Les deux noyaux ordinaires ont en outre exécuté la sonde ABI historique. [Banc des ELF archivés](../tests/abi_elf_archive.py) · [Référence indépendante v0.20.0](validation/rust-sdk-phase2-v0.20.0.json)

| Architecture | ELF archivé | SHA-256 avant exécution | Résultat fonctionnel |
|---|---|---|---|
| AArch64 | `sdk-example` | `db2d14ec8a1f8d1ac22acc331ee4d3d90336a90adc8d50ca057c3129e129c3a8` | EL0, sortie 0 |
| AArch64 | `threads-test` | `fa1640df7589a2700f172acc43197c2904e6e2f833fb748048bc15984575cbd9` | 2 migrations, 62 088 cadres libres avant et après |
| x86_64 | `sdk-example` | `1ad5ebf8b6a0d80b10d2df826500f39052fdc222fac418640b408b9657d78c6e` | Ring 3, sortie 0 |
| x86_64 | `threads-test` | `6073a78fb2ffa5c716e75ac942932728faa218d614c3dedf6f30c9f067f6b30c` | 2 migrations, 62 615 cadres libres avant et après |

L’archive distribuée du SDK a été lue et contrôlée avec `distribution/scripts/sdk_rust.py --check`. Elle est identique à celle de la clôture, soit 21 237 octets, 23 entrées et le SHA-256 `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. Les trois tests hôte du SDK ont réussi sur Darwin ARM64 natif et sous Rosetta x86_64. Le premier essai x86_64 utilisait un compilateur Homebrew dépourvu de la bibliothèque cible. L’archive de preuves conserve son diagnostic et le journal du passage réussi avec la toolchain Rust du projet. [SDK distribué](../../distribution/sdk/nova-system-0.2.0.tar.gz)

## Matrice intégrée

| Parcours | Périmètre | Résultat |
|---|---|---|
| Registre ABI | 8 tests, génération répétée et mode `--check` | Produits identiques, vérification des deux portes utilisateur, 46 constantes Rust compilées sur chaque cible, en-tête validé en C11 et C++11 sur chaque cible |
| Temporaire privé P9 | 2 ISA × 2 géométries × 3 topologies SMP × 4 démarrages | 48/48, dont 12 arrêts forcés de QEMU et 12 reprises d’orphelins. Les 48 catalogues publics sont identiques à leur référence. |
| Autorité et concurrence | 1, 2 et 4 vCPU par ISA | Racine vide à l’admission, isolation entre familles, partage entre threads, délégation explicite, refus des handles périmés. Six bancs d’attente ont rendu `CANCELLED` avant remise, `UNCERTAIN` après remise et `DENIED` après retrait. Zéro ticket résiduel. |
| Instances et BootID | 4 familles par topologie SMP et par ISA, puis essais de redémarrage | Après arrêt normal suivi d’un reset QMP et après arrêt forcé suivi d’une relance, l’ancienne identité a été refusée et la courante acceptée à numéro local et génération identiques. Le travailleur a retourné 41. |
| Chemins CLI historiques P8 | AArch64 et x86_64 × 512 et 4 096 octets | 16 démarrages sur quatre volumes. Les remontages en lecture ont préservé les générations 43 et 44. Seules les sessions comportant une écriture ont avancé la génération initiale 40. |
| CLI et catalogue public sur P9 | 4 sessions du lanceur ordinaire (mode 41) et 4 du lanceur de qualification (mode 42) | 8/8. Lecture, création et suppression dans la vue publique, registre privé invisible. Après arrêt, les 8 relectures ont retrouvé la sentinelle OID 3 et seulement le registre dans le conteneur privé. |
| Refus du lanceur | Même matrice ISA/secteur, 4 motifs par cellule | 16 refus, zéro écriture bloc observée, SHA-256 identique avant et après. Les quatre motifs sont l’absence de source BootID, un volume invalide et les deux incompatibilités de profil P8/P9. |
| Récupération et quarantaine | Darwin ARM64 et x86_64 sous Rosetta × 512 et 4 096 octets | 312 injections aux barrières F1/F2, 8 préparations interrompues. Contrôles de quota, d’erreur de synchronisation, de quarantaine et de conservation de la sentinelle extérieure. |

Le Cortex-A72 a utilisé `UEFI-QEMU-RNG-SEED` sans RNDR et x86_64 a utilisé RDRAND. En l’absence de source qualifiée, les deux profils ont refusé l’identité durable. Les instantanés des volumes invités ont été relus par le lecteur NovaFS P9 de l’hôte. Les deux noyaux ordinaires, reconstruits au commit de synchronisation puis comparés octet par octet aux images testées, portent les SHA-256 suivants.

| AArch64 | x86_64 |
|---|---|
| `e40f561ae809bfc348af5e78dc18e6a9bcaec5bfabfce102471c7f14dd2642e1` | `cd6b3e1743f69d4591d82b9af813f1186bece04b21a88ba81b3a5fbc0fce96b0` |

## Écart bloquant lors de la fin du service

Le défaut décrit ci-dessous a été reproduit sur le candidat `97ea39c`. Il a ensuite été corrigé et requalifié sur les deux architectures. [Rapport local du correctif](retrait-service-tmp-v0.21.0.md)

Le contrat garantit `UNCERTAIN` au client pour une opération déjà remise dont l’issue reste inconnue. Sur ce premier candidat, la reprise du client après disparition du serveur suivait un autre chemin. Si le serveur s’arrête après `NEXT` et avant `COMPLETE`, `retired` retire les baux et efface la requête et le ticket en vol. À son réveil, le client encore vivant contrôle d’abord son autorité. La disparition du bail lui fait obtenir `DENIED`. Cet état interdit correctement une nouvelle opération. Pour l’appel déjà remis, il ne dit pas si le serveur a pu exécuter l’écriture. [Chemin de l’appel, de l’acquittement et du retrait](../kernel/src/sched/process/tmp.rs) · [Garantie de l’appelant](contrat-tmp-prive-v0.21.0.md#retrait-coupure-et-reprise)

Un essai additionnel QEMU TCG à un vCPU a reproduit le cas sur AArch64 et x86_64. Le serveur témoin a reçu une opération `STAT` (ticket 3) puis s’est arrêté normalement avec le code 0 avant `COMPLETE`. Sur chaque architecture, l’appel initial a rendu `DENIED` (5) au lieu de `UNCERTAIN` (22). Un nouvel appel avec l’ancien droit a rendu `DENIED` (5), comme attendu. Aucun bail ni ticket ne subsistait. La sonde utilise le courtier du candidat et un service témoin sans disque. Elle observe la perte de l’information de remise et ne mesure aucun effet sur une écriture physique.

Les six essais SMP d’annulation de l’appelant gardent le serveur vivant. Ils vérifient un cas différent. L’écart est présent dans le commit de phase 2 et dans le candidat intégré, qui utilise le même courtier. Ce résultat a suspendu la fusion dans `main`. L’arbitrage qui a suivi a demandé de révoquer les droits immédiatement, de conserver le résultat de chaque appel admis et de vérifier la restitution des tickets et des baux.

## Archives et contrôles reproductibles

| Archive | Pièces | SHA-256 |
|---|---:|---|
| [ELF v0.20.0, origines, six journaux invités et SDK](validation/integration-abi-tmp-0210-elf.tar.gz) | 26 | `b336de3a5bd66c673e812392a68cd1604b6cfb82c204ad2b4dd049e1aac4ceec` |
| [Deux noyaux, images P9, relectures indépendantes, attentes et injections](validation/integration-abi-tmp-v0.21.0-p9.tar.gz) | 151 | `58c2caf755f43e720fe4e7b054299152b626f1a6db16aa291a0e1fbb60e20135` |
| [Registre, CLI P8, refus sans écriture, BootID et instances SMP](validation/integration-abi-tmp-0210-compatibilite.tar.gz) | 131 | `34b3da3ee02df69319a41899906decca745fb73180e2f5c64ebf91ad0e828d5e` |
| [Lanceurs CLI P9 ordinaire et de qualification](validation/integration-abi-tmp-0210-cli-p9.tar.gz) | 62 | `027a4909fcd8763330a54051e664530cdf006b1b8d3f85b4eeb329b7e82a1ce6` |
| [Fin normale du serveur après remise, delta de la sonde et deux journaux invités](validation/integration-abi-tmp-0210-fin-service.tar.gz) | 11 | `87d18bb159cb47c9c57812380aad86497840194d16cdedb59f17108347df5a42` |

Les comptes excluent les manifestes SHA-256. Les archives contiennent les images de données, résultats et journaux employés comme preuves. Les ISO de session P8 et CLI P9, identifiées dans les métadonnées des VM, sont conservées dans le répertoire de construction durable du worktree.

Pour vérifier le registre et l’archive distribuée depuis la racine du worktree, utiliser les temporaires du projet. Les guides de qualification donnent les commandes des deux cibles et les paramètres des bancs invités. [Rejouer les ELF](qualification-registre-abi-v1.md) · [Rejouer le temporaire privé et la CLI](tmp-prive-phase2-v0.21.0.md#reproduire)

```sh
export NOVAOS_TMPDIR=/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp
export TMPDIR="$NOVAOS_TMPDIR/"
make -C novaOS abi-check
python3 -B novaOS/tests/abi_registre.py -v
python3 -B distribution/scripts/sdk_rust.py --check
```

## Portée et suite

Le lanceur du temporaire privé exige P9. P8 garde son lanceur historique et ses générations en lecture. L’intégration conserve les formats disque et les interfaces distribuées. Les coupures invitées ont été produites par arrêt du processus QEMU. Les injections F1/F2 relèvent du dispositif de panne du laboratoire. UTM, le matériel physique et la coupure électrique de l’hôte restent hors de cette qualification ciblée. Une restauration complète de VM, mémoire comprise, peut aussi restaurer l’identité et les capacités de l’instantané. [Hypothèses de fraîcheur](bootid-phase1-v0.21.0.md) · [Qualification du stockage de phase 2](tmp-prive-phase2-v0.21.0.md)

La MOE a autorisé la correction et la requalification des appels en vol lors de l’arrêt du service. Le candidat corrigé est soumis à examen local avant fusion ou publication. Le cache persistant, le gestionnaire de services et la commande `permissions` restent des chantiers distincts.
