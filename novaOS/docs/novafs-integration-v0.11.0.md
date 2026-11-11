# Montage natif de NovaFS dans NovaOS v0.11.0

Le service ELF utilisateur lie le moteur NovaFS v0.9.12 du monorepo avec `no_std + alloc`. Le profil fichier monte un volume logique autonome produit par la CLI P8 sur un disque VirtIO attribué. L’application, le service NovaFS, le service bloc et le pilote disposent de quatre espaces d’adressage distincts. Le pilote propose les commandes au composant DMA de confiance. Le noyau applique les droits et le confinement matériel. Le montage, les arbres et la publication s’exécutent en Ring 3 ou EL0. [Qualification du parcours fichier](validation/novafs-natif-v0.11.0.md)

## Chargement et protections

Le lanceur admet des ELF64 statiques little-endian de la machine cible, fournis par la construction. Il vérifie les limites du tableau de programmes et de chaque plage de fichier, la congruence offset/adresse, les débordements et les adresses utilisateur. Chaque `PT_LOAD` non vide possède des pages exclusives. Deux segments partageant une page sont rejetés, même si leurs octets sont disjoints. Chaque page reçoit la partie initialisée de son segment, avec zéros dans la BSS et les marges. Le texte est RX, les constantes R/NX et les données RW/NX. L’entrée doit désigner des octets initialisés du texte. Les ELF dynamiques, l’interpréteur, TLS et les demandes W+X sont refusés. Les démonstrateurs plats historiques gardent leur profil d’origine.

La taille de pile provient de `PT_GNU_STACK.p_memsz`, arrondie à la page. Le défaut est une page. Le service NovaFS demande 256 Kio. Une page non mappée protège chaque extrémité. Les images ELF reçoivent une page de configuration privée à `0x401000`. La pile descend sous l’adresse `2^46 − 8192`, le tas grandit à partir de `2^46`. Le noyau vérifie la plage exécutable de retour et le pointeur de pile propre au processus. Ces espaces utilisent les tables et le contrôle matériel Ring 3 ou EL0 existants. Une faute localisée entraîne la collecte de l’espace et le réveil de ses observateurs. Les mappings privés, leurs tables intermédiaires et leurs cadres restent attribués à un seul processus. Le chargeur restitue les pages déjà préparées si une admission échoue.

## Tas privé et ABI

Le sélecteur additionnel `HEAP_RESIZE = 0x00010041` utilise la convention de registres et la paire de retour `(status, value)` de l’ABI v1. Ses trois arguments sont des u64. `(0, 0, 0)` donne la borne exclusive courante. `(ancienne_borne, nouvelle_borne, 0)` compare puis déplace cette borne sous le même verrou mémoire. Les bornes sont alignées sur 4 Kio et appartiennent à la moitié utilisateur. Une extension fournit des pages zéro RW/NX. Un retrait rend les cadres et invalide leurs mappings. Le succès renvoie la nouvelle borne. Un argument non valide, une borne périmée ou un second retrait utilisant l’ancienne borne donne `BAD_ARGUMENT`. Une pénurie donne `NO_MEMORY`. Une extension échouée conserve la borne et le contenu antérieurs. Une demande sans changement est idempotente. Les seize sélecteurs historiques et le port privé du pilote gardent leurs numéros et conventions.

Le sélecteur est nécessaire pour rendre et reprendre de la mémoire physique pendant la vie du service. Un tas imposé au lancement réserverait aussi son maximum au repos. `dlmalloc` gère les objets à l’intérieur de chaque ELF, avec un verrou local. Une libération d’objet le rend réutilisable par ce processus. Le noyau collecte le tas entier même si un service meurt dans l’allocateur. Les doubles retraits et adresses invalides sont testés à la frontière entre processus et noyau. La libération d’un pointeur Rust arbitraire par du code utilisateur `unsafe` reste hors contrat de `GlobalAlloc`. Une allocation Rust infaillible peut terminer le processus. Les réservations faillibles peuvent transmettre la pénurie à l’appelant. Le provisionnement des pages est immédiat et dépend des cadres effectivement disponibles. Un déplacement très grand garde la section critique mémoire pendant toute l’opération, ce qui peut augmenter la latence de préemption sur le système mono-CPU.

[Chargeur ELF](../kernel/src/memory/user/elf.rs) · [Propriété et collecte](../kernel/src/memory/user.rs) · [Allocateur du service](../user/novafs/heap.rs) · [ABI native](../abi/native.rs)

## Autorité et durée de vie

Le lanceur attribue un seul support à la chaîne. Le propriétaire du bail matériel est le pilote identifié par PID et époque. Une seule commande matérielle peut être en vol. La réponse INFO du bloc contient à +16 le masque de droits effectifs, qui tient compte du mode lecture seule du support. Un profil demandant l’écriture exige WRITE et FLUSH avant d’admettre le client. Le fichier raw doit rester exclusivement attribué à la VM pendant son montage.

Le client reçoit SEND sur un endpoint de requêtes et RECEIVE sur un endpoint de réponses. Seul le service détient leurs racines. Ce couple autorise l’accès à l’espace de noms du volume attribué. Le numéro de session (PID du service) corrèle les messages. Les handles désignent des entrées de la table privée de ce canal, avec des droits réduits à l’ouverture. Aucun des deux entiers ne permet à un autre processus d’obtenir l’endpoint. Une session admet 32 handles simultanés, numérotés de façon croissante avec refus du débordement. Une seconde ouverture du même chemin peut donner des droits différents. Le service traite une requête à la fois. Les ACL par fichier et la délégation de sous-arborescences restent à développer.

La fermeture de session retire la racine des requêtes avant la dernière réponse. La fin du service révoque le canal de réponses. Après une déconnexion, le lanceur ferme les canaux du groupe, attend les processus et retire le bail DMA. Une relance crée des processus, des capacités et un bail neufs. Une panne pendant une publication exige un nouveau montage. Un client qui perd la réponse d’une mutation après envoi obtient une issue indéterminée. La bibliothèque invalide alors la connexion. Les attentes sont bornées à 250 ticks pour un échange bloc, 5 000 pour la réception au repos du relais et du pilote et 15 000 pour la réponse fichier (250 Hz). Un client durable doit traiter une expiration comme une interruption de session. Le service de diagnostic démarré sans disque continue de proposer INFO/FIN v1.

## Protocole fichier v2

Chaque requête et réponse utilise exactement 128 octets. Le tag IPC augmente à chaque requête. Les champs multioctets sont little-endian. Les champs de requête inutilisés et la fin du payload doivent être nuls. Aucune adresse d’un processus ne traverse cette interface.

| Offset | Type | Champ |
|---:|---|---|
| 0, 1, 2, 3 | 4 × u8 | Version 2, opération, options, statut (zéro en requête). |
| 4, 6 | u16, u16 | Longueur du fragment et réservé. |
| 8, 16, 24, 32 | 4 × u64 | Session, handle, offset absolu, taille totale ou résultat numérique. |
| 40–127 | 88 octets | Chemin, données ou résultat INFO. Une erreur rend une charge utile vide. |

| Opération | Contrat |
|---|---|
| 1 MOUNT | Session et handle nuls. Le moteur décode l’ancre, vérifie les deux générations, leur filiation, la géométrie, les références, les données et la partition de l’espace. Le montage RW attend un FLUSH du support. La réponse livre la session à +8. |
| 2 INFO | Handle nul. Dans le payload, LV et FS (16 octets chacun), secteur et bloc (u32), capacité et disponible ordinaire (u64, en blocs), somme des tailles logiques publiées (u128), génération (u64), mode (1 RO, 3 RW), version de contenu (0), fonctionnalités (bit 0, transactions de fichiers). Le disponible déduit aussi le crédit de la préparation en cours. |
| 3 OPEN | Chemin absolu UTF-8 de 1 à 88 octets. Les composantes vides, `.`, `..` et les caractères de contrôle ASCII sont refusés. Options 1 lecture, 2 écriture, 4 création si absent. La création exige le droit d’écriture et publie par le moteur avant de livrer le handle à +16. Une création est suspendue par un retour BUSY si une écriture occupe la réservation. |
| 4 READ | Offset et nombre d’octets demandés, au plus 88 par trame. Le compte réel figure à +4. Zéro octet marque la fin du fichier pour une demande non vide. Le handle auteur voit sa propre écriture entièrement assemblée et admise, y compris les trous remplis de zéros. Les autres handles voient la vue publiée. |
| 5 WRITE | Une seule écriture privée à la fois. START=1, CONTINUE=2, longueur totale de 1 à 65 536 octets, fragments contigus et non vides. La réponse à +32 donne le cumul reçu. Au dernier fragment, `Engine::prepare_write` contrôle le coût CoW et les réservations. Le service retient le crédit et les octets, puis libère le plan. La génération reste exclusive jusqu’à SYNC ou abandon. |
| 6 SYNC | Handle inscriptible. Une requête incomplète renvoie BUSY. Le moteur reconstruit le plan sur la même tête, écrit ses dépendances, attend F1, publie l’ancre alternée et attend F2. La réponse à +32 identifie la génération. Un SYNC propre envoie également un vrai FLUSH. La double préparation a un coût inclus dans les mesures. |
| 7 CLOSE | Ferme le handle indiqué. Le bit DISCARD=1 autorise l’abandon explicite de sa préparation privée. Une fermeture ordinaire avec des octets en attente renvoie BUSY. Avec handle nul, la fermeture exige une session propre, libère tous ses handles, démonte le moteur, attend CLOSE du bloc et révoque l’accès. Une erreur de publication reste visible. |

Les erreurs ABI utilisées comprennent BAD_ARGUMENT, DENIED, LIMIT, REVOKED, CLOSED et NO_MEMORY. Les statuts du service sont 20 (absent), 21 (intégrité), 22 (publication indéterminée), 23 (volume plein, quota ou réserve), 24 (occupé), 25 (format ou opération non pris en charge) et 26 (E/S). L’intégrité, l’E/S et la publication indéterminée rendent la session inutilisable jusqu’à sa destruction. La bibliothèque fragmente les lectures et écritures. Lors d’une erreur de lecture multifragment, des octets des premiers fragments peuvent déjà se trouver dans le tampon de destination.

[Trame fichier](../abi/files.rs) · [Bibliothèque du client](../user/storage/fileapi.rs) · [Service fichier](../user/novafs/files.rs) · [Adaptateur sectoriel](../user/novafs/device.rs)

## Profil disque et reprise

P8 désigne la version du moteur et de son scrub. Un **LV autonome** possède l’enveloppe historique de contenu P0, deux ancres de 4 Kio et des manifestes chaînés. Le service suit le chemin commun `Engine::mount`, `prepare_write` et `write`. La réserve ordinaire de maintenance vaut 128 blocs. La politique du LV autonome autorise les fichiers creux sans quota logique externe. Les quotas et les crédits de pools requièrent le coordinateur de pool.

Les catalogues P0, les pools partagés P5/P6 et les pools P7 ont d’autres points d’entrée. P5/P6 exigent l’union des racines de tête et de prédécesseur, les références de vues, la réservation physique et, pour P6, la classification des unités compressées. P7 exige en plus la validation des banques et attestations répliquées, de leur filiation et des authentificateurs avec les clés appropriées. Aucun de ces conteneurs n’est exposé en écriture dans ce profil. Le service refuse une signature incompatible dès le début du montage. Il ne cherche pas un LV en balayant le disque. Les snapshots, clones, LZ4, clés et exports conservent leurs contrats dans NovaFS. L’invité ne propose aucun déverrouillage. [Contrats de récupération et de scrub](../../novaFS/docs/novafs-p8-scrub.md)

Le trait Device traduit chaque bloc de 4 Kio en un ou huit secteurs, les transporte par fragments de 96 octets et se ferme à la première erreur bloc. Les barrières attendent le statut de complétion VirtIO. Avant la publication de l’ancre, la génération précédente reste exigée. Pendant la publication, l’issue peut être l’ancienne génération ou la nouvelle entière. Une ancre effectivement déchirée impose le refus du montage normal. Après confirmation de F2, le moteur exige la nouvelle tête sous les hypothèses du support. Les données non confirmées exigent une reprise avec contrôle de génération. [Publication du moteur](../../novaFS/src/engine.rs) · [Modèle P1](../../novaFS/docs/novafs-p1-durabilite.md)

## Reproduction

Depuis la racine du monorepo, `make -C novaOS ARCH=x86_64 file-test` et `make -C novaOS ARCH=aarch64 file-test` construisent la CLI hôte et lancent les deux géométries. Pour consulter son propre LV autonome, utiliser `make -C novaOS ARCH=aarch64 file-info SECTOR=4096 IMAGE=/chemin/volume.raw`, avec un chemin absolu vers un fichier ordinaire. Les deux architectures acceptent les secteurs 512 et 4 096. La commande ouvre l’image en lecture seule et affiche l’identité du volume. Les essais en écriture emploient des copies temporaires.

[Rapport de validation du montage et des transactions](validation/novafs-natif-v0.11.0.md) · [Qualification précédente du chargeur et du tas](validation/preparation-novafs-v0.11.0.md)

---

## Audit initial sur la base 2e9ad2c

**Décision du 11 novembre 2026 — arrêt avant implémentation.** Sur la base propre `2e9ad2c261652ffbf7edc30457bb2980b3caeb84` de `main`, le service NovaOS 0.10.0 sait relire un secteur synchronisé après redémarrage. Il ne peut pas héberger le moteur NovaFS 0.9.12 dans son espace utilisateur actuel. Deux travaux distincts nécessitent un arbitrage : le chargement de services à plusieurs segments et la séparation du moteur transactionnel de son environnement Darwin. Les licences vérifiées sont [MIT pour NovaOS](../LICENSE) et [MPL-2.0 pour NovaFS](../../novaFS/LICENSE).

## Ce qui empêche le premier montage

| Point vérifié | Constat sur la base | Conséquence pour le portage |
|---|---|---|
| Image utilisateur | Le build invoque directement `rustc` pour chaque service. L’éditeur de liens limite texte et constantes à 4 092 octets et écarte `.data` et `.bss`. | Ajouter une dépendance Cargo au noyau ne la lie pas dans le service. Il faut construire un exécutable utilisateur distinct et charger ses segments avec leurs permissions. |
| Mémoire du processus | Le lanceur installe une page RX, une page RW de configuration et une pile de 4 Kio entre deux gardes. Son registre contient quatre adresses de pages, dont une optionnelle. L’ABI publique ne fournit ni tas ni appel d’allocation de pages. | La restitution à la mort du service, la copie IPC et le contrôle des mappings utilisent ce registre. Augmenter seulement la limite du linker ne rendrait pas ces pages accessibles. |
| Mémoire du moteur | Lors du chargement d’une banque P7, `State::load` crée un préfixe de sept blocs et un bloc de preuves, soit **32 Kio de contenu vivant** avant les domaines et les structures du FS. Même le chemin historique utilise des pages privées de 4 Kio et un plan CoW pouvant atteindre 64 pages. | La première valeur est une borne tirée du code, pas une mesure du pic. Désactiver le cache ne fournit ni les pages de transaction ni un allocateur au processus. |
| Frontière du moteur | La bibliothèque inclut directement le périphérique du laboratoire. `Device::mutation_lock` expose `Arc<Mutex<()>>`, les erreurs exposent `std::io::Error`. Le moteur verrouille son cache. Les coordinateurs protègent la file et la phase de publication. Le prêt du graphe partagé est lié à une garde concrète. | Isoler les imports de fichiers ne suffit pas. Il faut porter l’autorité d’écriture et la durée de vie de ses gardes tout en maintenant le comportement des écrivains hôtes. |
| Dépendances hôtes | Le fichier image utilise des E/S positionnelles, `tempfile` et `sync_all`. Les exports utilisent des flux `Read/Write`, le scrub chronomètre le parcours avec `Instant`. `HostEntropy` est absent sur cible nue, mais la façade du pool sécurisé l’importe. | Garder fichiers, CLI, JSON et instrumentation Darwin côté hôte. Prévoir des interfaces pour le flux, l’horloge de mesure et l’entropie. Les clés d’administration et l’accès à une entropie fiable nécessitent un provisionnement explicite avant toute mutation qui les exige. |

Sources : [liaison](../user/demo.ld), [construction des exécutables](../kernel/build.rs), [mappings, copie et collecte](../kernel/src/memory/user.rs), [contrat ABI](../abi/native.rs), [interface du moteur](../../novaFS/src/lib.rs), [backend hôte](../../novaFS/lab/device.rs), [état P7](../../novaFS/src/secure_pool.rs), [transactions](../../novaFS/src/engine.rs), [garde du graphe partagé](../../novaFS/src/shared.rs).

Cette refonte concerne l’hébergement du moteur et ses frontières de confiance. Rien dans l’audit ne justifie de remplacer les arbres, les codecs ou le format disque. Un service multipage avec une arène privée préallouée est une option qui évite d’emblée un nouvel appel système public. L’arène doit appartenir au processus, avec pages NX, piles gardées, effacement et collecte vérifiés. L’échec de provisionnement avant lancement et la mort pendant une publication doivent rester récupérables.

## Chemin de portage proposé pour arbitrage

1. **Héberger le service.** Conserver les petits exécutables existants. Pour l’image du moteur, provisionner séparément texte RX, constantes R, données et BSS RW/NX, pile et tas. Tester sur les deux architectures l’admission partielle, les gardes, le cloisonnement DMA et la restitution après arrêt. Le moteur s’exécute uniquement en Ring 3 ou EL0.
2. **Extraire une bibliothèque commune.** Réutiliser les mêmes sources de montage, validation, CoW et comptabilité avec `core` et `alloc`. Garder un seul écrivain par pool. Sa garde doit couvrir la relecture de la tête, la préparation, F1, la publication et F2, y compris après redémarrage du service. Un mutex local ne protège pas contre un second écrivain externe au disque. Qualifier séparément la perte du bail et la quiescence avant réattribution. Prévoir des erreurs de transport distinguant refus avant soumission et issue indéterminée.
3. **Adapter le périphérique.** Le service bloc accepte des trames de 128 octets avec au plus 96 octets de données. Un bloc NovaFS de 4 Kio correspond à huit secteurs de 512 octets ou un secteur de 4 096. Valider capacité, multiplication et plage entière avant transfert. Réassembler toutes les pièces et contrôler leurs réponses avant de livrer un bloc lu. Une écriture partiellement soumise peut avoir modifié le support. Le service actuel produit et compare un motif de test au lieu de transporter les tampons du moteur. [Protocole bloc](../abi/block.rs) · [Client bloc actuel](../user/storage/medium.rs)
4. **Exposer les fichiers.** La bibliothèque applicative utiliserait les capacités du service NovaFS. MOUNT sélectionnerait un volume admis par le superviseur. INFO lirait ses métadonnées. OPEN fournirait un handle local à la session avec des droits et une génération. READ et WRITE valideraient taille, déplacement, droits et fragments. SYNC ne confirmerait une transaction qu’après F2. CLOSE libérerait le handle. Les LBA et les capacités du pilote resteraient privés. Le lanceur distribue aujourd’hui les capacités au démarrage et retire les endpoints à la fermeture. L’interface fichier proposée reste à spécifier et à tester. [Cycle de vie et protocole existant](novafs-integration-v0.9.0.md)

## Format, reprise et compatibilité

P8 désigne la version du moteur qui ajoute le scrub. Elle reconnaît plusieurs formats historiques : LV et catalogue P0, pools partagés P5/P6 et pool authentifié P7. Un premier essai sur un LV autonome créé par la CLI P8 validerait uniquement ce profil. L’acceptation d’un pool partagé doit passer par son catalogue, ses références de tête et de prédécesseur et ses réservations. Identifier explicitement chaque profil pris en charge avant de permettre les mutations. [Récupération](../../novaFS/docs/novafs-recovery.md) · [Contrôles P2](../../novaFS/docs/novafs-p2-integrite.md) · [Admission P3](../../novaFS/docs/novafs-p3-espace.md) · [Cache P4](../../novaFS/docs/novafs-p4-cache.md)

Conserver les représentations existantes des snapshots et clones, le décodage LZ4, la vérification des unités chiffrées avant décompression et le format des exports autonomes. Le scrub en lecture seule et sa réparation contrôlée ont des autorisations différentes. Un volume demandant un profil ou une clé indisponible doit échouer explicitement. Aucune conversion automatique n’est prévue. [Partage P5](../../novaFS/docs/novafs-p5-partage.md) · [Compression](../../novaFS/docs/novafs-p6-compression.md) · [Clés et exports](../../novaFS/docs/novafs-p7-chiffrement.md) · [Scrub](../../novaFS/docs/novafs-p8-scrub.md)

F1 stabilise les dépendances CoW avant l’ancre ou les attestations de publication. F2 stabilise cette publication avant le succès client. Une erreur d’E/S après soumission impose la reprise prévue par le format avant de réutiliser les blocs. Pour P7, la reprise peut drainer le périphérique, vérifier de nouveau la tête et confirmer la réparation autorisée d’une réplique. Le résultat de FLUSH du disque virtuel permet de représenter les barrières du moteur dans le modèle QEMU. Le test sectoriel ci-dessous ne les exerce pas dans une transaction NovaFS. [Journal P7](../../novaFS/src/secure_commit.rs) · [Contrat P1](../../novaFS/docs/novafs-p1-durabilite.md) · [Qualification du chemin DMA](stockage-bloc-v0.10.0.md)

## Vérifications exécutées et limites

Hôte macOS ARM64, Rust stable 1.99.1, QEMU 11.1.3. Les vérifications suivantes portent sur les composants de départ.

| Contrôle | x86_64 | AArch64 |
|---|---|---|
| Compilation du noyau 0.10.0 | Réussie | Réussie |
| Compilation bare-metal de la bibliothèque NovaFS complète, dépendances verrouillées | Échec E0463, `std` indisponible | Échec E0463, `std` indisponible |
| Compilation isolée de la source cryptographique P7 et de ses dépendances | Réussie | Réussie avec backend SHA logiciel |
| Persistance sectorielle QEMU 0.10.0 : WRITE, FLUSH, arrêt forcé, comparaison intégrale de l’image et relecture dans une nouvelle VM | Réussie pour 512 et 4 096 octets | Réussie pour 512 et 4 096 octets |
| Tests hôtes ciblés `p8_scrub`, dont coupures, E/S, reconstruction et remontage | 6/6 sous Rosetta | 6/6 en natif |

La sonde cryptographique compile une bibliothèque. Elle ne qualifie ni l’édition de liens d’un exécutable de service, ni son entropie, ni le cycle de vie des clés. Les suites hôtes complètes, instrumentées et différentielles n’ont pas été relancées pour cet audit documentaire.

Les binaires plats du profil bloc mesurent 3 600 octets sur x86_64 et 3 668 sur ARM. Il s’agit de tailles de fichiers, pas du RSS. Les latences OPEN/READ/WRITE/SYNC, les débits fichier, le coût du montage et les allocations du moteur dans NovaOS restent sans mesure. Les mesures antérieures du transport et des cadres résidents sont disponibles [par profil QEMU](validation/bloc-natif-v0.10.0.md).

Avant qualification, utiliser des images jetables et un oracle macOS indépendant. Pour chaque architecture et secteur, créer un fichier côté hôte, le lire puis créer ou modifier un fichier côté invité, confirmer SYNC, tuer QEMU, contrôler le contenu avec le moteur hôte puis remonter en VM. Répéter autour de F1 et F2, avec échecs de lecture, d’écriture et de flush. Exiger tous les succès précédemment confirmés, rejeter les graphes incohérents et comparer les cadres, allocations et droits après fermeture et redémarrage. Ce parcours fichier et ses mesures restent à exécuter après arbitrage. Il ne qualifierait pas une coupure électrique physique.

### Reproduction depuis la racine du dépôt

```sh
for a in x86_64 aarch64; do
    make -C novaOS ARCH="$a" kernel &&
        (cd novaOS && NOVA_ARCH="$a" python3 tests/bloc.py --only durable) || exit 1
done
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    NOVAFS_TARGET="$t" novaFS/lab/run test --offline --test p8_scrub || exit 1
done
python3 novaFS/lab/p7-no-std.py
```

Sur la révision initiale 2e9ad2c, les deux échecs du paquet complet se reproduisaient avec les cibles Rust installées :

```sh
cd novaFS
for t in x86_64-unknown-none aarch64-unknown-none-softfloat; do
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo check --lib --locked --offline --target "$t"
done
```

**Arbitrage proposé.** Autoriser d’abord le chargeur de services multipages avec tas privé, et l’extraction du cœur portable, avec qualification indépendante sur macOS et sur les deux cibles nues. Reprendre ensuite l’intégration du même moteur sur un volume explicitement identifié. Les versions restent NovaOS 0.10.0 et NovaFS 0.9.12.
