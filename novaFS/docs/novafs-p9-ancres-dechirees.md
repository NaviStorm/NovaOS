# NovaFS v0.9.13 P9 : récupération après déchirure d’une ancre

**Qualification UTM 5.0.8 du 12 novembre 2026.** L’atomicité de l’ancre lors d’une perte d’alimentation reste non qualifiée. Les 96 essais au placement GPT installé ont tous rendu un état complet. Le contrat VirtIO laisse indéfini le contenu persistant d’une écriture encore volatile. Les résultats, les limites du stockage hôte et la décision à prendre figurent dans l’[audit UTM](#atomicité-des-ancres-sur-le-stockage-utm).

**Décision de qualification : le montage automatique du volume P9 autonome ne peut pas être autorisé sous les garanties demandées.** Une génération dont tous les blocs passent les contrôles peut être antérieure au dernier remplacement confirmé. Deux historiques qui exigent des décisions différentes produisent exactement la même image. Le refus de montage reste une limite de disponibilité, pas un résultat acceptable pour livrer le remplacement.

Base : `03e267b9cf710d7d53612ba89064a9af156be054`, branche `main` propre au départ. Les injections s’exécutent sur des disques simulés jetables, avec le moteur réel. La primitive, la reconstruction et les adaptateurs de test sont absents du service NovaOS. [Cas de coupure et de reconstruction](../src/fs/remplacement/recuperation.rs) · [Essais sur le pool chiffré](../src/secure_pool/qualification_ancres.rs) · [Mesures de validation](validation/v0.9.13/ancres-dechirees.json)

## Ce qui est écrit et ce que le montage peut prouver

Les blocs logiques 0 et 1 du volume portent A et B, de 4 096 octets chacun. Le formatage place le même manifeste de génération 1 dans les deux références, avec une enveloppe différente par position. Ensuite, la génération paire remplace A et l’impaire B. À partir de la génération 2, ce ne sont plus deux copies du dernier état. [Encodeur et décodeur](../src/codec.rs) · [Contrat de publication](novafs-recovery.md#publication-dune-génération)

| Octets (fin exclue) | Contenu de l’ancre autonome |
| --- | --- |
| 0–16 | Signature `NOVAP0FS`, version/réservé à zéro, type d’ancre, taille d’en-tête 96. |
| 16–56 | Identifiants FS et LV de 16 octets chacun, puis numéro du bloc de l’ancre. |
| 56–96 | Génération sur 8 octets, puis SHA-256 sur 32 octets. |
| 96–136 | Taille de bloc 4 096, réserve à zéro, capacité en blocs à l’offset 104, puis réserve à zéro. |
| 136–184 | Référence au manifeste : adresse, génération de naissance, SHA-256. |
| 184–4 096 | Zéros obligatoires. |

Les entiers sont en little-endian. Il n’y a **ni CRC, ni MAC, ni compteur externe** dans cette enveloppe. Le SHA-256 couvre le préfixe de domaine `NovaFS.P0.meta\0` et les 4 096 octets, avec le champ d’empreinte remis à zéro pour le calcul. Les données ont un domaine distinct qui lie aussi les identifiants, l’adresse et la naissance. Ces contrôles détectent la corruption ; une empreinte sans clé stockée sur le même disque n’authentifie pas celui-ci contre une réécriture malveillante.

Le moteur écrit hors des deux graphes protégés, attend les écritures CoW (données, arbres, espace et manifeste), puis attend **F1**. Il écrit ensuite une seule ancre, attend sa complétion puis **F2** avant de rendre le succès. Après redémarrage, le montage normal RO comme RW charge **les deux** ancres ; il contrôle position, identités, géométrie, parité et empreintes. Hors du cas initial, le manifeste récent doit engager la génération **et le digest** du précédent. Le contrôle traverse les cinq arbres, les données, la propriété des blocs, les âges de retrait et les références partagées. Le flush d’activation RW vient après ces contrôles. [Moteur et sélection de la paire](../src/engine.rs) · [Contrôle hors ligne par slot](../src/engine/integrity.rs)

**Pourquoi 96 octets suffisent à bloquer.** Après F1, un préfixe nouveau de 96 octets met en place la génération et le checksum de l’ancre candidate. L’adresse, la naissance et l’empreinte du manifeste à partir de 136 appartiennent encore à l’ancienne occupation du slot, deux générations plus tôt. Le nouveau checksum ne couvre pas ce mélange. Le décodeur rejette l’enveloppe avant de suivre la référence ; le montage ne choisit donc aucun des deux slots. Les 96 octets sont un exemple reproductible et non un seuil universel.

## Campagne sur le volume autonome

Sur chaque architecture : déplacement et publication d’une copie préparée, slot A puis B, secteurs de 512 puis 4 096 octets. Pour chacune des huit combinaisons, **32 plages × 9 événements = 288 coupures**. Les neuf points sont la soumission, l’effet et la complétion de F1 ; avant l’écriture d’ancre, à sa soumission et à sa complétion ; puis la soumission, l’effet et la complétion de F2. La trace et le reçu de l’appel sans panne servent d’oracles au banc, jamais d’entrées au montage. Le témoin historique rejoue aussi tous les événements d’E/S et les masques de secteurs entiers.

| Octets neufs persistants pendant que l’ancre est volatile | Résultat vérifié sur la fixture |
| --- | --- |
| Préfixe de 1 ou 32 octets | Ces octets sont identiques dans les deux versions. Ancienne paire valide, ancienne destination. |
| Préfixe de 64, 96 ou 128 octets | Métadonnées hybrides. RO et RW refusés avant écriture ou flush ; autre génération intégralement vérifiée et extractible en RO. |
| Préfixe de 184, 256, 512, 1 024, 2 048 ou 4 096 octets | L’ancre est identique, sur **tous** ses octets, à l’ancre nouvelle. Le manifeste et ses dépendances sont stables depuis F1. La nouvelle destination est lisible même si le succès n’est pas parvenu au client. |
| Plages intérieures ou à cheval sur deux champs ou secteurs | La plage est injectée à sa position réelle. Le résultat attendu est calculé octet par octet à partir des deux images de référence : ancienne enveloppe, nouvelle enveloppe ou refus. La liste exacte figure dans `PLAGES` du banc. |
| Deux écritures complètes de dépendances avant F1, autres écritures volatiles perdues | Deux blocs distincts de 4 Kio persistent. Les ancres et tous les autres blocs restent identiques à l’image initiale ; seule l’ancienne génération monte. En géométrie 4 Kio, une commande d’ancre ne contient jamais plusieurs secteurs. |

| Par architecture | Ancien état au montage | Nouvel état au montage | Refus normal | Total des injections de plage |
| --- | ---: | ---: | ---: | ---: |
| Secteurs 512 | 644 | 328 | 180 | 1 152 |
| Secteurs 4 096 | 644 | 328 | 180 | 1 152 |

Les huit coupures supplémentaires à deux blocs sont hors de ces totaux. Chaque montage réussi contrôle reçu, contenu exact, identité et quatre dates de la destination, ainsi que le nom source et l’original de la copie. Les écritures de préparation n’ont touché **aucune** adresse appartenant à l’un des deux manifestes, à leurs arbres ou à leurs pages de données. Dans chaque refus, le rapport indépendant établit quel slot est réellement exploitable ; le mode de sauvetage refuse une création sans produire d’E/S de mutation. Aucun mélange de générations ni destination manquante n’a été accepté.

## Pourquoi on ne peut pas choisir automatiquement la tête survivante

1. **Coupure avant confirmation.** À partir d’un état confirmé `g`, préparer le remplacement et réussir F1. Pendant F2, ne stabiliser que le préfixe de 96 octets. Le dernier succès rendu est celui de `g`.
2. **Corruption après confirmation.** Partir de la même base et terminer le même remplacement à horloge fixée. Le montage indépendant vérifie `g+1` et la nouvelle destination. Après ce succès, injecter un défaut latent sur le slot publié : rétablir uniquement ses 48 anciens octets de référence au manifeste.

Les **2 097 152 octets**, dépendances CoW comprises, sont identiques dans les deux images. La preuve a été répétée pour les deux parités et géométries (reçus 6/7 et 7/8). Il n’est pas supposé qu’une coupure ordinaire annule un flush réussi : le second historique est précisément le cas de corruption que le montage ne doit pas masquer. Même un avertissement suivi d’un repli automatique ferait perdre l’état acquitté dans le second historique. À l’inverse, le manifeste candidat et ses dépendances sont également présents sur une troisième image coupée **avant toute écriture d’ancre**. Les promouvoir sur la foi de leur présence confondrait préparation et publication.

**Stratégie éprouvée sur copie.** Le contrôle vérifie entièrement le slot survivant, puis le compare au reçu supposé frais de l’observateur extérieur. Le manifeste survivant donne la naissance et le digest de son propre parent. Le banc recherche exclusivement ce parent, refuse son absence ou son ambiguïté, reconstruit l’enveloppe du parent sur une branche mémoire indépendante et exige un **montage normal RO** de la paire ainsi reconstituée. Avec le reçu `g`, le graphe, l’espace et l’ancienne destination sont vérifiés. Avec le reçu confirmé `g+1`, le repli vers `g` est refusé. Une altération supplémentaire d’une page de l’ancienne destination invalide aussi le secours, malgré son enveloppe intacte. Aucun octet du support source n’est réparé.

Reconstituer au contraire l’ancre **nouvelle** à partir du reçu candidat permet même de retrouver exactement le SHA-256 neuf conservé dans le préfixe de 96 octets. Sur copie, la paire ainsi reconstituée monte aussi bien sur l’image déchirée que sur celle dont l’écriture d’ancre n’avait jamais été soumise. C’est une preuve de cohérence du candidat, pas de publication complète. Une déchirure qui ne conserve qu’une partie du checksum n’offre même pas cette comparaison.

Le reçu de laboratoire est une référence imposée par le test, **pas un service de fraîcheur**. Le test n’authentifie pas l’émetteur d’un reçu réel et ne met en œuvre aucun protocole de coordination avec F2. Un reçu ancien, facultatif, ou conservé dans la même image ne suffit pas. Si un observateur fiable sait que la seule panne est une coupure confinée à l’écriture en cours, le slot intact conserve la génération précédente ; le disque seul n’atteste pas cette prémisse. Sans autorité indépendante, reconstruire une paire cohérente ne prouve pas qu’elle était la dernière. Le banc ne réactive ni l’écriture, ni la collecte, ni le scellement à partir d’une vue de sauvetage.

## Snapshots, clones et chiffrement

Le pool P9 authentifié publie un **catalogue externe au LV**. Quatre emplacements logiques de génération portent chacun deux attestations HMAC physiquement distinctes ; le catalogue et les unités scellées ont leurs propres vérifications. Ces attestations ne sont pas des copies cachées des ancres d’un volume P9 autonome. Le [service installé](../../novaOS/user/novafs/files.rs) monte le volume par `Engine::mount_current`. [Contrat du pool et limite de fraîcheur](novafs-p7-chiffrement.md#publication-perte-de-fraîcheur-et-traitement-des-erreurs)

Le banc a exécuté le même remplacement privé dans l’origine d’un pool P9 chiffré, configuré en LZ4. Un snapshot conserve l’ancienne destination ; un clone possède sa propre destination divergente. Six injections par géométrie couvrent F1, chacune des deux copies du certificat, et l’effet de F2. Une copie partielle de 96 octets seule n’autorise pas la nouvelle génération ; une copie authentique entière suffit dans ces essais. Chaque réouverture vérifie le graphe, les trois contenus et le parent du clone. Une clé administrative incorrecte, un domaine verrouillé et une mauvaise clé de récupération interdisent l’accès correspondant ; les inspections n’écrivent rien. La politique de domaines et de nonces n’a pas été modifiée ; les tests historiques de rechiffrement, de rejeu du nonce après interruption et de migration sont conservés. Cela ne qualifie pas un passage du disque système au profil pool.

## Régressions exécutées

| Contrôle | ARM64 natif | x86_64 sous Rosetta pour NovaFS, invité QEMU pour NovaOS |
| --- | ---: | ---: |
| NovaFS standard / instrumenté, zéro échec | 196 / 197 | 196 / 197 |
| Opérations différentielles, 100 graines | 1 000 000 | 1 000 000 |
| Remontages et audits du modèle / publications observées | 1 000 / 345 183 | 1 000 / 345 183 |
| Interopérabilité liblz4 1.11.1, pages vérifiées dans chaque sens | 256 / 256 | 256 / 256 |
| NovaOS : démarrages / groupes bloc / ELF-processus-IPC / CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| DMA/GICv3 / DMA séquentiel, QEMU 10.0.12 livré avec UTM 5.0.8 | 11 / 23 | Profil AArch64 uniquement |

Aux 192 tests standards et 193 instrumentés historiques s’ajoutent le témoin antérieur du remplacement et les trois nouveaux tests. Le corpus couvre les transactions, la récupération, les snapshots/clones, les archives, le chiffrement, la compression et la migration P8/P9. Les quatre tests marqués optionnels dans Cargo ont été exécutés séparément : différentiel, image de pool, coût des extents et corpus LZ4. L’indicateur macOS `sysctl.proc_translated` vaut 1 dans le processus x86_64. [Effectifs, reçus, versions et empreintes](validation/v0.9.13/ancres-dechirees.json)

Les coupures dans le secteur sont celles du simulateur : aucun test n’a interrompu l’alimentation physique du stockage hôte. Les régressions NovaOS exercent les commandes distribuées et les protections DMA ; elles ne démontrent ni l’atomicité du stockage réel, ni un remplacement depuis NovaShell.

## Arbitrage à l’issue des injections simulées

**Priorité proposée :** qualifier, sans changer le format P9, une chaîne de stockage où tous les octets variables d’une ancre sont publiés indivisiblement à leur adresse et où une coupure ne touche pas l’autre slot. Une écriture atomique de 4 Kio convient ; pour ce format précis, une garantie comparable sur le premier secteur aligné peut suffire si le reste du bloc reste inchangé. Il faut également démontrer F1/F2 de bout en bout. La taille de secteur annoncée, un flush isolé ou un arrêt de VM ne démontrent pas ces propriétés.

Si cette garantie est indisponible, arbitrer un **témoin de publication authentifié indépendant et résistant au recul** : identité FS/LV et lignée, génération, racine et ordre durable des acquittements. Il faudra qualifier les coupures entre les deux supports, les clones, l’absence du témoin, la rétention des blocs et la sécurité des clés avant d’envisager RW. Ce serait une nouvelle architecture, pas une modification implicite des images existantes. Dans l’attente, NovaOS reste en v0.15.0 et NovaFS en v0.9.13 P9 ; aucun changement d’IPC, de droits ou de format.

## Reproduction

Depuis la racine du dépôt sur le Mac de qualification, avec les deux cibles Rust Darwin et Rosetta :

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib fs::remplacement:: -- --nocapture
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib qualification_ancres -- --nocapture
    python3 -B novaFS/lab/p51-validation-suites.py "$arch"
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test
done
python3 -B novaFS/lab/p6-lz4-interop.py --bibliotheque /opt/homebrew/opt/lz4/lib/liblz4.dylib
```

Les bancs DMA utilisent les [procédures et firmwares expérimentaux UTM](../../novaOS/docs/installer-v0.15.0.md). Les tests de cette qualification n’exigent pas l’installation d’une nouvelle version du système.

## Atomicité des ancres sur le stockage UTM

Base vérifiée `79f27b6da601900a2c62f9dde418f047a9c4cf0b`, branche `main` propre au départ. Le profil autonome livré utilise AArch64, GICv3 sans ITS, INTx, VirtIO-IOMMU sans bypass, TPM CRB et une seule cible VirtIO PCI. La ligne de lancement de l’application graphique archivée désigne `cible.qcow2` avec `discard=unmap,detect-zeroes=unmap`. Les nouveaux essais appellent le framework QEMU 10.0.12 de cette même application sur le Mac ARM64. L’interface graphique n’a pas été relancée pour mesurer les coupures. Le profil graphique publié utilise des secteurs de 512 octets. La variante 4 096 et le format brut sont évalués sur le moteur. [Journal du profil graphique, membre `utm/autonome.log`](../../novaOS/docs/validation/installation-utm-v0.15.0/preuves.tar.gz) · [Configuration du disque VirtIO](../../novaOS/tests/profils_utm.py)

### De l’ancre au support

| Secteur logique | Capacité de la cible | Premier LBA logique des slots A / B | Écritures VirtIO par ancre | Secteur portant les champs de l’ancre |
| --- | ---: | ---: | --- | --- |
| 512 octets | 64 Mio | 124 928 / 124 936 | 8 OUT successives de 512 octets | Le premier des huit |
| 4 096 octets | 268 Mio | 67 840 / 67 841 | 1 OUT de 4 096 octets | Les champs tiennent dans les 184 premiers octets du secteur unique |

Chaque ancre occupe un bloc P9 de 4 096 octets. Ses champs occupent les octets 0 à 183, puis viennent 3 912 zéros obligatoires. La GPT place le volume à 61 ou 265 Mio du début du disque. Le second slot commence exactement 4 096 octets plus loin. L’adaptateur découpe la page en secteurs logiques et attend le statut de chaque écriture. Les fragments IPC sont rassemblés avant de soumettre l’OUT. Sur le fil, les adresses et longueurs se comptent toujours en unités de 512 octets. La géométrie vérifiée au moniteur QEMU annonçait un bloc logique et un bloc physique virtuel de même taille, respectivement 512/512 et 4 096/4 096. [Adaptateur NovaFS](../../novaOS/user/novafs/device.rs) · [Transfert IPC](../../novaOS/user/storage/medium.rs) · [Requête VirtIO et attente de complétion](../../novaOS/kernel/src/hardware/block/mod.rs)

| Couche | Garantie documentée et portée dans ce profil |
| --- | --- |
| VirtIO Block | Une écriture soumise reste volatile jusqu’à satisfaction des conditions de stabilité. Sur le support persistant, le contenu des secteurs concernés est indéfini pendant cette fenêtre. Dans le mode utilisé, un FLUSH soumis après complétion des OUT, puis achevé avec succès, engage leur persistance selon le contrat de l’appareil. La topologie annoncée n’ajoute aucune garantie d’indivisibilité. [OASIS VirtIO 1.2, § 5.2.4 et 5.2.6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html) |
| QEMU 10.0.12 | Le modèle contrôle l’alignement du secteur de départ et le multiple de la longueur logique. L’OUT suit `blk_aio_pwritev`. Le FLUSH soumet les écritures en attente puis appelle `blk_aio_flush`. NovaOS envoie les types ordinaires 1 et 4 et négocie FLUSH. Il n’y a pas de demande FUA (*Force Unit Access*) par OUT dans ce chemin. Une obligation de persistance par écriture ne suffirait d’ailleurs pas à spécifier l’état visible pendant sa réalisation. [Pilote du tag 10.0.12](https://github.com/qemu/qemu/blob/v10.0.12/hw/block/virtio-blk.c#L225-L382) · [Politique de cache QEMU](https://qemu.readthedocs.io/en/stable-10.0/system/invocation.html) |
| Images | Le brut adresse les octets du fichier directement. QCOW2 alloue les données et les métadonnées par clusters, avec tables L1/L2 et compteurs de références. Les deux ancres sont dans le même cluster invité de 64 Kio sur le disque QCOW2 essayé. Le format ne promet aucune publication atomique des octets de l’ancre pendant une panne du support. Les images du banc sont indépendantes, sans fichier parent. QMP a confirmé le cache en écriture, les E/S via le cache hôte et les FLUSH activés (`writeback=true, direct=false, no-flush=false`). [Format QCOW2](https://www.qemu.org/docs/master/interop/qcow2.html) |
| macOS et support physique | Images sur APFS local, macOS 27.2 (26B123). APFS et le SSD Apple Fabric indiquent 4 096 octets adressables. Apple documente le CoW des métadonnées et le renommage atomique de fichiers groupés. Ces opérations n’établissent aucune unité d’écriture indivisible après perte d’alimentation pour les données d’un fichier ouvert et modifié en place. La granularité physique de programmation du SSD et sa taille d’écriture atomique sous perte d’alimentation sont inconnues sur ce chemin. [Garantie publiée pour APFS](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/Features/Features.html) |

Le backend fichier du tag 10.0.12 appelle `qemu_fdatasync`. Dans la tranche ARM64 du framework UTM mesuré, `_handle_aiocb_flush` appelle cette fonction à `0x4967b8`, et celle-ci branche directement sur `_fsync` à `0x8d69d0`. Apple distingue `fsync` de `F_FULLFSYNC`, qui demande en supplément au périphérique de vider ses tampons vers le stockage permanent. Cette voie ne prouve donc pas que les octets ont atteint un support résistant à une panne d’alimentation du Mac au retour de F2. **Durabilité** désigne la conservation d’une écriture réussie à travers la panne envisagée. **Atomicité** exige qu’une interruption de l’écriture expose l’ancienne valeur entière ou la nouvelle valeur entière. F1 ordonne les dépendances CoW et F2 précède l’acquittement, mais ces deux barrières ne rendent pas indivisible une OUT en cours. [Backend fichier](https://github.com/qemu/qemu/blob/v10.0.12/block/file-posix.c#L1626-L1640) · [Wrapper de synchronisation](https://github.com/qemu/qemu/blob/v10.0.12/util/osdep.c#L608-L615) · [Contrat Darwin](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html)

### Coupures sur le moteur distribué

Le moteur NovaFS privé de test prépare la transaction réelle en mémoire, enregistre ses cinq écritures CoW, F1, l’écriture de l’ancre et F2, et vérifie l’unicité des blocs écrits. Le déplacement publie le slot B (génération 7), la copie préparée le slot A (génération 10). Une sonde AArch64 dédiée rejoue ces octets par le pilote VirtIO du noyau sur une cible jetable. Elle vérifie la désignation, le numéro de série, la géométrie, l’étiquette propre à l’essai et les bornes d’adresse avant d’autoriser les sorties. Le support d’amorçage en lecture seule conserve son SHA-256. Le domaine UEFI du SID 48 est retiré avant l’usage du SID 56 par le noyau. L’étiquette et le journal d’essai ne participent à aucune décision de montage. [Fixture et remontages RO/RW](../src/fs/remplacement/utm.rs) · [Sonde privée](../../novaOS/kernel/src/hardware/block/qualification/ancres.rs) · [Orchestration QMP](../../novaOS/tests/atomicite_ancres.py)

Chaque essai démarre une VM neuve. Le banc compare le nombre et l’adresse de toutes les OUT aux traces VirtIO, vérifie les compteurs d’octets écrits et de FLUSH terminés, ferme le processus, vérifie le conteneur QCOW2 sans réparation et relit en brut. Il compare les octets du volume et tous les octets environnants aux écritures exécutées. Le moteur NovaFS tente ensuite de remonter **chaque volume de 2 Mio sur les deux architectures**, en RO et RW. Il vérifie le graphe, le reçu, l’identité, le contenu et les quatre dates de la destination, ainsi que la source du déplacement ou l’original de la copie. Dans un refus normal, le test vérifie l’ancien graphe en sauvetage RO explicite et l’identité octet par octet du volume après inspection.

| Campagne sur le moteur UTM | Images | Ancienne génération complète | Nouvelle génération complète | Montage normal refusé |
| --- | ---: | ---: | ---: | ---: |
| Position GPT normale, transaction complète, 12 points par opération, format et géométrie | 96 | 48 | 48 | 0 |
| Placement artificiellement décalé, arrêt après la première OUT d’ancre | 30 | 8 | 10 | 12 |
| Mise à jour volontaire d’une plage de l’ancre, par secteurs entiers | 16 | 4 | 0 | 12 |
| **Total** | **142** | **60** | **58** | **24** |

| Point de l’arrêt, au placement GPT normal | Résultat dans les huit combinaisons déplacement/copie × brut/QCOW2 × 512/4 096 |
| --- | --- |
| Avant transaction, après la première OUT de préparation, avant F1, pendant F1, après F1, pendant la première OUT d’ancre | Ancienne génération aux six points. |
| Après la première OUT d’ancre, avant F2, pendant F2 | Nouvelle génération aux trois points. Sur 512, les sept secteurs suivants contenaient déjà les zéros attendus. Un état nouveau peut être lisible avant que le demandeur ait reçu le succès. |
| Après F2 avant réponse, après réponse de succès de la sonde, après fermeture ordonnée | Nouvelle génération aux trois points. |

Les 24 arrêts en vol utilisent `blkdebug` **entre le pilote de format et le fichier**. Le moniteur attend le point `write_aio` (8) ou `flush_to_disk` (16), et les compteurs confirment que la requête en cause n’est pas terminée. La VM subit ensuite un SIGKILL. Ce point précède l’effet de la requête dans le backend fichier. Il ne donne pas accès à un instant interne de `pwrite`, de `fsync` ou de la programmation du SSD. Les autres arrêts forcés sont déclenchés aux jalons acquittés du pilote, sans filtre. Huit arrêts ordonnés attendent F2, la réponse de succès, la fermeture du transport et la sortie normale de QEMU. Au total, **134 SIGKILL, 8 arrêts ordonnés, aucune coupure électrique du Mac** dans les 142 essais. Le SIGKILL supprime la RAM de la VM et de QEMU tandis que le noyau macOS, ses pages de cache et le contrôleur restent alimentés. [Matrice par VM, compteurs et empreintes](validation/v0.9.13/atomicite-utm.tsv)

Les adresses VirtIO doivent rester alignées sur le secteur logique. Pour tester une ancre non alignée, le banc décale le contenu du volume et préserve les octets voisins par lecture-modification-écriture de secteurs entiers. La coupure entre deux commandes laisse un préfixe neuf de 1 ou 32 octets (ancienne ancre identique), de 64, 96 ou 128 octets (ancre hybride refusée), ou de 184 ou 256 octets (nouvelle ancre identique). Deux essais supplémentaires à 512 décalent le volume d’un secteur entier par rapport à l’alignement de 4 Kio. Les deux retrouvent la nouvelle génération. Au placement normal, les plages écrites volontairement sont `[0,96)`, `[64,96)`, `[136,184)` et `[511,513)`. Les trois premières produisent un refus dans chaque configuration. La dernière ne change que des octets qui étaient déjà nuls. Ces contrôles montrent comment un mélange est détecté. Aucune déchirure spontanée d’un secteur aligné n’a été observée.

### Acquittement et redémarrage sur le disque installé

Deux installations neuves issues de l’archive distribuée v0.15.0 fournissent quatre disques système, brut et QCOW2 dans chaque géométrie. Sur chaque disque seul, NovaShell exécute `cp /systeme /audit` vers un nom neuf. Le retour de l’invite précède le SIGKILL. Une nouvelle VM démarre sur le même disque, monte normalement P9 et lit exactement `NovaOS\n` dans `/audit`. La session se termine avec retrait DMA acquitté. Les deux GPT passent ensuite la vérification hors ligne. Ces quatre essais utilisent les exécutables distribués. La commande du système livré crée un fichier. La publication privée sur une destination existante est celle du banc à 142 VM.

| Géométrie et formats | OUT d’ancre enregistrées pendant la commande CLI | FLUSH achevés avant retour de l’invite | Relectures après SIGKILL |
| --- | --- | ---: | ---: |
| 512, brut et QCOW2 | 4 publications × 8 OUT d’un secteur filaire | 8 dans chaque cas | 2/2 |
| 4 096, brut et QCOW2 | 4 publications × 1 OUT de huit secteurs filaires | 8 dans chaque cas | 2/2 |

La géométrie du disque système QCOW2 publié a également été mesurée avec les paramètres implicites de l’application, à 512/512. Les deux autres propriétés physiques pertinentes sont la taille du cluster du fichier image (65 536) et la taille adressable du SSD (4 096). Aucune des trois mesures ne fournit la taille d’écriture indivisible en cas de panne. Le firmware sur disque système utilise le domaine 2 du SID 56, puis NovaOS le domaine 1 du même SID. Les quatre relectures se terminent sans écriture UEFI, sans faute IOMMU et sans plage DMA encore active. [Relevés QMP, empreintes et démarrages système](validation/v0.9.13/atomicite-utm.json)

### Images ambiguës et régressions

Huit comparaisons utilisent des volumes effectivement relus après la réponse de succès de la sonde. Sur une **copie en mémoire** de chacun, le banc rétablit les 48 anciens octets de la référence au manifeste. Les **2 097 152 octets** ainsi obtenus sont identiques au volume relu après la mise à jour du préfixe de 96 octets avant acquittement. La comparaison couvre les deux slots, les deux tailles de secteur et les deux formats. Ce second défaut est imposé après confirmation pour tester l’ambiguïté d’historique. Les arrêts forcés seuls après confirmation ont conservé le nouvel état dans cette campagne. Pour les images équivoques, le refus normal empêche de revenir silencieusement à une génération antérieure. Aucun montage réussi n’a rendu une destination absente ou un mélange de générations.

| Suite rejouée sur cette révision | ARM64 natif pour NovaFS, invité AArch64 pour NovaOS | x86_64 sous Rosetta pour NovaFS, invité x86_64 pour NovaOS |
| --- | ---: | ---: |
| NovaFS standard / instrumenté, zéro échec | 196 / 197 | 196 / 197 |
| Opérations différentielles, 100 graines | 1 000 000 | 1 000 000 |
| Injections historiques de plages et de coupures | 2 304 | 2 304 |
| Résultats ancien / nouveau / refus dans ces injections | 1 288 / 656 / 360 | 1 288 / 656 / 360 |
| Volumes issus des VM contrôlés en RO et RW | 142 | 142 |
| NovaOS démarrages / groupes bloc / ELF-processus-IPC / CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| DMA/GICv3 / DMA séquentiel | 11 / 23 | Profil AArch64 uniquement |

Les huit coupures historiques supplémentaires à deux blocs et les quatre paires d’historiques du simulateur ont également été rejouées par architecture. Les cinq tests Cargo optionnels sont exécutés séparément, y compris le nouveau banc d’échange d’images et le corpus liblz4 (256 pages dans chaque sens). `sysctl.proc_translated` vaut 1 dans le processus x86_64. `qemu-img` 11.1.3 sert à préparer, vérifier et convertir les fichiers hors ligne. Toutes les coupures VirtIO de la matrice proviennent du moteur 10.0.12 d’UTM. [Résultats de régression](validation/v0.9.13/atomicite-utm.json)

### Décision architecturale

**L’atomicité requise pour livrer le remplacement demeure non qualifiée.** Le contrat VirtIO permet un état persistant indéfini tant que l’écriture reste volatile. Aucun contrat identifié pour le fichier brut ou QCOW2 sur ce SSD via APFS ne resserre cette fenêtre à deux états possibles. Une garantie suffisante devrait couvrir tous les octets variables à leur adresse réelle, la protection de l’autre slot et des graphes engagés, ainsi que la durabilité de F1 et F2 de bout en bout. Passer à 4 096 octets logiques réduit le nombre de requêtes sans apporter cette garantie.

L’arbitrage proposé porte d’abord sur l’étude d’un **témoin de publication authentifié, durable, indépendant et résistant au recul** si la compatibilité avec les images P9 existantes est prioritaire. Il faudrait définir l’identité FS/LV et de lignée, l’ordre de persistance des deux supports et l’instant où le client peut recevoir le succès. L’autre voie est l’étude d’un **protocole de publication révisé**, capable de prouver quelle génération a été acquittée dans le modèle de panne retenu. Elle demanderait un arbitrage sur la compatibilité et la migration avant toute modification du format. Ajouter une copie de l’ancre seule ne résout pas l’identité des deux historiques. Le service NovaOS v0.15.0 garde le refus de remplacement et le refus de montage ambigu de NovaFS v0.9.13 P9.

### Rejouer la qualification sur le Mac UTM

Depuis la racine du dépôt, avec l’archive v0.15.0, UTM 5.0.8, les outils de construction du projet, les deux cibles Rust Darwin et Rosetta. La sortie doit être un nouveau répertoire sur le stockage à mesurer. Le banc n’accepte aucun chemin de disque cible fourni par l’opérateur.

```sh
mkdir -p novaOS/build
audit=$(mktemp -d "$PWD/novaOS/build/atomicite-utm-XXXXXXXX")
(
    cd novaOS
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
        --offline --locked --release --target aarch64-unknown-none-softfloat \
        --features ancres-p9-lab --target-dir "$audit/noyau"
)
python3 -B novaOS/tests/atomicite_ancres.py \
    "$audit/noyau/aarch64-unknown-none-softfloat/release/novaos" \
    "$audit/campagne" --paralleles 4
```

`coupures.tsv` consigne chaque point d’arrêt, les complétions et le SHA-256 du LV. `resultats.json` conserve les géométries QMP, les comparaisons d’historiques et les quatre parcours NovaShell. Les journaux série, traces VirtIO et volumes de relecture restent dans le répertoire d’essai. Les commandes de régression figurent dans la section [Reproduction](#reproduction).
