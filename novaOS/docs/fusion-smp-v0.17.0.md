# NovaOS v0.17.0 — Qualification du candidat SMP avant fusion

**Fusion suspendue dans l’attente d’une nouvelle décision MOA/MOE.** Le diagnostic ci-dessous porte sur le candidat avant correction. Les essais du correctif sont consignés dans la [requalification des réservations](reservations-dma-v0.17.0.md).

Après un refus de sauvegarde par vi, les quatre profils de qualification ont conservé une réservation bloc ouverte de 256 pages. La MOA conditionne la fusion au retrait sûr des ressources. L’origine de l’incident AArch64/512 initial demeure inconnue. [Qualification finale du stockage](qualification-stockage-v0.17.0.md) · [Diagnostic historique et témoins](#diagnostic-du-refus-avant-f2)

Le candidat SMP `835b9d9dd817a2feef54b6c17f000b0aec8a3879` descend du précédent `main`, `3e14a50bb5bc7960d16c1fbcfc7f49a681d6e7c6`. Le noyau, le service NovaOS de stockage et les commandes de la construction `b76c863c9302bf6ead53fac3b80b26df0513fe0d` portent la version 0.17.0. Les sources du moteur NovaFS et de l’ABI ont les mêmes objets Git qu’au départ, respectivement `3379ccd023e821e912130c7413113fe7546070cc` et `a70d5f23ae91211dd7a54c429e723f5157600e58`. [Historique SMP](smp-v0.17.0.md) · [Phase 2](smp-phase2-v0.17.0.md) · [Résultats IPC de phase 4](ipc-smp-phase4-v0.17.0.md)

## Installation dans UTM

Sur Mac Apple Silicon avec UTM 5.0.8, Python 3.11 ou ultérieur et `qemu-img`, partir de la racine du dépôt. L’archive de 3 881 952 octets a pour SHA-256 `5930b0d6cb3d1e2307f8f630fefa52af8e76967f4f13a4c34674be8b94205f9c`. Elle contient le support AArch64, les licences et le firmware EDK2 expérimental. Le profil installable utilise un vCPU, GICv3 sans ITS, un TPM CRB et les E/S classiques par polling. [Image UTM](../../distribution/images/novaos-0.17.0-utm-aarch64.tar.gz) · [Empreintes des ELF et du support](validation/fusion-smp-v0.17.0/distribution.json)

```sh
travail=$(mktemp -d "$HOME/Documents/NovaOS-0.17.0.XXXXXXXX")
dist="$travail/novaos-0.17.0-utm-aarch64"
vm="$travail/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.17.0-utm-aarch64.tar.gz -C "$travail" &&
    (cd "$dist" && shasum -a 256 -c SHA256SUMS) &&
    python3 -B novaOS/scripts/paquet_installation.py creer "$dist" "$travail/machine"
```

1. Dans **UTM → Fichier → Ouvrir**, sélectionner le paquet créé par le script et démarrer. La console affiche `NovaOS v0.17.0`, la série `NOVA-CIBLE-000000001`, le SID 56, la capacité de 67 108 864 octets et `DISQUE vierge verifie`.
2. Pour autoriser l’installation sur **cette cible neuve**, saisir exactement `AUTORISER NOVA-CIBLE-000000001 67108864`. Attendre `INSTALLATION bilan Ok(())`, puis arrêter la VM. En cas de refus du disque, conserver celui-ci et utiliser une nouvelle cible vierge.
3. Retirer de la bibliothèque UTM le **raccourci** du paquet externe, en conservant ses données. Si UTM a importé une copie, exporter d’abord la VM installée à l’arrêt et faire pointer `vm` vers ce paquet. Exécuter `python3 -B novaOS/scripts/paquet_installation.py systeme "$vm"`. Le script relit la GPT et sélectionne le disque installé.
4. Rouvrir ce même paquet puis démarrer. À l’invite `nsh>`, exécuter `ls --version`, `cp /systeme preuve.txt` et `cat preuve.txt`. Terminer avec `exit`, attendre `SYSTEME bilan Ok(())`, arrêter et redémarrer. `cat preuve.txt` doit afficher `NovaOS`. [Contrôle du disque et du profil](../scripts/paquet_installation.py) · [Essai automatisé dans l’application UTM](../tests/nsh_utm.py)

## Garanties SMP et limites

| Propriété | Mécanisme et portée de la qualification |
|---|---|
| Exécution exclusive et progression | La garde inter-CPU protège l’extraction dans la FIFO et la transition vers RUNNING. La commutation bascule de pile avant sa libération. Les campagnes de préemption, migration, courses et équité observent les progrès de leurs acteurs. Une borne universelle de famine reste à établir. [Ordonnanceur](../kernel/src/sched/mod.rs) · [Garde](../kernel/src/sched/gate.rs) · [Équité](../tests/ipc_equite.py) |
| Attente IPC et révocation | L’état d’attente et la sélection du prochain thread se publient sous la même garde. Le réveil vérifie le cookie et la génération du thread. Le détenteur, les droits et l’incarnation sont vérifiés avant usage. Les descendants et les attentes correspondantes sont retirés lors de la révocation. [IPC](../kernel/src/sched/process/ipc/mod.rs) · [Réveils](../kernel/src/sched/process/ipc/traffic.rs) · [Banc](../tests/ipc.py) |
| Isolation mémoire et TLB | Les droits des PTE sont contrôlés avant copie. Après retrait d’une traduction, la restitution attend l’invalidation x86 acquittée par les CPU en ligne, ou la fin de l’invalidation ARM dans le domaine partagé. Le banc exerce les protections utilisateur et les traductions distantes. [Tables](../kernel/src/memory/tables.rs) · [Sonde SMP](../kernel/src/sched/smp_probe.rs) |
| DMA | Le bail lie détenteur et époque. Une commande en vol conserve ses cadres jusqu’au drainage et au retrait confirmé. Un retrait incertain les conserve en quarantaine. [Propriété](../kernel/src/hardware/block/ownership.rs) · [Injection de défauts](../tests/dma_propriete.py) |
| FPU/SIMD | Le profil explicite `user-fpu` initialise un état privé par thread et le sauvegarde aux commutations. ARM couvre FP64/NEON et FPCR/FPSR, x86 couvre x87/SSE2 et les registres YMM lorsque XSAVE et AVX sont admis. AVX-512, SVE et SME sont hors du profil. Les modèles émulés ne qualifient aucun PC physique. [État x86](../kernel/src/arch/x86_64/fpu.rs) · [État ARM](../kernel/src/arch/aarch64/fpu.rs) · [Sondes FP et arrêt](../tests/smp_phase2.py) |
| Arrêt | Le coordinateur attend les acquittements, diagnostique les CPU absents ou muets à l’expiration du délai et conserve les ressources encore susceptibles d’être utilisées. Les essais d’arrêt, de panique et de secondaires indisponibles vérifient les masques attendus. [Coordination](../kernel/src/smp.rs) |

La distribution utilise le profil softfloat, les E/S classiques et le polling VirtIO-blk. Les E/S groupées, l’attente par IRQ et la FPU utilisateur sont des profils explicites de qualification. NovaFS conserve la publication CoW P9 avec F1 pour les dépendances, puis F2 pour l’ancre. Une coupure de VM sur Mac alimenté borne la portée des essais de durabilité. PC1 et le remplacement transactionnel restent désactivés. [Contrat NovaFS et DMA](es-v0.16.6-integration.md) · [Ancres P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

## Image installée et écrans

Les 27 entrées du manifeste ont été relues après extraction de l’archive. UTM 5.0.8 a installé le système sur une cible QCOW2/512 neuve, puis l’a démarré deux fois depuis le seul disque installé. La série a annoncé `NovaOS v0.17.0`, et `ls --version` ainsi que `rm --version` ont renvoyé 0.17.0. Le banc a exercé la complétion, les commandes de session, les scripts, les modes de vi, deux annulations et deux rétablissements. Au second démarrage, la comparaison du terminal avec les données attendues a couvert les 66 560 octets de l’original et les 66 562 de la copie éditée. Le temps de réouverture de l’original dans vi était de 24,5 ms au premier démarrage et de 206,0 ms au suivant. La procédure a aussi vérifié l’identité du support d’installation en lecture seule avant et après l’essai. [Résultats et empreintes série](validation/fusion-smp-v0.17.0/installation-utm.json)

Sous QEMU, quinze captures de 800 × 600 pixels correspondent aux cinq nouvelles empreintes de référence pour les trois topologies. Sur les cinq scènes à quatre vCPU, 384 pixels par scène ont changé, tous dans la bannière. L’essai de distribution UTM a utilisé le terminal série de l’application. [Empreintes QMP](validation/fusion-smp-v0.17.0/ecrans.json)

Dans trois autres VM UTM AArch64/QCOW2 à quatre vCPU, le profil GICv3 avec FPU a validé le rendez-vous des quatre processeurs, la conservation des registres après huit migrations au deuxième cycle, et l’arrêt avec les quatre acquittements. HMP a identifié les CPU 0 à 3. Cette campagne n’a pas relevé les piles des threads Darwin ni leur placement physique. [ELF, marqueurs invités et empreintes des traces](validation/fusion-smp-v0.17.0/utm-smp.json)

## Corrections de deux bancs hôtes

Le lecteur de traces DXE reconnaissait les bannières jusqu’à 0.16.6. Il refusait la nouvelle version après le retrait complet du domaine UEFI. L’extension explicite à 0.17.0 conserve les contrôles d’ordre, de droits, de libération et d’IRQ. Les onze cas ont passé avec le lecteur `0f0371dc6b5fb6f698b5c1a376288425d7a99296`. [Contrôles de trace](../tests/prototype_traces.py)

Dans la fixture QoS, la révision SMP `835aed6` a rendu visible l’absence de `System.CPUCount` dans la configuration fictive. Le parent `8a42b72` réussissait les sept tests. L’ancien `main` rencontrait un autre défaut, le noyau fictif manquant. La correction `e7bbd0efd90d8624813a2f2f9e97a9b0f5da4874` déclare un vCPU dans la fixture. Les sept tests ont réussi, y compris le refus de sauvegarde et le contrôle de relecture. [Banc QoS](../tests/test_qos_utm.py)

## Sauvegarde, révocation et récupération

Chaque ligne provient d’une sauvegarde distincte avec compteurs sur quatre vCPU et secteurs de 4 096 octets. Le profil groupé ajoute 4 Mio à une source de 4 Mio à plusieurs positions. Le temps commence à la commande et se termine à l’acquittement du succès. La destination de 8 Mio a été remontée et comparée au contenu attendu, SHA-256 `c82aa6dfd3dcce1bb85d1fe60dec9073ea80ae37a0465c152d7c7d41fd5d6a25`. Une observation par environnement permet de vérifier le seuil dans ce scénario. Les queues de distribution dépendent notamment de la charge du Mac. [Mesures individuelles, compteurs et preuves de relecture](validation/fusion-smp-v0.17.0/stockage.json) · [Définition du compteur IPC](../tests/es_groupees_verifier_mesures.py)

| Environnement | Commande au succès | IPC et dons | Avec contrôles | Avec les 8 copies mémoire locales | F1 / F2 | Pic de session, cadres de 4 Kio |
|---|---:|---:|---:|---:|---:|---:|
| QEMU AArch64, brut | 307,957 ms | 47 | 56 | 64 | 1,411 / 0,456 ms | 9 751 |
| QEMU x86_64, brut | 757,581 ms | 47 | 57 | 65 | 1,573 / 0,542 ms | 9 754 |
| UTM AArch64, QCOW2 | 317,807 ms | 47 | 56 | 64 | 1,735 / 0,198 ms | 9 732 |

La fermeture supplémentaire sur x86 porte le total contractuel à 57 sur 64 autorisés. L’addition des copies locales fournit une mesure séparée du travail mémoire. Les latences ci-dessus s’appliquent au profil groupé de laboratoire. Le chemin classique a servi à installer la distribution et à enregistrer les fichiers relus après redémarrage dans UTM.

La campagne a exercé 1 392 injections DMA et 768 sessions, le polling et le profil IRQ expérimental. Les essais DXE et DMA séquentiels ont couvert respectivement 11 et 23 scénarios. Les 36 coupures QEMU couvrent neuf frontières sur chacune des quatre combinaisons architecture/secteur. L’état du disque après chaque coupure armée a été vérifié au remontage. Les coupures physiques du Mac et l’atomicité générale des écritures de secteur restent hors du domaine testé.

Sur Darwin ARM64 natif puis x86_64 sous Rosetta, le moteur P9 a passé 196 tests standards, 199 instrumentés et 1 000 000 d’opérations différentielles par architecture, ainsi que 2 304 injections sur les ancres par architecture. Les six tests marqués ignorés dans chacune des suites Cargo sont consignés dans le relevé.

**Incident AArch64/512.** Au premier passage, dans la huitième cellule de la série de neuf frontières, vi a affiché une erreur d’E/S après `OUI`, alors que le banc attendait encore le marqueur permettant d’armer l’interruption pendant F2. QMP n’avait pas injecté la panne et vi n’a jamais confirmé la sauvegarde. Le statut public regroupe plusieurs erreurs de transport et d’indisponibilité. Le disque temporaire a été libéré à la sortie du banc, donc son état final et la cause sous-jacente restent inconnus. Le rejeu des neuf frontières a réussi en 157,030 s. Trois passages supplémentaires sur `835b9d9`, trois sur `b76c863` et un témoin sans injection sur le binaire versionné ont gardé leurs images. À la relecture indépendante, les sept sources et les sept destinations étaient complètes, octet pour octet. Ces observations bornent l’incident sans en établir la cause. [Essais appariés](validation/fusion-smp-v0.17.0/coupure-f2-appariee.json) · [Traduction des erreurs](../user/novafs/files.rs)

## Diagnostic du refus avant F2

La trace initiale contient la confirmation du nom de destination, le retour de vi en mode normal avec un tampon modifié et le message associé au statut 26. La série s’arrête sans marque d’armement de F2, sans erreur imprimée de longueur ou de statut VirtIO et sans succès de sauvegarde. Le pilote, le transport, le code d’erreur inférieur et le rang de la barrière ne figuraient pas dans le message. La consultation ciblée des erreurs de stockage de QEMU et du noyau macOS autour de l’incident n’a renvoyé aucune entrée. [Série originale](validation/fusion-smp-v0.17.0/refus-initial.serial.gz) · [Emplacement de l’armement dans la fixture](../tests/fixtures/io-0166-barrieres.patch)

Dans le service de stockage, `Error::Unavailable` et `Error::Io(_)` deviennent `IO` (26). Le moteur renvoie les erreurs de préparation et de F1 directement. Une erreur à l’écriture de l’ancre ou pendant F2 devient `Error::Publication`, qui remonte comme issue incertaine. Le journal original permet de localiser le refus en amont de la confirmation de F1 dans le chemin normal de publication. Il ne permet pas de savoir quelle opération a envoyé le premier statut ni si le premier FLUSH a atteint le support. [Classement des erreurs](../user/novafs/files.rs) · [Publication CoW](../../novaFS/src/engine.rs) · [Compte rendu des sondes](validation/fusion-smp-v0.17.0/diagnostic-incident.json)

| Témoin avec panne introduite sur une image privée | Premier évènement constaté | Retour observable par vi |
|---|---|---|
| Suspension de la première écriture de données | La file VirtIO reste au même indice `used` après soumission. Le pilote expire sur l’écriture et remonte 12. | 26, avant l’armement de F2 |
| Suspension du FLUSH F1 | Même expiration sur la commande FLUSH, après écriture des données. Le pilote remonte 12. | 26, avant l’armement de F2 |
| Rétention de la requête par le relais avant envoi au pilote | Le service attend la réponse du lot de 256 pages. Son délai de 506 ticks expire et il reçoit 8. | 26, avant l’armement de F2 |

Les trois disques des témoins ont remonté la génération 67, avec la source intégrale et la destination absente. Les différences internes viennent des sondes dans les seuls exports de laboratoire. Elles expliquent pourquoi le statut public de l’incident historique ne permet pas une attribution rétrospective.

Le même scénario a été exercé sur 56 images avec observation des échanges bloc, sans faute introduite avant l’armement. Les départs ont été synchronisés en groupes de 4, 12 et 20 VM avec sondes du service, puis de 20 VM avec sondes supplémentaires au noyau. Les 56 VM ont atteint l’armement, et leurs volumes ont passé la vérification indépendante après coupure. Ces résultats et la charge observée sont détaillés individuellement. [Rejeux, compteurs QMP et empreintes](validation/fusion-smp-v0.17.0/rejeux-diagnostiques.json)

Le banc conserve maintenant, à côté du rapport, le disque et la série de chaque passage en échec. La série de la reprise est également conservée si celle-ci a démarré. Le banc a été vérifié en provoquant une interruption du test avant `OUI`, puis en conduisant une véritable coupure en vol F2 jusqu’au remontage. Les copies de série gardent les emplacements historiques utilisés par les autres scripts. [Banc de coupures](../tests/coupure_cow_groupee.py) · [Terminal du banc](../tests/cli.py)

**Arbitrage au commit `e08b979`.** La MOA avait alors demandé de retrouver la première erreur de l’essai d’origine. Son mandat suivant autorise l’acceptation de la cause inconnue sous neuf conditions de sûreté. Le test du cycle de vie des réservations a échoué sur ce candidat et a suspendu la fusion. [Décision et résultats de la dernière campagne](qualification-stockage-v0.17.0.md)

### Reproduire la collecte sur une nouvelle image

Les deux jeux de sondes d’observation s’appliquent dans un **export jetable du profil coupe**. Les SHA-256 des patches décompressés et des ELF effectivement utilisés figurent dans le rapport causal. Depuis la racine du candidat local, exporter les sources du profil avec les versions finales du banc afin de conserver les images en échec. Les sorties de diagnostic utilisent la révision de code invité `b76c863` et les sondes sur erreur. [Sondes du service](validation/fusion-smp-v0.17.0/sondes-service.patch.gz) · [Sondes bloc](validation/fusion-smp-v0.17.0/sondes-bloc.patch.gz)

```sh
set -e
repere="$PWD/novaOS/docs/validation/fusion-smp-v0.17.0"
sortie=$(mktemp -d)
python3 -B novaOS/tests/reconstruire_es_0166.py HEAD "$sortie/exports" coupe
(
    cd "$sortie/exports/coupe"
    gzip -dc "$repere/sondes-service.patch.gz" > "$sortie/service.patch"
    gzip -dc "$repere/sondes-bloc.patch.gz" > "$sortie/bloc.patch"
    patch -p1 -F0 < "$sortie/service.patch"
    patch -p1 -F0 < "$sortie/bloc.patch"
    cd novaOS
    export RUSTC="$(rustup which --toolchain stable rustc)"
    rustup run stable cargo build --offline --locked --release --target aarch64-unknown-none-softfloat
    NOVA_VCPUS=4 NOVA_QUALIF_VCPU=4 python3 -B tests/coupure_cow_groupee.py aarch64 512 \
        "$sortie/rapport.json" --barrieres --dma-en-vol --multipage --taille 4194304
)
```

Un échec avant `COUPURE groupe armer` conserve les séries et le disque à côté du rapport. Relier le premier statut du transport à l’opération, au tag IPC, au compteur de la virtqueue, au rang de la barrière et au bilan P9 après arrêt. Pour reproduire le profil ordinaire du banc, utiliser l’export `coupe` sans lui appliquer les deux jeux de sondes.

Cette procédure a été exécutée depuis l’export du commit `8aafb94c1671204d4451558707b8cc71d5e6f7fc`. Les neuf frontières, dont les interruptions pendant F1 et F2, ont atteint leurs marqueurs et passé le contrôle de relecture après remontage en AArch64/512. [Validation du banc corrigé](validation/fusion-smp-v0.17.0/verification-banc.json)

## Régressions du candidat versionné

Avec rustc 1.99.1 et QEMU 11.1.3 TCG sur macOS 27.2, les **163 groupes de la campagne système** ont réussi au premier passage sur les ELF de `b76c863`. Les échecs initiaux décrits plus haut appartiennent aux campagnes distinctes de stockage et de vérification des bancs. [Détail de chaque groupe, profils et empreintes](validation/fusion-smp-v0.17.0/systeme.json) · [Lanceur système](../tests/smp_phase4_regressions.py)

| Mesure | AArch64 | x86_64 |
|---|---:|---:|
| Groupes réussis à 1 / 2 / 4 vCPU | 26 / 30 / 32 | 23 / 25 / 27 |
| Scénarios de démarrage, bloc, fichiers par topologie | 25 / 24 / 44 | 25 / 24 / 44 |
| Injections concurrentes IPC, trois topologies | 150 000 | 150 000 |
| Endurance IPC à quatre vCPU | 30 067 cycles en 301,109 s | 29 583 cycles en 300,151 s |
| Cadres libres stables pendant l’endurance IPC | 62 478 | 62 992 |
| Endurance de l’ordonnanceur avec restitution finale | 64,093 s | 60,825 s |

Les suites ont exercé ELF, le cycle des processus, la mémoire, les capacités, les espaces de noms, les commandes, les scripts, le terminal et vi. Les six essais d’équité ont suivi 78 acteurs sur 180 intervalles, sans alerte d’attente supérieure à une seconde. Deux autres essais ont révoqué le dernier droit du slot 31, réattribué le slot et vérifié le refus de l’ancien droit. Ces résultats portent sur les durées, les charges et les topologies indiquées.

### Débit IPC du nouvel ELF x86_64

Dans 12 démarrages alternés à quatre vCPU et 256 Mio, le banc a enregistré 84 lots après chauffe. Deux échantillons consécutifs avant chaque démarrage ont mesuré au moins 75 % de CPU hôte inactif. Sur quatre couples épinglés, le débit médian du témoin de phase 4 est de 19 267 tours/s, contre 18 778 pour le candidat, soit −2,54 %. Les trois médianes par redémarrage du candidat sont inférieures aux trois témoins appariés. Pour un couple libre, la latence médiane passe de 44,24 à 46,21 µs, soit +4,47 %. Les douze démarrages n’isolent ni un effet de placement de code ni une cause d’ordonnancement. Le débit à quatre couples demeure supérieur au repère historique de 17 187 tours/s. [Mesures individuelles, horloges invité et hôte](validation/fusion-smp-v0.17.0/ipc-apparie.json)
