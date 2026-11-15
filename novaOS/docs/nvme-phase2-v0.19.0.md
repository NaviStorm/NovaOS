# NovaOS v0.19.0 — Phase 2 NVMe sous QEMU

La chaîne de fichiers NovaFS P9 a été exercée sur un namespace NVMe virtuel inscriptible, avec des capacités séparées READ, WRITE et FLUSH. Le backend est réservé aux profils `nvme-rw-*` de la branche `codex/nvme-0190`. Le système ordinaire utilise VirtIO-blk et le polling. [Contrat du contrôleur](nvme-phase2-contrats.md) · [Service bloc et fichier](nvme-phase2-service-bloc.md) · [Phase 1](nvme-phase1-v0.19.0.md)

## Architecture exercée

Le client émet les requêtes de fichiers MOUNT, OPEN, WRITE, SYNC, READ et CLOSE sur sa session. Le service NovaFS utilise le moteur v0.9.13 et le protocole bloc existant. Le service bloc reçoit les pages, obtient le ticket de la commande et demande au pilote Ring 3 de le soumettre. Ces quatre processus ont chacun leur espace d’adresses. Le courtier privilégié valide l’autorité, gère les registres et construit les descripteurs NVMe. Le backend requiert une sélection au démarrage. L’essai passe par le client du protocole fichiers du dépôt. [Opérations du client](../user/storage/fileclient.rs) · [Titres et soumission effective](../kernel/src/sched/process/nvme/broker.rs)

Le profil fixe Q35 admet la fonction PCI `00:06.0`, son SID 48 et le namespace 1 d’un contrôleur de série `NOVA0190RW`. L’amorçage exige la remise d’un contrôleur inactif par le firmware. Le RAW mesure 4 Mio et les 1 023 premiers blocs de 4 Kio forment le volume P9. La dernière page contient une garde non nulle hors bail. Identité, limites LBA, longueur, génération, titulaire, numéro de ticket et droit sur l’opération sont revérifiés avant la soumission. Une source WRITE préparée est immuable. Le contrôleur reçoit une page privée alignée par commande, au plus huit secteurs de 512 octets ou un secteur de 4 096 octets. [Contrôles du backend](../user/nvme_rw_block.rs) · [Profils](../kernel/src/sched/process/nvme/profile.rs)

## Conditions de mesure et preuves

Le code invité testé est le commit `164a98677c9d9640a9f6f4e5aa86c8cbd1253793`. Le noyau de la matrice a pour SHA-256 `2303c41646c3574f588e3acbe2d53aaa09d4d7ff4c979314566faa5828a96443`. L’hôte est un Mac ARM64 sous macOS 27.2 (26B123), sur APFS. L’invité est QEMU 11.1.3, machine `pc-q35-11.1`, TCG, CPU `max`, 256 Mio, Intel VT-d, démarrage EDK2 depuis ISO. QMP confirme les deux tailles de secteur annoncées, le cache writeback activé, les accès directs désactivés et `no-flush=false`. Le lecteur NovaFS natif de l’hôte inspecte les deux ancres, vérifie le volume et extrait les octets de la génération sélectionnée. Une nouvelle VM remonte le même RAW en lecture seule et vérifie le fichier. Chaque relecture conserve le SHA-256 du RAW entier. [Matrice et identités des binaires](validation/nvme-phase2-qemu.json) · [Oracle de contenu et de chronologie](../tests/nvme_phase2_oracle.py)

| Essai | Couverture | Observation |
|---|---|---|
| Création, modification, sauvegarde et remontage | 1, 2 et 4 vCPU × 512 et 4 096 octets | Les six sauvegardes acquittées produisent la génération 4 et le fichier de 4 231 octets attendu. |
| Coupure de VM | Huit points × deux géométries, 2 vCPU | Les seize redémarrages donnent un contenu complet et conservent l’ancre de génération 3. |
| Retrait pendant WRITE et FLUSH, mort du pilote et du client | Chaque cas à 1, 2 et 4 vCPU, secteurs de 512 octets | Aucune confirmation applicative. La génération 3 reste lisible. |
| Concurrence, migration et copie de source modifiée après préparation | 1, 2 et 4 vCPU, 512 octets | Le second handle d’écriture du même client reçoit BUSY. Les octets effectivement publiés sont ceux de la source figée. La migration entre CPU est observable à 2 et 4 vCPU. |
| Lecture seule, expiration, faute DMA, QI en erreur et reçu WAIT retardé | Un essai par cause, 2 vCPU, 512 octets | Refus des opérations sans droit, contrôle de la panne et de l’état matériel avant restitution. |
| EIO du backend pendant WRITE, F1 et F2 | Trois essais, 2 vCPU, 512 octets | Le rapport associe la CQE au PID du pilote, au service bloc, au service NovaFS et au client. Aucune sauvegarde annoncée. |

Les 45 cellules ont utilisé 92 démarrages de VM, dont deux créations de volume et 45 remontages en lecture seule. Les temps hôte de la VM de modification, démarrage et arrêt compris, sont les suivants. Le timeout injecté a donné un retour TIMED_OUT au client et un temps total de VM de 9,166 s. L’échéance matérielle du profil est de cinq secondes.

| vCPU | Secteurs de 512 octets | Secteurs de 4 096 octets |
|---:|---:|---:|
| 1 | 2,465 s | 2,464 s |
| 2 | 2,787 s | 2,777 s |
| 4 | 2,652 s | 2,682 s |

### Publication vérifiée à chaque barrière

Le fichier initial `/sortie` contient six octets (`ancien`), génération 3. Sur un cycle normal, les traces relient sept WRITE de 4 Kio à trois FLUSH du namespace. Une FLUSH d’ouverture précède la transaction. Les six WRITE de préparation sont suivies de F1 (CID 32), de la WRITE de l’ancre (CID 33) et de F2 (CID 34). L’application confirme la sauvegarde après le CQE de F2. La relecture du contenu de 4 231 octets donne `9d691dbec9d08e43fb398131d7e4a7f53c0086e2b9380a652f3ab68c558ad849`. L’ancre précédente, ses identifiants et son empreinte sont contrôlés indépendamment sur chaque image. Les CID observés des trois injections EIO sont respectivement 26, 32 et 34. Le pilote et le service bloc retournent IO (26) et conservent le statut matériel `0x0280`. L’erreur WRITE ou F1 laisse la génération 3. En cas d’échec F2, le client reçoit UNCERTAIN (22) avec ce même diagnostic et retrouve ici la génération 4 au redémarrage. Le compteur frontal FLUSH de QMP demeure à zéro dans cette version de QEMU. Les commandes, CQE et points d’arrêt du backend fournissent les observations de barrière. [Trace et acquittement applicatif](../tests/nvme_phase2.py) · [Explication du compteur QEMU](nvme-phase2-contrats.md)

| Moment de la coupure | État observé après remontage en 512 et 4 096 octets | Retour de SYNC au client |
|---|---|---|
| Avant, pendant ou après la première WRITE | Génération 3 | Aucun |
| Pendant ou après F1 | Génération 3 | Aucun |
| Pendant la publication de l’ancre, commande suspendue | Génération 3 | Aucun |
| Pendant F2, commande suspendue | Génération 4 | Aucun |
| Après le CQE de F2, avant le retour à l’application | Génération 4 | Aucun |

Pour les quatre points « pendant », le banc observe le dispatch NVMe puis attend la suspension QEMU sur `write_aio` ou `flush_to_disk` avant de tuer le processus VM. Les autres points utilisent une sonde du service NovaFS. Une génération 4 lisible pendant F2 montre qu’un changement peut survivre sans que l’appelant ait reçu de succès. Une erreur de F2 exige donc le statut de publication incertaine. Ces coupures laissent le Mac alimenté et conservent son cache. Elles ne qualifient ni une perte électrique ni les écritures de secteur déchirées. [Limites de récupération P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

### Isolation, mémoire et retrait

Le banc vérifie les refus avant publication d’une commande illégitime. Il présente une capacité inadéquate, un namespace étranger, une plage hors bail, une adresse invalide, un transfert trop grand, une deuxième préparation du même ticket, un ticket annulé et une ancienne époque. La garde hors bail conserve la même empreinte. Six cadres NVMe sont épinglés sur un bail inscriptible (24 Kio), cinq pour la récupération en lecture seule (20 Kio). Le domaine VT-d utilise huit autres pages, soit 32 Kio. Après fermeture normale, seules ces huit pages de service restent épinglées. Les incidents de retrait du bail inscriptible en ont conservé quatorze, soit 56 Kio au total. Les quatre espaces utilisateur sont contrôlés pour exclure tout alias DMA. Le tampon intermédiaire du ticket WRITE est borné à 4 Kio.

Le retrait normal ferme BME, retire les traductions et attend le reçu d’invalidation avant restitution. En cas d’incertitude, BME est fermé et les cadres demeurent épinglés. Le témoin d’invalidation erronée laisse FSTS à 16, sans reçu. Lorsque le reçu arrive après l’échéance, l’hôte peut ensuite le lire sans que le noyau restitue rétroactivement les pages. Les deux incidents QI surviennent pendant la fermeture, après F2 et après la confirmation du fichier. La génération 4 reste lisible. Treize commandes suspendues puis relâchées après fermeture de la session ont produit une CQE tardive de succès, sans acquittement client. Une CQE arrivée après révocation ne rouvre aucun ticket. Lors du refus de lecture DMA de la page source (SID 48, raison 6), la page privée et les ancres sont restées intactes. La comparaison des RAW montre 100 octets modifiés dans le seul bloc candidat CoW, le bloc P9 numéro 2. La génération confirmée 3 et son fichier sont identiques après remontage. Une commande déjà publiée peut donc avoir affecté le média, même lorsque NovaOS refuse sa complétion. [Tests DMA et modèles](validation/nvme-phase2-materiel.json) · [Observation des tables et des reçus](../tests/nvme_domaine.py)

### Régressions ciblées

- Quatorze tests du modèle NVMe sur Darwin ARM64 natif et quatorze sous Rosetta x86_64. Ils couvrent notamment phase et CID, CQE tardive ou incorrecte, refus de source et borne de page. [Résultats matériels](validation/nvme-phase2-materiel.json)
- Trois profils Q35 EDU, dont le retrait normal avec 10 scénarios, 6 IRQ de faute et 22 invalidations acquittées. Deux témoins NVMe de phase 1 couvrent le bail de lecture et le refus DMA. [Régressions VT-d et NVMe](validation/nvme-phase2-materiel.json)
- Dans chacun des invités ordinaires AArch64 et x86_64, 17 cas IPC et la session classique à huit processus. Quatre cycles VirtIO de persistance (deux architectures × deux géométries) comprennent huit démarrages et une vérification de l’image entière. Les dix profils d’E/S groupées ont également été reconstruits par application des correctifs sans tolérance de contexte. Deux lignes de contexte de la sonde historique du service bloc ont été retirées pour permettre son application à cette révision. Les lignes de code ajoutées et supprimées par la sonde sont identiques à celles du commit de départ. Pour retrouver son archive et son SHA-256 historiques, consulter la branche au commit `2ddf61196bb3ed8d7bb93e30071897cb47ea6233`. [Invités](validation/nvme-phase2-materiel.json) · [Persistance VirtIO](validation/nvme-phase2-virtio.json)

### Défauts relevés pendant la mise au point

La création de toutes les racines IPC sous un même processus dépassait son quota de quatre endpoints. Les racines des délégations WRITE et FLUSH sont désormais réparties entre les processus déjà présents, avec des handles SEND dérivés limités au service bloc. Les limites par processus restent celles du noyau. Le mode du profil était aussi relu après la restitution de la mémoire de démarrage. Le lanceur le capture désormais avant cette restitution. Enfin, le courtier traduisait une CQE matérielle en refus générique. Le transport de qualification transmet le CID et le statut NVMe à chaque étage, avec le code public IO pour une erreur d’écriture ou de F1 et le code UNCERTAIN pour une erreur pendant la publication F2. [Câblage IPC](../kernel/src/sched/process/ipc/nvme_port.rs) · [Diagnostic de complétion](../user/nvme_diagnostic.rs)

Quatre tests de l’oracle hôte exercent les contre-exemples de garde altérée, publication incomplète, WRITE en dehors du bail et modification de secteurs hors de la requête fautive. [Tests de l’oracle](../tests/nvme_phase2_oracle_test.py)

## Reproduire

Depuis la racine, avec Rust stable, la cible `x86_64-unknown-none`, QEMU 11.1.3, Python 3, xorriso, les firmwares EDK2 et Limine disponibles comme en phase 1. Les répertoires de résultats doivent être neufs. Le banc crée exclusivement ses propres RAW et conserve les images, les journaux et les traces locaux. [Préparation commune](nvme-phase1-v0.19.0.md#reproduire-depuis-la-racine-du-dépôt)

```sh
cd novaOS
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
  --release --locked --target x86_64-unknown-none --features nvme-lab \
  --target-dir target/nvme-phase2
rustup run stable cargo build --release --locked --manifest-path ../novaFS/Cargo.toml \
  --target aarch64-apple-darwin --target-dir ../novaFS/target/phase2-oracle --bin novafs
ELF=target/nvme-phase2/x86_64-unknown-none/release/novaos
LECTEUR=../novaFS/target/phase2-oracle/aarch64-apple-darwin/release/novafs
python3 -B tests/nvme_phase2_oracle_test.py
python3 -B tests/nvme_phase2.py --elf "$ELF" --lecteur "$LECTEUR" \
  --point during-f1 --vcpus 2 --secteur 512 --sortie build/nvme-f1-essai
python3 -B tests/nvme_phase2.py --elf "$ELF" --lecteur "$LECTEUR" \
  --jobs 3 --sortie build/nvme-phase2-matrice
python3 -B tests/nvme_phase2_rapport.py build/nvme-phase2-matrice \
  --lecteur "$LECTEUR" --rapport build/nvme-phase2-matrice/rapport-verifie.json
```

## Limites et décision suivante

La qualification porte sur un seul namespace virtuel, un seul ticket, une paire de files d’E/S et des commandes de 4 Kio maximum. La reprise en écriture après une quarantaine nécessite un nouveau démarrage et une nouvelle admission. Le banc n’exerce aucun SSD physique, aucun partitionnement, aucun démarrage autonome depuis NVMe et aucune interface graphique. L’endurance IPC, le million d’opérations différentielles NovaFS et les campagnes complètes de coupures VirtIO se situent hors du périmètre ciblé. Le manifeste du noyau reste à v0.18.0 pendant ces travaux de branche. La branche est soumise à l’examen MOA/MOE pour décider de la suite du pilote virtuel. Une qualification sur matériel physique demanderait un mandat et un banc distincts.
