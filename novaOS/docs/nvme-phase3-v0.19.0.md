# NovaOS v0.19.0 — Qualification finale du NVMe virtuel

Le profil `nvme-lab` relie le client de fichiers, le service NovaFS P9, le service bloc et le pilote NVMe isolé en Ring 3 au contrôleur Q35 par le courtier et Intel VT-d. Les essais concernent un namespace jetable sur QEMU x86_64 11.1.3, avec au plus une commande en vol. Les constructions ordinaires utilisent VirtIO-blk et le polling. Les capacités privées autorisant READ, WRITE, FLUSH et la gestion du contrôleur ont des titulaires et des portées distincts. [Contrat matériel](nvme-phase2-contrats.md) · [Contrat du service bloc](nvme-phase2-service-bloc.md)

## Cycle d’une commande et décision de restitution

| État observé | Autorité et effet |
|---|---|
| Réservation et préparation | Le bloc présente un droit pour le namespace, l’intervalle de LBA et la génération. STAGE fige les octets d’un ticket WRITE dans le courtier. Une annulation dans cet état ferme la réservation sans publier d’entrée matérielle. |
| Autorisation effective et soumission | Sous le verrou commun de l’ordonnanceur et de l’IPC, le courtier revérifie les titulaires, les capacités et le ticket. Il prépare la page DMA privée, inscrit la SQE puis signale la file au contrôleur. Cette dernière étape engage le matériel. |
| En cours | La révocation ferme l’admission logique. Une commande signalée peut encore lire la source, écrire sur le média ou mettre à jour une file de complétion. Ses cadres ont toujours le même propriétaire. |
| Terminée | Le courtier rapproche la phase, le CID, le SQID, la tête de file et le statut de la commande attendue et consulte VT-d. Le service bloc libère le ticket après ces contrôles. |
| Expirée ou annulée en vol | Le résultat reste en échec. En l’absence de preuve fiable de drainage, la fonction PCI est clôturée et le domaine conserve cadres et tables en quarantaine. Une CQE tardive ne rouvre pas la session. |
| Contrôleur arrêté et réinitialisé | Sur la voie sûre, les files sont supprimées, SHN/SHST acquitté et CC.EN désactivé. RDY, CC, CSTS, AQA, ASQ et ACQ sont vérifiés avant restitution. Le drainage autorise le retrait des traductions. |
| DMA retiré et cadres restitués | BME fermé, le contexte et les tables sont retirés. Le DRHD doit consommer les invalidations de contexte et d’IOTLB avec drainage, puis écrire le cookie WAIT attendu. Le courtier revérifie la clôture, les registres, l’absence de faute et le contexte vide. L’allocateur peut alors transférer la propriété des cadres. |

La phase 2 a montré l’effet concret d’une commande déjà engagée. Un refus VT-d sur la lecture de la source du WRITE a laissé 100 octets modifiés dans le seul bloc CoW candidat, numéro 2. Les six cadres demeuraient épinglés. Les deux ancres et le fichier confirmé restaient intacts. La modification du média relève de la transaction en préparation. Une réattribution mémoire a, séparément, besoin de la preuve de retrait ci-dessus. [Matrice de phase 2](validation/nvme-phase2-qemu.json) · [Preuve de drainage](../kernel/src/hardware/nvme/mod.rs) · [Retrait VT-d](../kernel/src/hardware/vtd_lab.rs)

## Erreurs et publication

Le contrôle des capacités précède la doorbell. Le droit ou le namespace incorrect donne DENIED (5), une plage ou une taille invalide BAD_ARGUMENT (3), une adresse utilisateur invalide BAD_ADDRESS (4). Ces refus surviennent en l’absence d’un CID matériel correspondant. Un timeout donne TIMED_OUT (8). Une faute VT-d invalide la livraison même si le périphérique produit un CQE de réussite. Le pilote et le service bloc transportent le CID et le statut NVMe exact lorsqu’une complétion en erreur existe. [Admission du bloc](../user/nvme_rw_block.rs) · [Code du diagnostic](../user/nvme_diagnostic.rs)

Pour une modification de fichier, NovaFS termine les WRITE des dépendances, attend F1, publie l’ancre, attend F2 puis confirme au client. Une erreur WRITE ou F1 du backend a produit IO (26). Une erreur F2 a produit UNCERTAIN (22), avec le CID et le statut natifs conservés jusqu’à l’application. Le client effectue ces échanges avec les opérations MOUNT, OPEN, WRITE, SYNC, READ et CLOSE de l’interface fichiers native. Les relevés suivent les quatre processus réels et le courtier. [Ordre de publication P9](../../novaFS/docs/novafs-recovery.md) · [Mesures par PID et par commande](nvme-phase2-v0.19.0.md)

## Preuves de phase 2 réutilisées

La revue part du commit de code `164a98677c9d9640a9f6f4e5aa86c8cbd1253793` et du commit de qualification `21a8553cf7babf2b80fd6d1b030ea7117c1bc5e7`. Ce dernier descend du `main` initial `386e53da7e9abb7ce1ef5e32bd64f5c667c74bb7`. Les journaux et les RAW ont été relus avec l’oracle de la révision qualifiée. Il reproduit le rapport archivé de SHA-256 `712117f2192aeee7386bb966a746b7da3dfb8e86f90b4814b55137ff9a38fe3e`. Ses 45 scénarios, 92 démarrages, 45 remontages RO, 16 coupures et 13 CQE tardives sont comptés au titre de la phase 2. [Qualification antérieure](nvme-phase2-v0.19.0.md) · [Relecture et compatibilité](validation/nvme-phase3-compatibilite.json)

Les preuves couvrent les secteurs de 512 et 4 096 octets, la révocation depuis un autre CPU, la mort du pilote et du client, la migration et le refus de tickets périmés à 1, 2 et 4 vCPU selon le scénario. Le refus QI avec IQE et le WAIT lu tardivement sur l’hôte ont tous deux conservé les six cadres. Une sauvegarde déjà confirmée avant ces deux échecs de fermeture reste lisible. Les résultats des anciennes campagnes ont leur propre identité de binaire. [Résultats QEMU NVMe](validation/nvme-phase2-qemu.json) · [Régressions de phase 2](validation/nvme-phase2-materiel.json)

## Essais de phase 3

Le code qualifié est le commit `9c2cd65f45ec1ef65ca3c1bea1afb2260854e6b3`. Les cinq scénarios ont utilisé douze démarrages QEMU (deux préparations, cinq opérations et cinq remontages RO). Le binaire `nvme-lab` testé porte le SHA-256 `5ac916e1088468aa14f73749a7eef1b902a4c0b7bec000d07b651cac89c30eb1`. Pour chaque scénario, le banc conserve la série, la trace du contrôleur, le RAW et les empreintes de la relecture dans une nouvelle VM. [Matrice et captures](validation/nvme-phase3-qemu.json)

| Scénario | vCPU / secteur | Résultat matériel et applicatif | Relecture |
|---|---|---|---|
| Retrait avant WRITE | 2 / 512 | CPU 1, ticket 73 préparé. SUBMIT_WRITE refusé avec DENIED (5), zéro WRITE, RAW identique et six cadres restitués après WAIT. | g3, ancien contenu |
| Retrait après WRITE | 4 / 4 096 | CPU 3, ticket 73 terminé matériellement. RELEASE refusé avec DENIED (5). Une WRITE a changé 4 080 octets du seul bloc CoW 2. F1 n’a pas été soumise. | g3, ancien contenu |
| Expiration pendant F1 | 2 / 4 096 | Six WRITE, 4 896 octets changés dans les six blocs de préparation. Le client reçoit TIMED_OUT (8). La CQE de succès tardive du CID 32 reste sans effet sur le statut. Les six cadres gardés en quarantaine conservent leurs empreintes aux deux prélèvements QMP. | g3, ancien contenu |
| Erreur NVMe pendant F2 | 2 / 512 | FLUSH CID 34, statut NVMe `0x0280`. Le bloc retourne IO (26), NovaFS et le client UNCERTAIN (22). Aucune confirmation de durabilité. | g4 complète et lisible |
| Réattribution, puis réouverture en lecture seule | 2 / 512 | Sept WRITE, F1 au CID 32, publication, F2 au CID 34, puis confirmation. Le second pilote effectue 21 READ sous son propre bail. | g4, 4 231 octets attendus |

Les cinq remontages en VM utilisent uniquement des droits de lecture et conservent le SHA-256 du disque entier. Le rapport a été régénéré depuis les traces, les RAW et les prélèvements de cadres. Son SHA-256 est `75af2d26d160124ea28c2fbe4c93962abd87ba594126c480d8123b06e33dd34c`. Les cinq tests unitaires de l’oracle réussissent, dont le refus d’un CID réutilisé sans réinitialisation à la frontière des deux incarnations.

Dans l’essai de réattribution, le superviseur reprend les six adresses physiques effectivement restituées par l’allocateur après le WAIT. Il remplit chaque page avec un motif distinct. La première incarnation du pilote (PID 3) et la suivante (PID 7) emploient le CID administratif 1 et les CID d’E/S 7 à 27, séparés par un arrêt et une réinitialisation attestés dans la trace matérielle. Le second bail utilise cinq autres pages physiques et une autre fenêtre d’IOVA. Les quatre espaces utilisateur ont passé l’audit de leurs tables avec 14 puis 19 cadres interdits, dont les six témoins lors de la seconde admission.

L’hôte a lu les 24 576 octets à trois instants. Entre les incarnations, le contexte était vide et la tête et la queue des invalidations valaient 144. Après les 21 READ, les cinq traductions restaient actives, BME ouvert, CC et CSTS à zéro et les deux index à 192. Avant libération, BME était fermé, le contexte vide et les deux index à 240. Les six motifs sont restés identiques. Après les six libérations individuelles, le noyau a vérifié zéro témoin et huit cadres permanents de service.

## Mise au point de la mesure de réattribution

Deux essais préparatoires ont échoué, chacun après la confirmation de la première transaction. Leurs RAW, séries et traces QEMU sont conservés sous `novaOS/build/` dans le répertoire de qualification.

| Lot | Signal observé | Cause vérifiée et correction de la sonde |
|---|---|---|
| `nvme-phase3-a4b507c7/reuse` | L’invariant mémoire a compté 241 cadres engagés et 235 attribués. | La sonde avait obtenu six cadres sans inscrire leurs propriétaires dans le bilan mémoire. Elle utilise désormais six allocations épinglées au nom du superviseur, compte les propriétaires réels et vérifie leur libération individuelle. Le contrôle global du noyau reste en vigueur. |
| `nvme-phase3-finale-69b185dd/reuse` | Le service NovaFS est sorti avec TIMED_OUT (8) et l’application avec CLOSED (11) avant l’audit des espaces du deuxième groupe. | La sonde suspendait le traitement du courtier trois secondes alors que le rendez-vous DISCOVER expire après une seconde. La fin d’un processus retire son espace. La lecture des tables a été déplacée entre les incarnations et après les dernières lectures. Le superviseur distingue désormais acteur terminé et projection interdite. |

Les deux RAW ont été remontés en lecture seule par l’oracle hôte. Tous deux restituent la génération 4, avec le fichier complet de SHA-256 `9d691dbec9d08e43fb398131d7e4a7f53c0086e2b9380a652f3ab68c558ad849`. Les SHA-256 des deux séries d’échec sont respectivement `b54a81939a992f2b7afec1ca82df2d164c12e0519fa46c2f355c47e517165073` et `46d6b39cc285fc97b0c7834f22abf7c81704ab8b7df3b7188b23e2a412ccab65`.

## Vérifications de compatibilité

Les constructions ordinaires ont démarré sous QEMU sur AArch64 et x86_64. Les deux bannières annoncent v0.19.0, les cinq comparaisons de framebuffer réussissent et les quatorze exécutables CLI qui exposent une version portent le même numéro sur les deux architectures. Le contrôleur compte 16 tests réussis en ARM64 natif et 16 sous Rosetta. Sur les deux architectures hôtes, quatre tests du format VT-d ont réussi. L’épuisement de la dernière époque autorisée (3 584) refuse un nouveau bail et empêche le recyclage d’une ancienne IOVA. Le codec du diagnostic compte deux tests réussis sur chaque architecture. Les dix profils historiques d’E/S groupées ont passé le contrôle d’application de leurs correctifs à zéro fuzz. [Relevé de compatibilité](validation/nvme-phase3-compatibilite.json)

Les arbres Git du moteur NovaFS et de l’ABI native correspondent respectivement à `3379ccd023e821e912130c7413113fe7546070cc` et `a70d5f23ae91211dd7a54c429e723f5157600e58`, comme sur le `main` v0.18.0. Le pilote bloc classique, les images distribuées et le code de publication P9 conservent leurs empreintes. Pour les 16 images de coupure couvrant les deux géométries, le lecteur a effectué deux extractions par image, soit 32 au total. Dans les 16 cas, la génération confirmée 3 restitue les six octets `ancien`. L’ancre courante restitue ces six octets dans 12 cas et les 4 231 octets complets de la génération 4 dans quatre cas. Les empreintes des 16 disques concordent avant et après extraction. [Oracle des deux générations](../tests/nvme_phase3_recuperation.py)

## Reproduction des essais complémentaires

Depuis `novaOS/`, avec Rust stable, QEMU 11.1.3, Limine, xorriso et les firmwares EDK2 de la phase 2. Le banc crée ses RAW sous un répertoire neuf. Les options `--point revoke-before`, `--point revoke-after`, `--point timeout-flush`, `--point eio-f2` et `--point reuse` permettent les essais individuels. Les captures d’un essai échoué sont conservées. [Banc ciblé](../tests/nvme_phase3.py)

```sh
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
  --locked --release --target x86_64-unknown-none --features nvme-lab --target-dir target/nvme-phase3
rustup run stable cargo build --manifest-path ../novaFS/Cargo.toml --locked --release \
  --bin novafs --target aarch64-apple-darwin --target-dir ../novaFS/target/phase2-oracle
python3 -B tests/nvme_phase3.py \
  --elf target/nvme-phase3/x86_64-unknown-none/release/novaos \
  --lecteur ../novaFS/target/phase2-oracle/aarch64-apple-darwin/release/novafs \
  --sortie build/nvme-phase3-essai
python3 -B tests/nvme_phase3.py rapport build/nvme-phase3-essai \
  --lecteur ../novaFS/target/phase2-oracle/aarch64-apple-darwin/release/novafs \
  --rapport build/nvme-phase3-essai/rapport-verifie.json
```

Pour contrôler à nouveau une campagne de phase 2 conservée avec ses RAW et son `bilan.json`, lancer `python3 -B tests/nvme_phase3_recuperation.py CHEMIN_CAMPAGNE --lecteur CHEMIN_DU_LECTEUR`. Chaque image est vérifiée contre l’empreinte publiée avant toute extraction.

## Limites et décision suivante

La qualification ciblée réutilise les régressions de phase 2. Elle ne comprend pas de nouvelle campagne d’endurance IPC, de benchmark, de million d’opérations différentielles NovaFS ou d’essai graphique UTM. Le stockage exercé est un RAW QEMU sur un Mac alimenté. La coupure physique, le déploiement sur SSD, l’atomicité des secteurs et l’amorçage autonome sur NVMe nécessitent leurs propres préconditions et essais. Aucun accès au SSD du Dynabook n’entre dans cette qualification. Le service conserve une seule commande en vol et un maximum de 4 Kio par transfert. Le contrat assume une perte de disponibilité du contrôleur jusqu’au prochain démarrage quand le retrait ne peut pas être confirmé. L’archive UTM distribuée reste celle qui a été qualifiée en v0.17.0. Pour v0.20.0, choisir par arbitrage MOA/MOE le prochain profil matériel et son modèle de panne avant toute extension des droits.
