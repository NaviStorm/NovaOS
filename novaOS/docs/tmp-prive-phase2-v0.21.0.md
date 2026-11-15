# Qualification du temporaire privé · phase 2 de v0.21.0

La phase 1 a été fusionnée dans `origin/main` au commit `a6df93fb8687c68893680707f8763471d9f8a2b3`. Sa seconde ascendance est le candidat qualifié `6b75a2bd0a449d2cb3e36c1db9b50292b811280e`. La phase 2 part de cette fusion sur `codex/tmp-prive-0210`. Les manifestes restent NovaOS 0.20.0, NovaFS 0.9.13 P9 et SDK 0.2.0. [Contrat de phase 2](contrat-tmp-prive-v0.21.0.md)

## Essais invités P9

QEMU 11.1.3, émulation TCG, 256 Mio, disque VirtIO PCI brut jetable. AArch64 utilise Cortex-A72 sans RNDR avec relais UEFI du profil QEMU et VirtIO-IOMMU. x86_64 utilise le CPU `max` et VT-d. Les deux ELF de cette matrice et de la campagne CLI finale sont identifiés ci-dessous. Les journaux individuels donnent les BootID, les sorties de processus, les générations, les acquittements SMP et les statistiques du pilote.

| Invité | SHA-256 du noyau | Secteurs | vCPU par géométrie | Démarrages |
|---|---|---|---|---:|
| AArch64 | `f98257c49744d51e1f304122507bebd4943273e50e1652875abd6039b070f1f7` | 512 et 4096 | 1, 2 et 4 | 24/24 |
| x86_64 | `b4eba6e7032ca786cf277c4d45e437be48af6a6016da0d214af12f828b409c0d` | 512 et 4096 | 1, 2 et 4 | 24/24 |

Chaque cellule démarre quatre fois le même disque avec une époque neuve. Le premier invité lance deux principaux du même manifeste. Chacun ouvre `commun` dans un répertoire initialement vide. Le banc lit les deux contenus, vérifie un thread, des enfants avec et sans délégation, un handle étranger, un handle fermé, les deux sens du renommage interne et deux destinations extérieures refusées. Les portes réservées au serveur et un tampon de réponse en lecture seule sont refusés. Un principal sort normalement, un autre est arrêté avec un WRITE préparé et un handle ouvert, le troisième déclenche une faute utilisateur. Le noyau termine ensuite la session et recueille les acquittements des vCPU.

Le deuxième démarrage réutilise effectivement le numéro local et la génération du premier principal et obtient une racine vide. Au troisième, l’hôte envoie SIGKILL à QEMU après confirmation de SYNC, avec l’application encore vivante. Le lecteur indépendant retrouve deux racines et leurs marqueurs, dont celle de `nsh`, et le contenu synchronisé `A`. Le quatrième démarrage retire ces orphelins avant de fournir une nouvelle vue. Après chaque sortie normale seul le sceau demeure. Les 48 BootID observés sont distincts.

Après chacun des 48 démarrages, le lecteur hôte ouvre le disque en lecture seule, vérifie P9 et compare au témoin initial le chemin, le type, l’OID et tous les octets des fichiers publics, en descendant aussi les répertoires vides. Les 48 comparaisons sont identiques. Les métadonnées d’accès dépendantes de l’heure sont exclues de ce manifeste. Les images relues, la série, les résultats JSON et les empreintes figurent dans l’[archive QEMU](validation/tmp-prive-phase2-v0.21.0-qemu.tar.gz).

## Compatibilité et refus d’admission

| Épreuve | Résultat observé |
|---|---|
| CLI historique sur P8, 2 ISA × 2 géométries × 4 démarrages | 16/16. Génération initiale 40, après modifications 43, après création suivie d’une coupure 44. Les sessions de lecture au remontage conservent respectivement 43 et 44. Les assertions historiques sont exécutées telles quelles. |
| CLI v0.21.0 sur P9, AArch64/512 et x86_64/4096 | 2/2. Sept commandes par session couvrent énumération, lecture, création, suppression, droits et révocation. Après arrêt, génération 96, sentinelle OID 3 intacte et seul sceau présent pour les deux images. [Preuves CLI](validation/tmp-prive-phase2-v0.21.0-cli.tar.gz) |
| P8 donné au lanceur P9 | 4/4. Profil de qualification sur 512 et lanceur ordinaire sur 4096, pour les deux ISA. Message `TMP format P8 non pris en charge`, code `UNSUPPORTED` (25) pour le service et pour l’admission du principal. Aucune invite. |
| P9 donné au lanceur historique P8 | 4/4. Refus explicite du format, aucune invite. |
| Source BootID absente sur P9, ou volume invalide | 8/8. Dans le premier cas, refus 12 à l’admission ; dans le second, la chaîne de stockage signale l’échec. Aucune instance applicative n’obtient de capacité. |

Pour les 16 refus, la trace QEMU des requêtes du périphérique ne contient aucune écriture. Une image initiale comprimée a été relue et comparée octet par octet au disque après l’essai. Les journaux des deux refus de format attestent des lectures. Les profils P8 et P9 sont sélectionnés explicitement au démarrage. La vérification des deux ancres et manifestes a lieu avant le montage inscriptible. La construction d’une image fictive par le banc est distincte de l’admission d’un volume existant.

## Injections et régressions ciblées

| Banc | Couverture et résultat |
|---|---|
| Moteur P9 du service sur Darwin ARM64 natif et x86_64 Rosetta, 512/4096 | Pour chaque couple, 24 injections F1/F2 en préparation du registre, 18 en création et 36 en purge. **312 injections**, plus **8 coupures** aux deux emplacements de préparation. Reprise idempotente, sentinelle extérieure intacte. [Images et journaux du stockage](validation/tmp-prive-phase2-v0.21.0-stockage.tar.gz) |
| Défauts et limites du stockage | Registre préoccupé, substitution, marqueur incomplet, 63 marqueurs incomplets, registre surdimensionné, quota insuffisant, volume plein et erreur de FLUSH. Refus ou quarantaine vérifiés, conservation des objets extérieurs. Le profil P9 ne possède ni type lien symbolique ni lien physique de catalogue. Le contenu d’un fichier ressemblant à un chemin extérieur n’est jamais interprété pendant le retrait. |
| Attentes et révocation, 2 ISA × 1/2/4 vCPU | 6/6 avec le courtier réel, un écrivain témoin et sans disque. Réveil d’un récepteur déjà inscrit, annulation avant et après remise, acquittement tardif, refus après destruction, zéro ticket et zéro bail après purge. Les scénarios disque et ce banc vérifient deux frontières distinctes. |
| Phase 1 et IPC | Instances sur 1/2/4 vCPU, deux campagnes BootID (reset QMP, SIGKILL suivi de relance, absence de source), courses IPC sur les deux ISA. [Journaux du noyau](validation/tmp-prive-phase2-v0.21.0-noyau.tar.gz) |
| Régression VFS publique | Quatre cellules `namespace.py --only normal` en 512/4096 sur les deux ISA. Deux clients, refus et mutations, identité du chemin de 1 074 octets après redémarrage. Les sept tests Python de collecte et conservation des preuves ont réussi. |

Deux défauts détectés au cours de la qualification ont été corrigés. La révocation du canal temporaire d’une commande CLI interrompait aussi la boucle du service public, ce qui faisait échouer la commande suivante. Le service ferme désormais la délégation concernée et reprend la session publique. Le deuxième défaut effaçait le motif d’échec d’admission lorsque le service quittait avant son premier échange. Le lanceur transmet désormais son refus de format au principal en attente.

## Intégrité des archives

| Archive | SHA-256 |
|---|---|
| QEMU, 174 fichiers relus | `b4448a260e19c287ad1e04cbf16d5b461e24d38adda011e4e85b1edeb28a3fb4` |
| CLI, 68 fichiers relus | `67aea76e61ddfa9a2cf2e095449b2d338c10b15e62b8132dbb625cad063cb25e` |
| Stockage, quatre images P9 et deux journaux | `2183f17efc55278ab97fb51691e338c568bffec2cf0addd8d3d3da92279fcdbb` |
| Noyau et régressions, 36 pièces | `a069e8e9c3c677f34c145cacfb231a42b7f2de52280a258879e1a6db8bc81794` |

## Reproduire

Depuis la racine du worktree durable sur macOS, définir `NOVAOS_TMPDIR` selon les consignes du dépôt et utiliser un répertoire de preuves neuf à chaque lancement. Répéter les profils avec `NOVA_ARCH=x86_64` et `aarch64`, les deux tailles de secteur et 1/2/4 vCPU.

```sh
export NOVAOS_TMPDIR=/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp
export TMPDIR="$NOVAOS_TMPDIR/"
export NOVA_ARCH=aarch64
make -C novaOS ARCH="$NOVA_ARCH" kernel
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --lib --bin novafs
python3 -B novaOS/tests/tmp_prive.py --secteur 512 --vcpus 4 --preuves novaOS/build/mon-essai-p9
python3 -B novaOS/tests/tmp_prive_refus.py --production --secteur 4096 --preuves novaOS/build/mes-refus
python3 -B novaOS/tests/cli.py --sector 512 --preuves novaOS/build/mon-essai-p8
python3 -B novaOS/tests/tmp_prive_cli.py --secteur 512 --preuves novaOS/build/ma-cli-p9
python3 -B novaOS/tests/tmp_attentes.py
sh novaOS/tests/tmp_prive_stockage.sh "$NOVA_ARCH"
```

La qualification couvre ces deux machines QEMU et le modèle de panne exercé. Le profil UTM installé, NVMe, une coupure électrique physique et la restauration d’une VM mémoire comprise n’ont pas été qualifiés ici. Au terme de cette campagne de phase 2, la coordination des cinq sélecteurs avec le registre ABI restait à réaliser. La [qualification intégrée](integration-abi-tmp-v0.21.0.md) consigne le résultat sur le code commun. Le cache persistant et la commande `permissions` relèvent d’arbitrages ultérieurs.
