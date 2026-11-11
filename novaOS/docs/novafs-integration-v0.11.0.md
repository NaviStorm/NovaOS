# Audit du portage de NovaFS en espace utilisateur

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

Pour reproduire les deux échecs du paquet complet avec les cibles Rust installées :

```sh
cd novaFS
for t in x86_64-unknown-none aarch64-unknown-none-softfloat; do
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo check --lib --locked --offline --target "$t"
done
```

**Arbitrage proposé.** Autoriser d’abord le chargeur de services multipages avec tas privé, et l’extraction du cœur portable, avec qualification indépendante sur macOS et sur les deux cibles nues. Reprendre ensuite l’intégration du même moteur sur un volume explicitement identifié. Les versions restent NovaOS 0.10.0 et NovaFS 0.9.12.
