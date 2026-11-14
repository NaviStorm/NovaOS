# NovaOS v0.17.0 · Régressions de la phase 4 SMP

Le 14 novembre 2026, le candidat `a8dc2534ac9fabcca2e480d1054fd0b60f180f17` a été exercé sous QEMU 11.1.3 TCG et dans l’application UTM 5.0.8 avec QEMU 10.0.12. Les suites utilisateur se sont exécutées dans de vrais invités. Le moteur NovaFS a également été testé sur macOS 27.2 ARM64 natif et x86_64 sous Rosetta. Les données individuelles identifient les ELF, les fixtures, les codes de sortie, les essais initialement en échec et les rejeux. [Résultats des régressions](validation/smp-phase4-regressions-v0.17.0.json)

## Processus, mémoire et FPU

À quatre vCPU, la sonde historique de préemption a déclenché son assertion de cinq secondes sur ARM et x86. Les ELF de phase 3 reproduisent le même défaut. Deux travailleurs toujours prêts peuvent occuper deux CPU libres sans qu’une commutation leur soit imposée. La sonde ABI avait également besoin qu’un appel système soit préempté. Le premier passage a expiré après ses 16 messages de diagnostic. Les bancs publient désormais les travailleurs concernés avec une affinité commune avant leur première exécution. Les conditions sur le nombre de préemptions, la progression et les délais restent vérifiées. La démonstration de démarrage utilise ses affinités habituelles. Le test de migration SMP exerce séparément les transferts de CPU. [Sonde de processus](../kernel/src/sched/process/demo.rs) · [Sonde ABI](../kernel/src/sched/process/abi_probe.rs)

Les 25 démarrages historiques ont réussi dans chacune des six configurations. À quatre CPU, le rejeu complet utilise le correctif des sondes. Le profil GICv3 a également passé l’arrêt, la panique, et, lorsque plusieurs CPU existaient, les scénarios de secondaire absent et muet, aux trois topologies. Les campagnes SMP vérifient la progression de deux processus utilisateur par CPU, la migration, les invalidations TLB distantes, les timers locaux, les acquittements à l’arrêt et après une panique. À deux et quatre CPU, le secondaire absent et le secondaire muet produisent les masques de diagnostic attendus. À un CPU, il n’y a aucun secondaire à retirer, aucun destinataire d’IPI distant et aucune migration entre CPU. Les contrôles des frontières du cache des plages RAM ont été rejoués avant et après restitution du chargeur. Les six scénarios mémoire du banc historique exercent aussi les protections de page sur les deux architectures à 1/2/4 vCPU. La collecte de l’orphelin a été vérifiée jusque sous préemption du récupérateur. [Campagne SMP](../tests/smp.py) · [Arrêts et FP](../tests/smp_phase2.py) · [Frontières RAM](../kernel/src/memory/probes.rs)

Avec `user-fpu`, la phase a couvert FP64, NEON, FPCR et FPSR sur AArch64 avec GICv2 et GICv3. Sur x86, le modèle Intel avec XSAVE et XCR0=7 a exercé x87, SSE/SSE2 et AVX. Le programme compare les quatre mots de 64 bits de chacun des seize YMM à chaque tour, dont les deux mots hauts, et vérifie les registres de contrôle. Chaque architecture/topologie a exécuté deux cycles avec deux acteurs par CPU et réemploi des slots. À quatre CPU, chaque cycle a fait migrer huit acteurs. Les sondes ABI et processus SMP ont aussi vérifié les sorties EL0/ring3 et les fautes locales attendues. Le modèle `qemu64,vendor=GenuineIntel,-avx,-xsave` a exercé le repli x87/SSE2. Les profils matériels physiques, AVX-512, SVE et SME restent à qualifier séparément. [Sonde assembleur x86](../user/fp_x86_64.S) · [Sonde assembleur ARM](../user/fp_aarch64.S)

## IPC et restitution mémoire

Les cinq ordres T09 ont chacun été exécutés 10 000 fois dans chaque cellule, soit 300 000 injections à travers les six configurations. La suite contrôle le terme unique de l’attente, la génération, l’échéance et le nombre de réveils. Dans tous les invités de 128 Mio, les compteurs de cadres et d’IPC après destruction retrouvent exactement leur valeur au repos. Chaque cadre ci-dessous occupe 4 Kio. La saturation utilise 71 cadres propres à l’IPC et les restitue tous. [Suite IPC](../tests/ipc.py)

| Architecture | vCPU | Préemptions T09 | Cadres résidents au repos | En saturation | Cadres libres après destruction |
|---|---:|---:|---:|---:|---:|
| AArch64 | 1 | 1 011 | 700 | 1 181 | 29 871 |
| AArch64 | 2 | 917 | 734 | 1 215 | 29 837 |
| AArch64 | 4 | 444 | 783 | 1 264 | 29 788 |
| x86_64 | 1 | 1 047 | 742 | 1 223 | 30 428 |
| x86_64 | 2 | 1 116 | 791 | 1 272 | 30 371 |
| x86_64 | 4 | 344 | 857 | 1 338 | 30 289 |

À quatre vCPU, l’endurance IPC ARM a accompli 25 524 cycles en 301,041 secondes, avec 62 492 cadres libres à chacun des 204 relevés. Sur x86, elle a accompli 32 119 cycles en 300,028 secondes, avec 62 993 cadres libres aux 295 relevés. L’endurance de l’ordonnanceur a également retrouvé le niveau initial des cadres et du tas après 60,129 s sur ARM et 60,276 s sur x86. T17 a réussi ses 60 séries et 13 groupes d’histogrammes sur chaque architecture, avec 451 569 et 439 430 livraisons, 9 919 et 9 021 préemptions respectivement, zéro erreur.

## Services et applications

Les six cellules ont réussi les tests de cycle de vie des processus et de restitution des ELF, les 24 groupes bloc, les 44 groupes fichiers sur secteurs 512 et 4 096 octets, les espaces de noms, les commandes CLI, leurs horodatages P9, les scripts et le terminal nsh. Le scénario nsh comporte 113 commandes et celui des scripts 31 commandes par géométrie. À 512 comme à 4 096 octets, vi a relu 16 fichiers, exercé le refus concurrent et la révocation puis vérifié les octets après redémarrage. Le banc de rendu a contrôlé 20 frappes, 20 lignes, le curseur et quatre géométries dont 80 × 24. Ces essais de rendu observent le flux terminal dans QEMU. [Suite de fichiers](../tests/fichiers.py) · [Tests de vi](../tests/vi.py) · [Contrôles du rendu](../tests/vi_ecran.py)

## NovaFS, bloc et DMA

NovaFS a passé sur chaque architecture Darwin 196 tests standards, 199 instrumentés et 1 000 000 d’opérations différentielles avec 345 183 commits observés, 54 643 pénuries injectées et 1 000 remontages et audits. Les six cas marqués ignorés par chacune des suites de base figurent dans les décomptes. Les campagnes complémentaires couvrent les migrations P8/P9, snapshots, clones, chiffrement, compression, injections et contrôles de restitution. Le laboratoire d’ancres a exercé 2 304 injections par architecture. Le montage a retrouvé 1 288 états anciens, 656 nouveaux et refusé 360 images ambiguës. Le test des attestations du pool sécurisé a également vérifié six injections par géométrie et architecture, avec refus des mauvaises clés. Les arbres Git du moteur NovaFS, des composants utilisateur et de l’ABI sont identiques à la référence de phase 3. [Contrat P9 de récupération](../../novaFS/docs/novafs-p9-ancres-dechirees.md) · [Suites NovaFS](../../novaFS/lab/p51-validation-suites.py)

Les 24 groupes du banc bloc couvrent les secteurs de 512 et 4 096 octets. La campagne de propriété DMA a injecté 120 fautes par cellule ARM et 112 par cellule x86 à chacune des deux géométries, soit **1 392 injections et 768 cycles de session** sur les six configurations. Les images sont restées identiques, les portails de lecture révoqués ont refusé le transfert en préservant la trame, et les compteurs de propriété ont retrouvé leurs valeurs de référence. [Banc bloc](../tests/bloc.py) · [Campagne de propriété DMA](../tests/dma_propriete.py)

Les 36 interruptions contrôlées sous QEMU couvrent neuf frontières par architecture/géométrie. Chaque essai a remonté P9 normalement, puis vérifié la source et la destination octet par octet, avant et après un redémarrage invité. Dans les quatre séries, la nouvelle destination complète existait après F2 et lors de la suspension du FLUSH F2 ; elle était absente aux sept autres points. Les essais à 512 octets utilisaient les transferts multipages de 4 Mio ; ceux à 4 096 octets utilisaient une source de 524 416 octets. Le Mac et le stockage hôte sont restés alimentés. [Banc de coupures](../tests/coupure_cow_groupee.py)

Le moteur embarqué par UTM a exécuté séparément 11 essais du firmware DXE, reprise comprise, et 23 essais de la politique DMA séquentielle. Les traces couvrent l’attachement du SID 48, la restitution avant transfert, le SID 56 de la cible, la quarantaine et le redémarrage. Ces deux bancs configurent un vCPU, leur pflash privé et un TPM de test. Les données détaillent séparément ces deux campagnes et les exécutions dans l’application UTM. [Firmware DXE](../tests/prototype_uefi.py) · [DMA séquentiel](../tests/dma_sequentiel.py)

## UTM et sauvegarde durable

L’application UTM a démarré quatre vCPU Cortex-A72 émulés, 256 Mio de RAM, GICv3 sans ITS, TPM CRB et disque QCOW2. La sonde SMP a enregistré 1 000 tours IPC, 73 préemptions, la migration 0 → 1, les traductions distantes sur le masque `0xe` et le rendez-vous `0xf`. L’arrêt coordonné a obtenu `ack=0xf`. Le profil FP a vérifié deux cycles de huit clients et huit migrations par cycle. HMP a renvoyé quatre fois le même PID. L’échantillon de pile du moteur contient quatre threads distincts dans `mttcg_cpu_thread_fn`. L’affectation à des cœurs physiques du Mac reste inconnue. [Banc UTM](../tests/smp_utm.py)

Le scénario de sauvegarde groupée ajoute 4 Mio par des éditions multiples à une source dense de 4 Mio. Le lecteur indépendant a relu une destination de 8 Mio de SHA-256 `c82aa6dfd3dcce1bb85d1fe60dec9073ea80ae37a0465c152d7c7d41fd5d6a25` dans les trois environnements. Dans UTM, le démarrage autonome de contrôle a de nouveau vérifié la sortie DXE avec 244 droits retirés, zéro droit actif, BME et bypass nuls. La fermeture du pilote a confirmé le retrait DMA. [Banc de sauvegarde](../tests/integration_main_performances.py)

| Profil groupé 4 vCPU / 4 096 octets | Commande à la confirmation durable | F1 puis F2 | Échanges avec contrôles | Décompte élargi aux copies du prêt | Pic de cadres de session |
|---|---:|---:|---:|---:|---:|
| QEMU AArch64 | 274,004 ms | 1,448 / 0,164 ms | 56 | 64 | 9 751 |
| QEMU x86_64 | 580,703 ms | 1,580 / 0,325 ms | 56 | 64 | 9 754 |
| UTM AArch64/QCOW2 | 339,986 ms | 2,006 / 0,150 ms | 56 | 64 | 9 732 |

Chaque cellule satisfait les critères du scénario mesuré, avec 47 échanges interservices, deux FLUSH physiques et 45 requêtes d’écriture. Un essai par plateforme, exécuté pendant les régressions, ne permet pas d’estimer un percentile de stockage. Les mesures CPU/IPC comparatives utilisent leur propre banc. Le profil distribué conserve les API classiques et le polling. La fixture des E/S groupées a demandé de recaler deux lignes de contexte sur `locals_to_check` dans l’export de laboratoire. Ses ajouts de code restent identiques ; toutes les fixtures ont été appliquées sans approximation de contexte (`patch -F0`). [Fixtures et construction](../tests/reconstruire_es_0166.py)

## Reproduction

Depuis la racine du monorepo, utiliser un répertoire de sortie neuf. Les deux cibles Rust sont `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. L’exemple construit sur le commit des bancs corrigés. Les SHA exacts des premières constructions sont consignés dans le JSON. La dernière contre-épreuve à quatre vCPU a utilisé un export Git ordinaire de `d7b3544d04a7a2055902904bf3f463a1d8a9c3e8`. Elle a validé le rendez-vous SMP, IPC en 128 Mio, les cinq ordres de courses, ELF et le cycle des processus sur les deux architectures. Ce noyau avec les sondes finales occupe respectivement 797 et 858 cadres résidents au repos, puis 1 278 et 1 339 en saturation. Le lanceur accepte `--cas` pour rejouer un seul test, par exemple `--cas courses`. À quatre vCPU il inclut une endurance IPC de 300 secondes.

```sh
D="$PWD/novaOS/build/rejeu-regressions-phase4"
python3 -B novaOS/tests/reconstruire_es_0166.py ead7e98 "$D/exports" brut sonde coupe utm-sonde regression
export RUSTC="$(rustup which --toolchain stable rustc)"
(
  cd "$D/exports/brut/novaOS" || exit 1
  for cible in aarch64-unknown-none-softfloat x86_64-unknown-none; do
    rustup run stable cargo build --release --locked --offline --target "$cible" --features user-fpu --target-dir "$D/fp"
  done
  rustup run stable cargo build --release --locked --offline --target aarch64-unknown-none-softfloat --features gicv3-lab,user-fpu --target-dir "$D/gic3"
)
for n in 1 2 4; do
  python3 -B novaOS/tests/smp_phase4_regressions.py "$D/exports/brut" "$D/arm-$n" aarch64 "$n" \
    --fp "$D/fp/aarch64-unknown-none-softfloat/release/novaos" --gic3 "$D/gic3/aarch64-unknown-none-softfloat/release/novaos"
  python3 -B novaOS/tests/smp_phase4_regressions.py "$D/exports/brut" "$D/x86-$n" x86_64 "$n" \
    --fp "$D/fp/x86_64-unknown-none/release/novaos"
done
```

Les suites hôtes NovaFS utilisent `python3 -B novaFS/lab/p51-validation-suites.py aarch64`, puis `x86_64`. Pour les ancres, définir `NOVAFS_TARGET` sur l’architecture Darwin correspondante et exécuter `novaFS/lab/run test --offline --lib fs::remplacement -- --nocapture` puis le filtre `qualification_ancres`. Les procédures de construction des pflash privés et des profils de stockage figurent dans les rapports antérieurs. [Reproduction des profils UTM et FPU](smp-phase2-v0.17.0.md#régressions-et-reproduction) · [Profils d’origine](smp-v0.17.0.md#reproduction)
