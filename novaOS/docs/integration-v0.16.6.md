# NovaOS v0.16.6 — Intégration des E/S dans main

## Modifications livrées

- Le chemin classique conserve les services isolés, les contrats IPC/VFS publiés et la publication vers un nom libre après confirmation. La boîte PAGE de 4 Kio et le transport sectoriel assurent la communication avec des composants de générations différentes. La découverte porte sur le canal et les droits accordés à son titulaire. L’INFO historique de vi garde sa réponse de dix octets. [ABI IPC](ipc-abi.md) · [Contrats de négociation](es-v0.16.6-negociation.md)
- Le pilote VirtIO-blk conserve la propriété du domaine, des traductions et des cadres jusqu’au retrait confirmé. Une commande déjà soumise doit être drainée. Un retrait incertain conserve le bail en quarantaine. La préparation CoW groupée, les prêts immuables multipages et la file de lots sont disponibles dans un export de test explicitement construit. [Propriété DMA](es-v0.16.6-correctifs-dma.md)
- Le moteur reste NovaFS v0.9.13 P9. L’index d’extents et la lecture des pages déjà vérifiées ont été optimisés dans le moteur. Le protocole P9 écrit les dépendances et le manifeste avant F1 puis l’ancre alternée avant F2. Le succès suit la synchronisation promise. La sauvegarde classique peut effectuer plusieurs publications P9 pour préparer une seule copie. Le chemin groupé qualifié utilise deux FLUSH. [Contrat de publication et ordre des écritures](es-v0.16.6-integration.md#publication-coupure-et-compatibilité-disque)

## Périmètre Git et provenance

La référence de départ de main est `3193bc9108bdc771f4627f57c84e093375cc9ea4`. Le candidat autorisé est `f7654a3bc913017418cdd0a5b833b5e6a5338631`. Après récupération des références distantes, main était leur ancêtre commun et le dépôt était propre. La comparaison couvre 27 commits et 174 fichiers de stockage, noyau, CLI, bancs et rapports. La fusion conserve les deux parents. La version Cargo des trois paquets (noyau, CLI et service de stockage) correspond à leur bannière et à leurs sorties `--version`. Le numéro du format P9 et celui de son moteur suivent le cycle de NovaFS.

Le code de la fusion a été figé dans l’arbre Git `22ea6c7958db06a4917f60e1cedd09935d9a8629` avant les compilations. Les relevés identifient également l’objet local de qualification à deux parents `a0e7b000c5bf4d3246361ad13978540ec5c4aaf2`, utilisé pour exporter les sources. Les mesures sur le groupe expérimental identifient la liste des fixtures appliquées à cet export. Les rapports du candidat antérieur gardent leurs dates, leurs noyaux et leurs résultats. [Audit ayant motivé la décision MOA/MOE](es-v0.16.6-correctifs.md)

Le relevé de distribution conserve les identifiants Git des sous-arbres `components`, `novaFS`, `novaOS/kernel`, `novaOS/user`, `novaOS/abi` et des scripts de construction. Pour vérifier la correspondance sur un clone du commit de livraison, comparer `git rev-parse HEAD:novaOS/kernel` et les autres chemins de la même manière avec `composants_git_qualifies`. Les ajustements postérieurs aux compilations portent sur les bancs hôtes, les résultats, la distribution et sa documentation. [Empreintes des sources et de l’image](validation/integration-main-v0.16.6-livraison.json)

## Configuration et activation de test

| Construction | Chemin de sauvegarde | Attente VirtIO-blk |
|---|---|---|
| `make -C novaOS ARCH=aarch64 kernel`, équivalent x86_64, système UTM ou installateur | Chemin classique. PAGE peut être négocié sur la liaison bloc, dans la limite de la capacité. | Polling de la virtqueue et acquittement de l’interruption du périphérique. |
| Export explicite `sans` ou `utm` | Profil de qualification avec vi, nsh, service NovaFS et pilote capables de reconnaître BATCH et PLAN_V1. Le pilote vérifie le bail et les deux extrémités. | Polling. |
| Export expérimental avec la fixture d’attente IRQ | Même protocole de stockage dans le profil qualifié. | Attente par interruption avec réveil et repli bornés. [Banc IRQ](es-v0.16.6-irq.md) |

La commande suivante construit les trois profils QEMU et le profil UTM dans des répertoires neufs, à partir du commit choisi. Le profil `sans` désigne historiquement le **profil groupé sans sonde**. Le profil `brut` reproduit la construction ordinaire. Cette commande lance effectivement un invité AArch64 et utilise un disque jetable. Les fichiers `provenance.json` identifient les sources, les binaires et les fixtures. Le numéro de profil de démarrage à lui seul ne peut activer BATCH dans le binaire brut. [Constructeur des exports](../tests/reconstruire_es_0166.py)

```sh
sortie="$PWD/novaOS/build/qualification-main-0166"
python3 -B novaOS/tests/reconstruire_es_0166.py HEAD "$sortie" brut sans sonde utm
python3 -B "$sortie/sans/novaOS/tests/es_groupees_elargies.py" aarch64 512 ajout-4m-multiple \
    "$sortie/sans/novaFS/target/aarch64-apple-darwin/release/examples/es-groupees" \
    "$sortie/sauvegarde-aarch64.json" --sans-sonde
```

La construction ordinaire annonce uniquement PAGE au niveau bloc. Le client ne choisit la voie sectorielle historique que pour une absence de fonction reconnue avant la première opération. Il propage les refus de droit, retraits et erreurs de transport. Sur la voie expérimentale, les capacités du noyau, du relais, du pilote, de la session NovaFS et du dialogue de vi doivent correspondre avant le prêt ou la publication. Une réponse incertaine après publication interdit de rejouer automatiquement l’écriture. PC1 et le remplacement transactionnel restent désactivés. [Trame bloc](../abi/block.rs) · [Lecteur et négociation](../user/storage/medium.rs) · [Matrice des contrats](es-v0.16.6-negociation.md)

## Installation dans UTM

Depuis la racine du dépôt, décompresser la distribution dans un nouveau répertoire. `paquet_installation.py creer` construit un profil UTM AArch64 à partir du firmware privé et initialise une cible jetable de 64 Mio. Le disque d’installation est en lecture seule. Après confirmation et installation, arrêter la VM, exporter la copie installée, la passer en mode système et la démarrer depuis la cible. La procédure illustrée par les essais v0.16.5 explicite le cycle des profils. [Procédure du profil UTM](es-v0.16.5.md#installation-dans-utm) · [Installateur v0.16.6](../../distribution/images/novaos-0.16.6-utm-aarch64.tar.gz)

```sh
tar -xzf distribution/images/novaos-0.16.6-utm-aarch64.tar.gz
(cd novaos-0.16.6-utm-aarch64 && shasum -a 256 -c SHA256SUMS)
python3 -B novaOS/scripts/paquet_installation.py creer novaos-0.16.6-utm-aarch64 vm-0166
```

## Mesurer la sauvegarde durable

La comparaison porte sur une création de 4 Mio, une insertion de deux octets au milieu d’un fichier de 4 Mio et une édition multiple donnant une destination de 8 Mio. La durée s’étend de la commande de sauvegarde à l’acquittement. La grille sépare le temps après confirmation, les compteurs IPC de la sauvegarde, les FLUSH et le pic de la session. Le témoin supplémentaire utilise l’ELF effectivement présent sur le disque installé. [Mesures individuelles et procédure de reproduction](integration-v0.16.6-performances.md)

Les 22 sauvegardes groupées hors sonde sont restées sous une seconde. Les maxima mesurés sont **131,940 ms** sous QEMU AArch64, **198,201 ms** sous QEMU x86_64 et **909,640 ms** dans UTM AArch64/QCOW2. Sur les 18 cellules groupées avec compteurs, le maximum est de 47 échanges interservices et dons acquittés, **56 en incluant les contrôles**. Le décompte qui inclut aussi les copies locales du prêt atteint 64. Le chemin classique a employé de 84 311 à 168 600 échanges avec contrôles. La mémoire maximale de session sur les passages instrumentés est de 11 969 cadres en classique et 9 766 en groupé (cadres de 4 Kio). Ces maxima portent sur des entrées allant jusqu’à 8 Mio en sortie et incluent les services de la session.

| Environnement | Essais hors sonde par chemin | Classique par défaut, min–max | Groupé expérimental, min–max |
|---|---:|---:|---:|
| QEMU AArch64, image brute | 6 | 4,846–10,445 s | 74,915–131,940 ms |
| QEMU x86_64, image brute | 6 | 4,245–8,732 s | 127,016–198,201 ms |
| UTM AArch64, QCOW2 | 10 | 4,510–61,722 s | 92,762–909,640 ms |

## Limites de la qualification

Le domaine qualifié est mono-vCPU. Un retrait matériel durablement incertain peut retenir des cadres précisément inventoriés jusqu’au redémarrage. Sur x86_64, neuf cadres VT-d appartiennent à la machine entre sessions. Le lanceur ouvre le transport avant de créer les espaces d’adressage utilisateurs. Une ouverture initiale plus tardive pour des processus déjà créés demanderait de revoir la propagation des mappings superviseur.

Un arrêt forcé de la VM laisse le Mac alimenté. La perte électrique physique n’a pas été exercée. L’atomicité d’une écriture d’ancre sur tout stockage ne peut pas être déduite d’un FLUSH. Le montage normal refuse une paire d’ancres invérifiable. Sous UTM, la charge hôte et le placement du vCPU peuvent modifier les latences. Les campagnes antérieures avaient observé jusqu’à 1 951,191 ms en polling. [Limites de P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md) · [Extrêmes historiques UTM](es-v0.16.6-irq.md)

## Construction de la distribution

Les fichiers EDK2 `QEMU_EFI.fd` et `QEMU_VARS.fd` proviennent du firmware privé qualifié pour le profil AArch64. Le noyau autonome est d’abord placé dans les ESP des deux géométries. L’installateur embarque ensuite ces données. La distribution utilise les sources ordinaires du dépôt, sans fixture de laboratoire. Définir `FIRMWARE` avec le chemin absolu vers ce répertoire. Depuis la racine du dépôt, utiliser la chaîne Rust et les outils de création d’image du projet.

```sh
(
    set -eu
    cd novaOS
    mkdir -p build
    sortie=$(mktemp -d "$PWD/build/distribution-0166.XXXXXXXX")
    cible=aarch64-unknown-none-softfloat
    export RUSTC="$(rustup which --toolchain stable rustc)"
    rustup run stable cargo build --locked --offline --release --target "$cible" \
        --features systeme-gpt --target-dir "$sortie/systeme"
    python3 -B scripts/esp_installation.py "$sortie/systeme/$cible/release/novaos" "$sortie/esp"
    NOVA_ESP="$sortie/esp" rustup run stable cargo build --locked --offline --release \
        --target "$cible" --features installation-gpt --target-dir "$sortie/install"
    python3 -B scripts/paquet_installation.py construire "$FIRMWARE" \
        "$sortie/install/$cible/release/novaos" "$sortie/support"
)
```

L’archive installable mesure 3 802 997 octets. Son SHA-256 est `e53e8b8190633dda5214b20dbda9d568aca07980fe71d24b7db234dc6c23926a`. Les 27 fichiers du manifeste de l’archive ont été vérifiés. L’installation a utilisé le support QCOW2 dont le SHA-256 est `fe878e38bc26dade3c9279bec97782d73425ccfff03168b933fc434a777090b9`. L’application UTM 5.0.8 a redémarré deux fois sur le seul disque installé. Après redémarrage, le terminal a exécuté les scripts et relu un original de 66 560 octets et sa copie de 66 562 octets. Le support d’installation a conservé son empreinte initiale. Les deux noyaux, les journaux série et le firmware sont identifiés dans le relevé de livraison.

## Compatibilité, sécurité et coupures après fusion

Les deux architectures QEMU ont exercé les géométries de 512 et 4 096 octets. Les 56 compositions vi/VFS/service/pilote, dix inversions de géométrie, deux témoins classiques et vingt transferts de 12 ou 24 Kio ont tous produit le contenu attendu après redémarrage. Les vingt sorties de `cat` ont également été comparées octet par octet. La négociation incompatible conserve les droits accordés et le protocole ancien.

Les 928 injections DMA et 512 interruptions par décès du titulaire ont exercé le polling et le prototype IRQ. Les cadres de session ont été restitués après retrait confirmé. Lors d’un retrait incertain, le test exige simultanément une fermeture en quarantaine, le nombre exact de cadres encore épinglés et le refus d’une nouvelle ouverture. Deux reprises successives doivent ensuite retrouver le nombre initial de cadres. [Propriétaire du retrait](../kernel/src/hardware/block/ownership.rs) · [Assertions dans l’invité](../tests/dma_propriete_invite.rs) Huit essais de saturation ont vérifié la reprise des réservations. Après chacun des 256 cycles d’endurance, le compteur de processus et celui des pages IPC sont revenus à zéro. [Relevés IPC/VFS et DMA](validation/es-v0.16.6-main-compatibilite-dma.json)

| Coupure dans l’application UTM, destination de 8 Mio | Secteurs | Résultat en lecture indépendante puis au redémarrage |
|---|---|---|
| Dernier lot de pages écrit, avant F1 | 512 et 4 096 | Source intacte, destination absente. |
| F1 acquittée, avant l’ancre finale | 512 et 4 096 | Source intacte, destination absente. |
| F2 acquittée, avant la réponse à vi | 512 et 4 096 | Source intacte et destination complète. |

Le chemin classique a subi quatre interruptions QEMU après F2 du premier lot privé, une pour chaque couple architecture et géométrie. Le montage normal a relu les 135 171 octets de la source et le nom final était absent. Pendant les essais UTM, deux lancements de la cellule 512/F2 ont échoué avant le démarrage du noyau. QEMU a signalé `Operation not permitted` à l’ouverture de fichiers de la VM importée. Après relance de l’application, la cellule a réussi sur une copie fraîche. Le relevé conserve ces incidents, les générations lues et les empreintes des journaux. [Coupures et livraison UTM](validation/integration-main-v0.16.6-livraison.json)

## Régressions du système et du moteur P9

Les 86 commandes historiques, les 40 commandes supplémentaires du chemin classique et la commande de lecture/écriture P9 interversions ont réussi. Les invités QEMU 11.1.3 AArch64 et x86_64 ont chacun passé les 25 démarrages, 24 groupes bloc, 44 groupes fichier, 24 groupes d’espace de noms, les contrôles ELF/processus/IPC et les suites CLI, nsh, vi et mémoire. Le moteur de QEMU 10.0.12 livré avec UTM a exécuté les 11 cas GICv3 et les 23 cas DMA séquentiels. Les bancs QEMU spécialisés ont exercé 36 coupures et 20 fautes DMA, avec remontage et contrôle des octets. Les tests de concurrence ont exercé la modification de la source, la suppression, la réutilisation du nom et le conflit à la publication.

Sur Darwin ARM64 natif puis x86_64 sous Rosetta, le moteur portable a passé **196 tests standards et 199 instrumentés par architecture**, un million d’opérations différentielles, 2 304 injections sur les ancres et 486 sur les transactions CoW. Six tests restent marqués ignorés dans chacune des suites Cargo. Les quatre parcours binaires interversions ont écrit avec un moteur, écrit avec l’autre et relu les octets avec les deux, sur secteurs 512 et 4 096. Le format P9 est resté lisible et inscriptible dans les deux sens.

Sur le banc d’ouverture et de modification localisée d’un fichier de 4 Mio, vi atteint 111 pages privées en classique sur AArch64 (444 Kio), 112 sur x86_64 (448 Kio), et 114 en groupé (456 Kio) sur les deux architectures. Ces chiffres couvrent l’ouverture et l’édition locale. Le banc de sauvegarde mesure séparément le coût de création de plusieurs Mio de texte neuf.

Quatre premiers passages de régression ont nécessité un rejeu. Une attente du chargeur Darwin avant le lancement de la VM, un arrêt avant la bannière du noyau, un refus de montage AArch64/512 avant toute demande d’écriture et une expression régulière du banc restée à v0.16.5 sont consignés. Lors du refus de montage, 2 029 lectures bloc ont été comptées, sans écriture, FLUSH ni invite. Le service peut traduire plusieurs erreurs sous-jacentes en code 26. Le retrait DMA a abouti et les trois compteurs finaux (DMA, processus, IPC) sont revenus à zéro. Les douze sources de ce chemin sont identiques entre le candidat qualifié et le code fusionné. La cause précise reste indéterminée. Son rejeu à charge réduite a franchi les neuf coupures et les neuf remontages. L’oracle des captures de démarrage tient compte des cinq pixels logiques qui changent entre les glyphes « 5 » et « 6 ». Le banc UTM lit désormais la bannière attendue dans l’ELF de référence et la contrôle aux deux démarrages, y compris avec le témoin v0.16.5. Les profils mémoire d’amorçage qualifiés sont 128 et 256 Mio. [Résultats et incidents des 127 commandes](validation/es-v0.16.6-main-regressions.json)
