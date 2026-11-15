# Stockage du journal des services sur NovaFS P9

## Autorité et chemin des données

Le démon de journalisation reçoit les événements, vérifie les droits des clients et construit l’identité de leur émetteur. Pour publier, il adresse une trame de 128 octets au service utilisateur NovaFS sur une paire IPC privée. NovaFS traite une requête à la fois sur le montage P9 déjà validé. L’ABI v1 de l’appel système et le format disque du moteur sont ceux du jalon précédent. [Service NovaFS](../user/novafs/files/journal.rs) · [Client du démon](../user/services/journal_backend.rs) · [Commandes nsh](journal-nsh-v0.22.0.md)

Les mots de configuration 16 et 17 de NovaFS contiennent les poignées de réception et de réponse. La révocation des racines du démon invalide cette paire. NovaFS relit les poignées à chaque passage et juste après réception. Il associe le numéro de requête à la paire observée et répond sur la poignée capturée. Un nouveau démon reçoit sa propre paire et recommence ses numéros de requête. Une opération déjà remise peut avoir abouti au moment de la révocation.

| Opération privée | Entrée | Résultat |
| --- | --- | --- |
| `STATUS` | Aucune | Première séquence conservée, suivante exclusive, segments, rétention, révision, quota et données utiles en octets |
| `READ` | Séquence | Enregistrement complet de 96 octets ou absence |
| `APPEND` | Prochaine séquence attendue et enregistrement complet | Séquence après commit ou acquit de doublon |
| `ROTATE`, `PURGE` | Révision attendue | Révision publiée |
| `RETAIN` | Révision attendue et nombre de segments entre 1 et 8 | Révision publiée |

Les numéros pertinents ont deux portées. Une séquence identifie un événement publié sur ce journal. Une révision identifie son état administratif et son contenu. Chaque append effectif, changement de rétention, purge effective ou rotation effective avance la révision. Une purge conserve la prochaine séquence globale. L’écart entre première et prochaine séquences correspond au nombre d’événements actuellement retenus. Un conflit sur la révision ou sur la prochaine séquence renvoie `BUSY`. [Trame de stockage](../user/services/journal_storage_wire.rs)

## Organisation, rotation et quota

NovaFS tient l’ensemble des segments logiques dans un objet privé P9 nommé `U+001F` suivi de `NovaOS-journal`. L’objet contient un en-tête de 96 octets et au maximum 8 segments de 16 événements. Un segment occupe au maximum 1 536 octets utiles, et l’objet complet 12 384 octets. L’en-tête porte la signature `NVJSEG01`, les deux identifiants de volume, la révision, la prochaine séquence, la rétention, le nombre de segments, leurs huit longueurs et le CRC-32 de l’objet entier calculé avec le champ CRC à zéro.

La lecture vérifie le type de l’objet, les limites, le CRC, l’identité du volume, le remplissage à zéro, la contiguïté des séquences et le schéma de chaque événement. Le journal de service et la configuration persistante emploient la même routine CRC-32. Les références de blocs P9 conservent leurs contrôles d’intégrité. Un attaquant contrôlant tout le stockage peut restaurer une ancienne image ou reconstruire une enveloppe cohérente.

La rotation automatique intervient avant l’écriture qui dépasserait le dernier segment. Au plafond, elle évince le plus ancien segment entier. Une rotation manuelle sur le dernier segment déjà vide conserve la révision. Une réduction de rétention évince les segments excédentaires les plus anciens. Le fichier conserve sa plus grande taille allouée dans cette limite et remet à zéro les octets libérés du point de vue courant. La purge retire les enregistrements de la consultation courante. Les snapshots, les anciens blocs CoW et les sauvegardes ont leur propre durée de conservation. [Stockage](../user/novafs/files/journal_store.rs)

Avant chaque commit, le plan `prepare_write_id` comptabilise les données et métadonnées lui restant à allouer. Le journal exige encore 16 blocs ordinaires libres de 4 Kio après cette réservation, en plus de la réserve de maintenance P9. Pour l’inauguration, une marge supplémentaire de 64 blocs couvre le plan de création ou de renommage. Un refus pour espace bas conserve les événements déjà publiés. Ce budget porte sur les blocs, indépendamment de la taille de secteur de 512 ou 4 096 octets. [Admission P9](../../novaFS/src/engine/admission.rs)

## Publication, récupération et doublons

Le premier append crée un objet de préparation privé, y écrit l’enveloppe complète puis le renomme. Les trois transactions disposent chacune des barrières F1/F2 du moteur. Une préparation abandonnée reste invisible, y compris quand elle est vide ou partiellement écrite. Un nouvel essai explicite peut réutiliser cette préparation après relecture du nom canonique.

Une fois le journal créé, chaque publication utilise un seul plan CoW pour l’en-tête et les données. F1 ordonne les blocs préparés avant la publication, F2 confirme la publication selon le contrat du périphérique. Le serveur envoie l’acquit de succès après le retour du commit. Les lectures de consultations utilisent une vue vérifiée. Un curseur tombé avant la première séquence conservée doit être traité explicitement par le démon.

L’EID combine le BootID, l’identité d’instance et la génération de l’émetteur, et son numéro d’événement. Un événement conservé et identique à l’exception de la séquence globale proposée rend sa séquence initiale. Un EID conservé avec d’autres données est refusé. Après éviction ou purge, l’historique permettant de reconnaître cet EID a expiré. Un résultat incertain interdit la répétition automatique, même si l’événement ne figure plus parmi les enregistrements consultables.

| Situation | Comportement observable |
| --- | --- |
| Refus de remise au rendez-vous IPC | Erreur de remise, aucune opération admise sur cette requête |
| Échec sur l’objet inaugural de préparation avant le renommage | L’événement reste non publié. Le moteur peut exiger un remontage |
| Échec certain de préparation, saturation ou conflit | Refus explicite, dernière publication conservée |
| Publication de résultat inconnu ou perte de réponse après remise | `UNCERTAIN` pour une mutation, aucune répétition automatique |
| Réponse reçue sur l’ancien tag après abandon | L’appel courant attend son propre tag et son type de réponse |
| Enveloppe publiée invalide ou événement partiellement encodé | Lecture refusée. Les mutations n’effacent ni ne remplacent l’objet corrompu |
| Ancre P9 déchirée qui rend le montage normal ambigu | Refus du montage normal selon le contrat P9 existant |

À l’échelle du VFS, le nom de préparation et le nom publié sont filtrés du parcours de la racine. `OPEN` historique refuse les contrôles ASCII. Les opérations relatives, les chemins fragmentés, les opérations de renommage et les sélections déléguées passent par `take_path`, qui vérifie la chaîne reconstituée et rejette également `..`. Les opérations par poignée retrouvent la table de la session. Donner un numéro d’objet P9 à la place d’une poignée utilisateur ne confère aucun droit. P9 résout le nom exact et son parent et distingue deux sortes d’objets, fichier et répertoire. Les capacités de lecture publique ordinaires continuent donc d’en énumérer uniquement la vue publique. [Entrée VFS](../user/novafs/files.rs) · [Résolution utilisateur](../user/novafs/files/namespace.rs) · [Délégations](../user/novafs/files/selected.rs) · [Résolution NovaFS](../../novaFS/src/fs.rs)

## Qualification du stockage

Le banc compile le module réellement utilisé par NovaFS avec le moteur du dépôt, puis l’exécute deux fois sur macOS. Il utilise `SimDevice` pour les coupures, la mesure et la saturation, et `FileImage` pour la préparation orpheline et la corruption d’enveloppe. Chaque Mach-O couvre les secteurs de 512 et 4 096 octets. La campagne de transport utilise le client du démon, des résultats de rendez-vous injectés et un compteur d’envois. [Banc P9](../tests/journal_stockage.rs) · [Lanceur](../tests/journal_stockage.sh) · [Banc de transport](../tests/journal_transport.rs)

```sh
novaOS/tests/journal_stockage.sh aarch64
novaOS/tests/journal_stockage.sh x86_64
rustup run stable rustc --edition 2024 --test --target aarch64-apple-darwin -C opt-level=2 \
    novaOS/tests/journal_transport.rs -o "$NOVAOS_TMPDIR/journal-transport-arm"
"$NOVAOS_TMPDIR/journal-transport-arm"
rustup run stable rustc --edition 2024 --test --target x86_64-apple-darwin -C opt-level=2 \
    novaOS/tests/journal_transport.rs -o "$NOVAOS_TMPDIR/journal-transport-x86"
"$NOVAOS_TMPDIR/journal-transport-x86"
```

Résultats obtenus dans **chacune des quatre combinaisons architecture/géométrie**.

| Essai | Résultat |
| --- | --- |
| Journal initial, remontage, 129 émissions | 128 événements au quota, puis éviction du premier segment et comparaison octet par octet des 113 événements retenus |
| Rotation manuelle, rétention à 1, purge, nouvelle émission | Révisions et séquences vérifiées. Après purge, la séquence suivante demeure 131 |
| Noms ressemblants, caractères échappés ou imprimables | Création de trois fichiers publics sur des identifiants distincts, écritures, relecture du journal privé intact |
| Vue antérieure, CAS obsolète, EID identique ou contradictoire, schéma invalide | Refus ou acquit de la séquence initiale selon le cas. Aucun deuxième enregistrement |
| Réserve et rétrocompatibilité | Premier append, append courant et rotation refusés avec 15 blocs ordinaires disponibles. État publié intact. Accès au backend refusé sur P8 |
| Préparation incomplète et corruption dans un objet publié par un commit P9 valide | Préparation ignorée. L’intégrité de l’enveloppe publiée échoue et les mutations restent refusées |
| Interruption avant, pendant et après chaque commande du scénario de référence | 138 points d’arrêt sur support simulé. Création 66, append 24, rotation 24, purge 24. Remontage en lecture seule et comparaison intégrale avec l’état ancien ou nouveau |
| Erreurs des barrières avec 0 ou 1 bloc récent persistant | 24 injections. Une erreur de dernière barrière rend l’issue inconnue, avec relecture ancienne ou nouvelle selon les octets effectivement persistés |
| Préfixes persistants de 1, 96, 512 et 2 048 octets en cours d’append ou de rotation | 48 injections. Les 40 cas hors ancres restituent l’ancien journal. Parmi huit injections sur les ancres, six permettent la lecture d’une génération complète et deux imposent le refus sécurisé du montage normal |
| Client IPC, par architecture | Quatre tests ciblés. Révocation avant remise, perte d’acquit après remise, acquit ancien suivi d’une nouvelle lecture, vérification du résultat et du nombre d’envois |

Les refus après déchirure de l’ancre concernent une écriture de 96 octets de son préfixe, une fois pour append et une fois pour rotation. Cette limite historique de P9 s’applique avant toute interprétation du journal. Une coupure de processus, une perte d’écritures volatiles simulée et une coupure électrique réelle correspondent à des conditions distinctes. [Audit des ancres P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

### Mesures sur hôte

Chaque mesure d’append inclut la relecture du journal, la validation du contenu, la préparation CoW et le retour du commit. Les deux architectures ont exécuté les 129 émissions séquentielles. Les durées concernent le moteur de stockage sur le périphérique de laboratoire en mémoire. La colonne rotation correspond à une mesure sur huit segments en retenant huit segments.

| Exécutable | Secteur | Événements/s | Append p50 / p95 / p99 / max (µs) | Rotation (µs) |
| --- | ---: | ---: | ---: | ---: |
| ARM64 natif | 512 | 5 120 | 198,1 / 266,8 / 285,8 / 296,8 | 255,8 |
| ARM64 natif | 4 096 | 5 134 | 194,2 / 282,1 / 308,1 / 330,2 | 255,9 |
| x86_64 Rosetta | 512 | 3 608 | 257,2 / 420,9 / 517,3 / 527,2 | 364,8 |
| x86_64 Rosetta | 4 096 | 3 831 | 259,8 / 360,7 / 379,0 / 386,6 | 347,3 |

Le stockage charge un objet de 12 384 octets au maximum. Le tableau des événements décodés contient au plus 12 288 octets. Ces deux tailles décrivent les tampons applicatifs et excluent l’allocateur et les plans CoW du moteur. Les acquisitions du démon, celles du service NovaFS et le temps de bout en bout dans QEMU relèvent du rapport de qualification invité.

L’archive durable se trouve dans `OS-worktrees/preuves-journalisation-0220-phase2/stockage/`. Les journaux `aarch64-complet.log` et `x86_64-complet.log` contiennent les mesures individuelles et les résultats d’injection. Leurs SHA-256 vérifiés sont respectivement `9474d95dfe7f4fe6a96e44e8925a9d1b517e3091188bdcdc3b81031fa152be71` et `6231da3d8220b7795fd29c5a0e06712a90852faa36c27c4a5b83ded15884616d`. Les fichiers de transport portent le préfixe `transport-`.

Le rejeu avec les trois noms ressemblants confirme également que le seul préfixe provoquant un refus de montage dans la matrice est de 96 octets. Les sorties se nomment `aarch64-alias.log` et `x86_64-alias.log` et leur manifeste `SHA256SUMS-ALIAS` porte l’empreinte `f0cce9cd7cd764cfd517b372b0e66e11e9a8bca52e03169ae4be2224b44ec82e`.

### Refus du premier append sur un volume rempli dans QEMU

Le banc crée un fichier public de 1 245 184 octets sur une copie d’un volume P9 de 512 blocs de 4 Kio. Il écrit 304 blocs de contenu déterministe et les relit intégralement. La politique de réserve P9 et les binaires du système gardent leur configuration de qualification. Sur une autre copie de cette image, une tentative inaugurale par le client hôte du moteur renvoie `NoSpace`. La comparaison SHA-256 avant/après confirme que cette tentative préserve chaque octet de la copie.

Dans la VM, le démon doit publier son événement `ONLINE` avant d’annoncer `READY`. Sur le volume rempli, le superviseur constate `FAILED` pour le service 16. La commande native `journal status` retourne `journal indisponible (12)`. Le service 10 atteint `RUNNING`. Les opérations de lecture de `/documents/exemple.txt` et les requêtes `NVFS` du service 9 réussissent, y compris après son arrêt contrôlé et sa relance sous une nouvelle identité. Les deux vCPU acquittent l’arrêt. Une relecture indépendante de l’image confirme l’absence des noms publié et de préparation du journal, la révision 0, la prochaine séquence 1, et l’identité et le contenu des deux fichiers publics. Le refus observé dans la VM est celui du seuil de réserve lors de la première création. Le diagnostic `NoSpace` est observé directement sur sa copie hôte.

| QEMU, 2 vCPU | Secteur | Blocs ordinaires libres avant / après | Réserve maintenance avant / après | Génération P9 finale |
| --- | ---: | ---: | ---: | ---: |
| AArch64 | 512 | 55 / 51 | 128 / 128 | 398 |
| x86_64 | 4 096 | 55 / 51 | 128 / 128 | 398 |

Les deux essais utilisent `novaOS/build/target-journal-qualifie`. Les SHA-256 des noyaux sont `109ea1a5bdbac4a6d2cd05c439f671c2b47e8ea5b2f0d16297477de877176ed1` sur AArch64 et `e54fe3aaf98a2c4981674224a274ab34cffb59bde5a400d42ee65c8b654d1318` sur x86_64. Les séries, images, commandes QEMU et résultats JSON se trouvent dans `OS-worktrees/preuves-journalisation-0220-phase2/stockage/volume-plein/`. La relecture des 37 entrées du manifeste a réussi. SHA-256 du manifeste `4b7bafc766cbb8dddd79d712b6cbb6a4dac954f0843ca75f1a0f2beeb02ce961`.

Depuis la racine du dépôt, avec `NOVAOS_TMPDIR` configuré, une bibliothèque NovaFS hôte construite par le banc précédent et un répertoire de preuves durable existant désigné par `PREUVES`, exécuter les commandes suivantes. Chaque argument de dossier d’essai doit désigner un nouveau répertoire. [Préparation du volume](../tests/journal_saturation_fixture.rs) · [Scénario QEMU](../tests/journal_saturation.py)

```sh
rustup run stable rustc --edition 2024 --target aarch64-apple-darwin -C opt-level=2 \
    -L dependency=novaFS/target/aarch64-apple-darwin/release/deps \
    --extern novafs=novaFS/target/aarch64-apple-darwin/release/libnovafs.rlib \
    novaOS/tests/journal_saturation_fixture.rs -o "$PREUVES/fixture-enospc"
python3 -B novaOS/tests/journal_saturation.py aarch64 512 "$PREUVES/arm" "$PREUVES/fixture-enospc" \
    novaOS/build/target-journal-qualifie/aarch64-unknown-none-softfloat/release/novaos
python3 -B novaOS/tests/journal_saturation.py x86_64 4096 "$PREUVES/x86" "$PREUVES/fixture-enospc" \
    novaOS/build/target-journal-qualifie/x86_64-unknown-none/release/novaos
```
