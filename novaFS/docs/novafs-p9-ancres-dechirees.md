# NovaFS v0.9.13 P9 : récupération après déchirure d’une ancre

**Audit du protocole de publication, base `729c5b0`.** Un journal sur le même support peut retrouver la génération publiée malgré la perte d’une copie. Si ses deux copies disparaissent après acquittement, le support entier peut redevenir identique à celui d’une transaction seulement préparée. La [comparaison des protocoles et l’arbitrage](#révision-du-protocole-de-publication) donnent les hypothèses, les mesures et la limite de compatibilité avec P9.

**Qualification UTM 5.0.8 du 12 novembre 2026.** L’atomicité de l’ancre lors d’une perte d’alimentation reste non qualifiée. Les 96 essais au placement GPT installé ont tous rendu un état complet. Le contrat VirtIO laisse indéfini le contenu persistant d’une écriture encore volatile. Les résultats, les limites du stockage hôte et la décision à prendre figurent dans l’[audit UTM](#atomicité-des-ancres-sur-le-stockage-utm).

**Décision de qualification : après déchirure d’une ancre, la reprise automatique du volume P9 autonome ne peut pas être autorisée sous les garanties demandées.** Une génération dont tous les blocs passent les contrôles peut être antérieure au dernier remplacement confirmé. Deux historiques qui exigent des décisions différentes produisent exactement la même image. Le refus de montage reste une limite de disponibilité, pas un résultat acceptable pour livrer le remplacement.

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

L’audit UTM proposait soit l’étude d’un **témoin de publication authentifié, durable, indépendant et résistant au recul**, soit celle d’un **protocole de publication révisé**. L’arbitrage suivant a retenu la seconde étude. Aucune migration ni modification de format n’a été autorisée. [Résultat de l’étude du protocole](#révision-du-protocole-de-publication)

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

## Révision du protocole de publication

Base `729c5b0b9b6d65a3d1ad4755a6531f483f5bcf13`, branche `main` propre au départ. L’étude porte sur le remplacement privé d’une destination existante. La publication d’une copie déjà préparée et le déplacement utilisent la même transaction. Le retour de succès est distingué de la réponse affirmative à une invite de NovaShell, qui autorise seulement une opération.

### Ce qui engage une transaction P9

| Étape | Action du moteur et conséquence d’une interruption |
| --- | --- |
| Préparation en RAM | `prepare` contrôle le mode, la santé du moteur et le débordement de génération. Il duplique le manifeste et l’état d’allocation, fixe la génération et le digest parents, puis vérifie identités et noms source/destination. Le nom cible est réaffecté dans le plan privé. L’ancien objet avec extents entre en collecte différée. Noms, parents, dates et comptabilité sont inclus dans ce même plan. Un refus à cette étape ne publie rien. |
| Écriture CoW et manifeste | `Tx::finish` réserve les pages et produit les racines, l’arbre d’espace et le manifeste. `publish` écrit les pages privées dans l’ordre des adresses de la `BTreeMap` et attend chaque complétion. Le manifeste peut donc être physiquement écrit avant une autre page du plan. Sa présence, même si son SHA-256 est valide, ne décide aucune publication. Les adresses des deux graphes protégés restent réservées. |
| F1 | Le succès de la première synchronisation couvre les écritures du plan qui la précèdent, sous le contrat du support. Les dépendances peuvent toutes être durables alors que la transaction reste **seulement préparée**. Une coupure avant toute soumission d’ancre donne précisément cet exemple. |
| Publication | Le moteur écrit un bloc d’ancre dans le slot de parité `(g+1) % 2`. Une transaction est **publiée pour la relecture** lorsque les octets persistants désignent le nouveau manifeste et que les graphes passent les contrôles du montage. C’est possible avant F2. Entre la soumission de l’ancre et la stabilisation de F2, les octets persistants peuvent donner l’ancienne paire, la nouvelle, ou une enveloppe déchirée que P9 refuse. |
| Durabilité et acquittement | Le succès de F2 atteste la **durabilité** dans le domaine de panne que couvre effectivement le support. Aucun FLUSH ne prouve l’indivisibilité de l’écriture en cours. Ensuite le moteur invalide son cache concerné, installe la tête et l’allocation, retrouve l’état sain puis rend le résultat de `transact`. La transaction est **confirmée à l’appelant** quand il reçoit ce succès. Une perte de la réponse après F2 laisse une transaction durable dont cet appelant ignore l’issue. |
| Erreur ou remontage | Une erreur sur les écritures ou F1 interdit de réutiliser les adresses dans l’instance. Une erreur sur l’ancre ou F2 est une erreur de publication à issue indéterminée. Le montage normal relit les deux ancres, leur filiation exacte et les graphes, y compris l’espace, les données et les références partagées. En RW, le flush d’activation vient seulement après ces contrôles. Une paire ambiguë entraîne un refus. Le sauvetage explicite d’un slot vérifie un état extractible en RO. Il ne certifie pas sa fraîcheur. |

Le contrôle des deux graphes rend une destination absente ou mixte observable dans le banc. La sûreté visée est plus exigeante : tout remontage présenté comme normal doit restituer un préfixe d’opérations qui inclut **tous** les succès déjà livrés. Il peut inclure une opération dont la réponse a été perdue. Un repli vers des fichiers cohérents peut enfreindre cette propriété. [Préparation, publication et montage](../src/engine.rs) · [Contrôles du remplacement](../src/fs/remplacement.rs) · [Réponse du service distribué](../../novaOS/user/novafs/files.rs) · [Contrat de reprise](novafs-recovery.md)

### Modèle de défaillance et limite d’observation

| Hypothèse | Propriété que l’on peut vérifier |
| --- | --- |
| Coupure avec barrières fidèles | Un seul écrivain. Les écritures en vol peuvent persister en partie, dans tout ordre ou pas du tout. Une barrière achevée conserve ses écritures dans le domaine annoncé. Une nouvelle écriture reste confinée à son adresse et préserve les blocs précédemment stabilisés ailleurs. Aucune hypothèse de secteur atomique. Une décision entière retrouvée après F1 peut désigner des dépendances complètes même si la réponse a été perdue. |
| Corruption localisée après un succès | Pour la tolérance à une copie perdue, au moins une copie complète de chaque décision requise, sa chaîne et les blocs nécessaires au graphe doivent survivre. Les vérifications détectent les altérations accidentelles sous l’hypothèse usuelle sur SHA-256. Si une dépendance indispensable échoue, il faut refuser. La séparation de deux adresses logiques n’établit pas deux domaines de panne physiques. |
| Panne commune, FLUSH mensonger ou retour à une image antérieure | Les deux copies peuvent revenir ensemble à leur état vierge ou un support complet à une ancienne image valide. Un SHA-256, un MAC correct, un numéro de génération et une chaîne de parents peuvent tous rester cohérents dans cette image. Aucune garantie de fraîcheur absolue ne découle de ses seuls octets. Le chemin UTM n’a pas démontré la persistance après perte électrique du Mac. |

Soit `I` un même état persistant complet. Si une exécution s’arrête après préparation et qu’une autre confirme la transaction puis perd les seules traces de la décision, un lecteur qui reçoit `I` et la même identité a les mêmes entrées. Rendre l’ancienne génération est correct pour la première exécution et perd un succès dans la seconde. Rendre la nouvelle peut promouvoir une transaction jamais publiée. Refuser les deux évite cette sélection mais sacrifie la disponibilité, même pour une image qui ressemble à un ancien état sain. Indépendamment, deux exécutions identiques jusqu’à F2 peuvent différer uniquement par la livraison de la réponse. Le disque ne peut pas attester la réception d’un message.

### Comparaison des stratégies

La colonne « décision de reprise » indique ce qui autoriserait réellement le choix. Les propriétés conditionnelles concernent une coupure confinée avec barrières fidèles. Elles ne valent pas pour une restauration complète du support.

| Stratégie | Écritures partielles, ordre et décision de reprise | Compatibilité |
| --- | --- | --- |
| Double ancrage avec génération | P9 conserve deux générations adjacentes. F1 protège les dépendances. Une ancre hybride empêche le montage normal. Choisir seulement le plus grand numéro lisible masquerait une perte après acquittement. Deux publications P9 successives du même contenu donneraient deux reçus distincts. Le survivant seul ne prouve ni le second succès ni l’emploi de cette règle par les anciens écrivains. | P9 actuel pour l’alternance. Une duplication identique d’une génération supérieure à 1 dans les deux slots viole la parité. Une transaction vide est élidée par `prepare`. |
| Journal de publication en ajout seul | Écrire après F1, à une adresse jamais réutilisée. Suivre les commits entiers chaînés, contrôler le graphe et refuser une lacune suivie d’un commit. Ignorer une queue endommagée n’est sûr que si la perte d’une décision déjà confirmée est exclue. La collecte et la bascule de journal ont besoin de leur propre protocole. | Les enregistrements et leur localisation exigent un profil identifié. Un lecteur P9 existant n’en connaît pas l’autorité. |
| Enregistrements de commit redondants | Deux copies après F1 dans des blocs distincts, succès seulement après les deux écritures et F2. Une copie valide peut suffire en RO si la chaîne et les deux graphes sont valides. Deux copies valides divergentes ou des fragments sans copie valide provoquent un refus. La disparition conjointe des copies reste indiscernable d’une queue vierge. | Prototypé sur support synthétique. Le pool dispose déjà de certificats redondants pour son propre catalogue, à une autre couche. |
| Publication en plusieurs phases | Stabiliser une intention, puis écrire une décision qui la lie par digest, puis la stabiliser avant réponse. Une intention complète seule ne publie rien. Avec décision partielle ou discordance, refuser ou vérifier une copie indépendante de cette décision. Ajouter un marqueur « réponse envoyée » déplace la dernière fenêtre de panne et ne constate pas la livraison. | La distinction préparation/décision existe déjà dans P9. Une intention ou un état d’acquittement persistant supplémentaire exige une représentation et des règles de reprise nouvelles. |
| Vérification croisée des manifestes | Vérifier génération, digest du parent, racines et propriété avant exposition refuse les mélanges et les dépendances réordonnées manquantes. Un couple de manifestes complètement préparés peut néanmoins n’avoir jamais été publié. Interdire la promotion d’un manifeste trouvé par balayage. | Le contrôle de filiation existe en P9. Il se combine avec un sélecteur de publication, sans le remplacer. |
| Conservation explicite des générations | Épingler les racines et les blocs accessibles avant toute réutilisation. Une ancienne vue reste lisible après déchirure si ses données survivent. Purger après la barrière et la levée des références de toutes les vues. Le plus récent état récupérable peut précéder le dernier succès. | Deux graphes sont déjà contrôlés en P9. Une rétention persistante plus longue sur LV autonome nécessite des racines et une comptabilité reconnues par tous les écrivains. |
| Marqueurs d’intégrité authentifiés | Un MAC lié à l’identité, à la lignée, à l’adresse, au parent et à la racine permet de refuser une substitution, sous réserve de la clé et de la vérification du graphe. F1 puis F2 restent nécessaires. Une ancienne décision authentique peut être rejouée avec toute sa chaîne. | Le SHA-256 autonome n’est pas un MAC. Le pool chiffre les unités et authentifie son catalogue avec ses propres clés. L’ajout d’un MAC au LV autonome demanderait un profil de clés et une négociation. |
| Reprise en lecture seule avant toute écriture | Vérifier éligibilité, paire et données sans modifier le support. Un diagnostic peut signaler les copies dégradées et permettre une extraction. Sans décision suffisamment étayée, la seule issue sûre est le refus. Une réparation suivie d’un contrôle effacerait les preuves nécessaires. | Le contrôle P9 et le sauvetage explicite existent. La vue synthétique du prototype présente deux ancres en RAM. Aucun de ces chemins ne donne de nouveaux droits d’écriture. |

Pour comparer les coûts, un bloc vaut 4 Kio, `J` est le nombre de décisions conservées et `K` le nombre de générations épinglées. Les ordres de grandeur ci-dessous sont des coûts du protocole en plus des données CoW, hors caches, staging et bitmaps du vérificateur. Ce ne sont pas des mesures de temps du SSD.

| Stratégie | Mémoire de travail | Espace disque et latence | Snapshots, clones et chiffrement |
| --- | --- | --- | --- |
| Double ancrage | Un tampon d’ancre et deux références, O(1). | Deux blocs fixes. Une écriture d’ancre et F1/F2 par mutation. Une seconde publication réelle répète le cycle et consomme aussi de nouveaux blocs CoW. | Préserver les deux graphes au retrait. Un clone bit à bit copie les deux ancres. L’ancre autonome assure une intégrité sans clé. |
| Journal en ajout seul | Un bloc de lecture et la tête de chaîne, O(1) en parcours séquentiel. | Au moins un bloc par décision dans cette hypothèse de disposition, O(J) jusqu’au checkpoint. Une écriture après F1, puis F2. Remontage en O(J) lectures. | Une branche doit avoir sa propre lignée dès la divergence. La rétention du journal ne réserve pas les données d’un snapshot. L’historique des racines peut révéler l’activité d’un volume chiffré. |
| Commits redondants | Deux références de 48 octets par décision. Décodage en flux avec un nombre constant de tampons de bloc. | Deux blocs par décision, deux écritures après F1, un F2 commun. Le prototype réserve six blocs (24 Kio) pour trois paires. Pour le remplacement mesuré, sept écritures de 4 Kio contre six en P9, avec deux barrières dans les deux cas. | Un snapshot doit figer la décision **et** les graphes. Les clones doivent séparer leurs publications futures. Les deux copies du banc sont hachées. Une extension au pool exigerait une authentification conforme à sa lignée. |
| Plusieurs phases | Une référence et un état par transaction en vol. | Une intention distincte coûte au moins un enregistrement supplémentaire. Si elle a sa propre phase stable, trois barrières au lieu de deux. Une intention groupée avec les dépendances conserve deux barrières. | Le snapshot retient la dernière décision visible. Un clone ne reprend pas une intention de sa branche mère. Lier cryptographiquement intention, décision et contexte de branche si le volume est scellé. |
| Manifestes croisés | Deux références et les tampons des manifestes. Le contrôle complet garde aussi la comptabilité des graphes. | Le lien parent existe déjà. Aucune écriture ni barrière supplémentaire en P9. Le remontage lit les deux manifestes et les blocs accessibles. | Vérifier les blocs partagés depuis les deux vues. Pour du chiffré, utiliser la référence de l’instance scellée et sa classe, y compris pour une page compressée. |
| Générations conservées | O(K) références, plus l’état de comptage ou de marquage des blocs. | Les blocs divergents restent occupés tant qu’une vue les cite. La consommation peut atteindre une copie complète par génération divergente. La collecte augmente le travail différé. | Le snapshot épingle, le clone peut diverger. Garder aussi tables de compression, domaines et clés indispensables aux vues retenues. |
| Marqueurs authentifiés | État du MAC et clés par domaine. | HMAC-SHA-256 ajoute 32 octets par attestation avant alignement, deux attestations dans le pool. Coût du calcul et de la vérification des clés, sans supprimer de barrière. | Lier vue, type, propriétaire et filiation. Le MAC de publication n’emploie pas de nonce AEAD. Toute nouvelle unité chiffrée continue d’exiger un nonce approprié sous sa clé. |
| Reprise RO | Tampons de lecture et mémoire de vérification du graphe. La vue du banc synthétise un bloc d’ancre à la demande. | Zéro écriture, zéro FLUSH en RO. Le coût de démarrage est celui du sélecteur et des graphes parcourus. | Une vue RO ne collecte aucun bloc et ne change aucun domaine. Vérifier les clés et le graphe avant toute ouverture inscriptible ou réparation. |

SQLite décrit une décision de commit dans une trame valide de son WAL et un checkpoint distinct. Le journal JBD2 décrit le descripteur, les données et le commit vérifié avant rejeu. Ces mécanismes éclairent la distinction entre préparation, décision et recyclage. Leurs garanties ne sont pas transférées au stockage UTM. [Format du WAL SQLite](https://sqlite.org/fileformat2.html#the_write_ahead_log) · [Journal JBD2](https://docs.kernel.org/filesystems/ext4/journal.html) · [Publication authentifiée du pool NovaFS](novafs-p7-chiffrement.md#publication-perte-de-fraîcheur-et-traitement-des-erreurs)

### Prototype sur un seul support simulé

Le modèle utilise exclusivement des images neuves en RAM. Sur **un même** `SimDevice`, il place un volume de laboratoire de 512 blocs, puis six blocs expérimentaux. Trois paires accueillent le point de départ et au plus deux nouvelles décisions. Les copies sont à des adresses distinctes du même support. Chaque enregistrement lie l’identité FS/LV, la taille du LV, son adresse, la référence au parent et celle au nouveau manifeste. Un SHA-256 situé dans les 32 derniers octets couvre le reste du bloc et un domaine dédié. Les octets réservés doivent être nuls. Ce contrôle vise les corruptions accidentelles. Aucune clé, aucun nonce et aucun témoin hors support ne sont nécessaires au lecteur expérimental. [Écrivain et lecteur privés au test](../src/fs/remplacement/protocole.rs)

1. Le banc extrait les cinq pages CoW réellement écrites par le remplacement P9 sur une fixture indépendante, dont le manifeste. L’écrivain vérifie la tête et l’absence de dégradation, écrit ces pages, attend F1, puis écrit les deux décisions à des adresses vierges. Il attend F2 avant le succès. Il refuse de réutiliser un enregistrement, y compris après dégradation ou lorsque le journal est plein.
2. À froid, le lecteur balaie les trois paires. Une paire entière vide appartient à la queue libre. Un commit plus loin après un trou, une rupture entre la référence parente et la tête précédente, ou deux décisions valides divergentes entraînent un refus. Une seule décision exacte suffit en RO avec signalement de la copie manquante ou endommagée. Des octets non nuls sans décision entière imposent le refus, même avant tout succès client.
3. Le lecteur synthétise en RAM les deux ancres correspondant à la décision retenue et présente une vue RO au montage normal P9. Celui-ci valide les graphes parent et courant. Un graphe incomplet provoque un refus sans essai automatique d’une génération plus ancienne. Les ancres P9 physiques de la fixture servent au diagnostic de leur enveloppe. Les traces et les reçus attendus ne sont lus que par les assertions du banc, jamais par le lecteur. La vérification ne modifie aucun bloc et n’émet aucun FLUSH.

**Compatibilité mesurée.** Le lecteur expérimental refuse le P9 ordinaire sans journal. Le moteur P9 distribué refuse le support synthétique pris en entier, dont la taille ne correspond pas aux ancres. Si on lui présente seulement le LV à sa taille P9 correcte après un succès journalisé, il monte, vérifie les deux graphes et restitue **l’ancienne destination**. Ce dernier résultat exclut un déploiement transparent par simple changement d’écrivain. Un futur profil aurait besoin d’une identification que tous les anciens lecteurs et écrivains refusent de manière sûre. La vue de reprise du banc est en lecture seule ; l’écrivain de test peut ajouter une décision sur un support sain. Aucune migration n’est définie.

### Résultats expérimentaux

Les mesures ci-dessous sont identiques sur ARM64 natif et x86_64 sous Rosetta. Elles comptent une fois chaque cas **par architecture**. Les deux opérations emploient les deux parités de génération. Le simulateur permet de déchirer un octet à l’intérieur d’un secteur déclaré de 4 096 octets.

| Coupures du journal par géométrie | Ancienne destination vérifiée | Nouvelle destination vérifiée | Refus | Total |
| --- | ---: | ---: | ---: | ---: |
| Secteurs de 512 octets | 3 624 | 892 | 1 528 | 6 044 |
| Secteurs de 4 096 octets | 3 120 | 388 | 504 | 4 012 |
| **Total** | **6 744** | **1 280** | **2 032** | **10 056** |

Pour chaque opération et géométrie, 27 événements couvrent les trois étapes de chacune des sept écritures et des deux FLUSH. L’ordre normal puis l’ordre inverse des pages et des copies donnent 54 événements parcourus. À chacun, le banc injecte trois politiques de persistance et 17 plages par copie, dont des préfixes de 1, 32, 64, 96, 128, 512 et 4 096 octets, des plages intérieures et des déchirures du checksum final. Il énumère aussi tous les masques de secteurs d’une copie à la soumission de F2. Avant la fin de F1, tous les remontages restent anciens. Dès l’effet stable de F2, tous retrouvent la nouvelle destination. Les 2 032 refus correspondent à du contenu de décision partiel sans copie valide. Ils constituent une **limite de disponibilité après coupure ordinaire**, même sans perte de succès confirmé.

| Autres contrôles, hors des 10 056 coupures | Résultat par architecture |
| --- | --- |
| Réordonnancements forcés à travers F1 | 32 sous-ensembles des cinq pages CoW × 4 combinaisons de copies × 4 fixtures, soit 512 états. 128 anciens sans décision, 12 nouveaux complets, 372 refus lorsque la décision désigne un graphe incomplet. Aucune promotion de manifeste orphelin. |
| Défauts imposés après succès | 236 cas. Une copie abîmée ou effacée laisse le nouveau reçu vérifiable en RO avec dégradation signalée. Les enveloppes d’ancre A, B et A+B peuvent être endommagées sans perte des références conservées dans le journal. Deux décisions abîmées, une dépendance indispensable corrompue, une filiation rompue ou deux branches légitimes concurrentes sont refusées dans les cas correspondants. |
| Erreurs de complétion | 32 erreurs avant F2, dont des écritures en échec pouvant avoir persisté jusqu’à une page entière. Les 12 erreurs de F2 stabilisent zéro, une ou deux copies. Aucun de ces appels ne rend de succès. Le montage peut déjà vérifier le nouveau contenu si une décision entière a persisté. |
| Publication suivante et rétention | Une deuxième mutation distincte est publiée et relue dans chaque fixture. La source et la destination du remplacement restent vérifiées. La reprise dégradée empêche l’ajout. Une fois les trois paires occupées, l’écrivain rend `NoSpace` avant toute écriture. Une lacune au milieu est refusée même si les données suivantes sont lisibles. Aucune rotation ou collecte du journal n’a été qualifiée. |

**Contre-exemple après confirmation.** Sur une branche, couper après F1, avant la première décision. Sur une autre, attendre un vrai succès de F2 et de l’appel, puis effacer uniquement les deux copies de cette décision. Les **2 121 728 octets du support, journal et données compris**, sont identiques. Le même résultat survient si F2 annonce faussement un succès et que la coupure perd les deux écritures encore volatiles. Le lecteur rend l’ancienne génération sans anomalie décelable dans ces trois images. En revanche, une coupure à la complétion effective de F2, juste avant que le succès ne soit livré, donne exactement l’image de l’appel réussi et la **nouvelle** génération. Ces deux égalités ont été établies par comparaison intégrale pour chaque fixture. Elles séparent la perte d’une confirmation durable de la seule perte de la réponse. [Injections et comparaison octet par octet](../src/fs/remplacement/protocole/injections.rs) · [Effectifs et SHA-256 des supports](validation/v0.9.13/protocole-publication.json)

### Régressions et périmètre cryptographique

| Contrôles | ARM64 | x86_64 |
| --- | ---: | ---: |
| NovaFS standard / instrumenté, zéro échec | 196 / 197 | 196 / 197 |
| Injections historiques sur les deux ancres, ancien / nouveau / refus | 1 288 / 656 / 360 | 1 288 / 656 / 360 |
| Différentiel, opérations / remontages et audits | 1 000 000 / 1 000 | 1 000 000 / 1 000 |
| Interopérabilité LZ4, pages dans chaque sens | 256 / 256 | 256 / 256 |
| NovaOS, démarrages / groupes bloc / ELF-processus-IPC / CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| UTM 5.0.8, QEMU 10.0.12, DMA/GICv3 / DMA séquentiel | 11 / 23 | Matériel émulé AArch64 uniquement |

Les 2 304 injections historiques par architecture s’ajoutent à celles du prototype. Les tests optionnels comprennent le nouveau test isolé et la relecture sur chaque architecture des 142 volumes issus du banc UTM historique. Cette matrice a été rejouée sur des images neuves. Les 142 résultats et leurs empreintes sont identiques à la matrice archivée, et les quatre systèmes brut/QCOW2 ont relu le fichier acquitté après redémarrage. **La campagne UTM exerce le protocole distribué et le banc d’ancres historique. Le nouveau journal a été testé exclusivement dans le simulateur.** [Matrice historique](validation/v0.9.13/atomicite-utm.tsv)

Sur le pool chiffré existant, douze injections par architecture vérifient que l’origine suit la publication, que le snapshot conserve l’ancienne destination et que le clone conserve sa divergence. Les mauvaises clés sont refusées. Les suites de compression, de chiffrement et de migration/rechiffrement P9 ont aussi été rejouées : deux branches d’un même état avec la même clé produisent des nonces distincts dans les unités observées, la migration sépare les domaines et le test de scellement interrompu vérifie le comportement à la reprise. Ces résultats portent sur le pool actuel et son générateur d’entropie. Le journal simulé n’émet aucune unité chiffrée. Son intégration au pool, le cycle de vie des clés et la conservation des racines de snapshots restent à spécifier. [Pool, snapshots et certificats](../src/secure_pool/qualification_ancres.rs) · [Clones, compression et nonces](../tests/p7_secure.rs) · [Coupures, migration et rechiffrement](../tests/p9_pannes.rs)

Pour reproduire le prototype, depuis la racine du dépôt :

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib \
        fs::remplacement::protocole::injections::journal_sur_un_seul_support \
        -- --ignored --exact --nocapture
done
```

Chaque ligne `PROTOCOLE` contient les mesures d’une fixture. Les procédures [de régression](#reproduction) et [du banc UTM](#rejouer-la-qualification-sur-le-mac-utm) complètent cette commande.

### Arbitrage sur les garanties de publication

**Arrêt avant intégration.** La redondance interne améliore la reprise après un défaut localisé sans exiger l’atomicité d’un secteur. Elle ne satisfait ni la disponibilité systématique devant une décision déchirée, ni la détection d’un recul commun des copies après confirmation. Le lecteur P9 actuel ne reconnaîtrait pas l’autorité du nouveau journal. NovaOS reste en v0.15.0 avec le remplacement désactivé et NovaFS en v0.9.13 P9.

Si la détection de toute perte confirmée reste impérative, conserver le remplacement désactivé. Le refus P9 protège contre les ambiguïtés détectables ; il ne détecte pas la restauration intégrale d’une ancienne image valide. Si un modèle de panne borné est acceptable, l’étape proposée est **une spécification**, soumise à arbitrage, d’un profil distinct : contrat de persistance de F1/F2, étendue maximale d’un dommage, traitement RO des traces partielles, refus par les anciens lecteurs, règles de rétention et de recyclage, puis politique des branches et du pool chiffré. L’acceptation de ces hypothèses et la compatibilité du futur profil sont deux décisions distinctes. Toute modification du format exige une autorisation ultérieure.

## Qualification du protocole conditionnel sur VirtIO

Le 12 novembre 2026, le profil de publication distinct de P9 a été exercé sur les quatre couples brut/QCOW2 et 512/4 096 dans le moteur d’UTM 5.0.8. Les 24 reçus des essais PC1 ont été retrouvés sur l’image rouverte. L’application UTM a produit un reçu supplémentaire sur QCOW2/512, vérifié après arrêt et export. Une coupure après un seul secteur de A laisse une intention sans décision valide en 512, et une décision A complète en 4 096. Dans les deux cas, l’appelant n’avait encore reçu aucun succès.

La matrice simulée montre qu’une intention survivante permet de signaler la perte des deux décisions après confirmation. Le retour de tout le support à une ancienne image authentique laisse les deux historiques impossibles à distinguer. Les 116 acquisitions réelles ont été relues sur ARM64 et x86_64. La machine est restée alimentée et aucun banc dédié n’était disponible pour R02. Ces résultats maintiennent la limite de persistance physique déjà identifiée pour le SSD et le backend Darwin. La recommandation demeure de conserver NovaFS v0.9.13 P9 et NovaOS v0.15.0 avec le remplacement désactivé. [Qualification, coûts et conditions de reprise](novafs-publication-conditionnelle.md#10-qualification-utm-du-12-novembre-2026) · [Matrice des démarrages](validation/v0.9.13/publication-conditionnelle-utm.tsv)
