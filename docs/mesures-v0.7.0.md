# Mesures physiques et validation de NovaOS v0.7.0

## Reproduire

Depuis le dépôt sur macOS ARM64, les suites démarrent de vraies VM UEFI sous QEMU TCG. Le même ELF sert à l’image de démarrage et aux sondes. Le banc contrôle la quantité de RAM configurée via QMP. Il écrit les mesures dans `build/abi-<architecture>-<Mio>.json` et conserve le journal série correspondant.

```sh
make ARCH=x86_64 test
make ARCH=aarch64 test
make ARCH=x86_64 ram-test
make ARCH=aarch64 ram-test

make ARCH=x86_64 run    # VGA et clavier AZERTY, diagnostics COM1
make ARCH=aarch64 run   # ramfb et diagnostics PL011
```

## Périmètre des compteurs

L’unité élémentaire est le cadre physique de 4 Kio. Le gestionnaire compte toutes les allocations et restitutions de tables, y compris les racines, les tables temporaires et les branches annulées après pénurie. À chaque bilan, le noyau confronte le nombre initial de cadres distribuables moins les cadres libres à la somme des tables, piles de threads, cadres du Buddy et pages utilisateur. Un écart fait échouer la sonde.

- **Image** couvre les pages de l’ELF chargé, y compris BSS, piles d’urgence statiques, tables de descripteurs et programme utilisateur embarqué. La copie privée de ce programme dans chaque processus est comptée avec ses pages utilisateur.
- **Piles** compte les pages mappées des piles superviseur de threads et des piles utilisateur. Chaque pile superviseur engage 64 Kio. Chaque pile utilisateur engage 4 Kio. Les pages de garde restent absentes.
- **Tas** compte séparément les pages de données engagées et les pages de métadonnées du Buddy. Le champ `cache` mesure le suffixe et les métadonnées que pourrait restituer une purge. Ces pages sont déjà incluses dans le tas. Le bilan physique ne purge pas le cache pour faire baisser la mesure.
- **Métadonnées de processus** mesure en octets la table fixe dans l’image et le volume de ses enregistrements occupés. La capacité allouée du vecteur de TCB est indiquée séparément et déjà incluse dans le tas. Ces trois nombres décrivent les catégories précédentes.
- **Vidéo** distingue les pages du framebuffer de la RAM du micro-noyau. Sur Q35, la VGA standard utilise de la VRAM. Sur virt, le ramfb occupe de la RAM système et entre dans le total physique retenu.
- **Total retenu** comprend l’image, les cadres effectivement attribués, toute réservation ELF supplémentaire, les cadres de Limine encore retenus et la vidéo adossée à la RAM système. Les plages ACPI et réservées au firmware demeurent indisponibles. La mémoire du processus QEMU sur le Mac est une autre mesure.

Sur les deux cibles, la fenêtre virtuelle des données du Buddy réserve **256 Mio**. Sa fenêtre de métadonnées peut couvrir 16 Mio supplémentaires. Les mappings HHDM sont des alias de cadres existants. Seules leurs tables ajoutent une consommation physique. Les trous entre les adresses privées des processus et les gardes n’allouent aucun cadre.

## Restitution de la mémoire d’amorçage

Le premier bilan est pris après activation des tables NovaOS, alors que la pile et les réponses de Limine restent vivantes. Sur la pile privée du thread principal, le noyau copie la ligne de commande, la géométrie vidéo et la carte mémoire. Il vérifie que les pixels sont couverts par une plage framebuffer, que les plages récupérables sont alignées et qu’elles ne recouvrent aucune autre entrée. Les accès publics aux réponses du chargeur sont fermés avant toute réutilisation des cadres.

Le gestionnaire publie la carte possédée et transfère chaque cadre récupérable non nul à sa liste libre. Son curseur initial continue de distribuer uniquement les régions USABLE d’origine. Un cadre attribué est remis à zéro avant exposition. Une pénurie empêchant la copie de la carte laisse les cadres Limine en réserve. Les mesures au repos sont prises après le passage de relais. [Garanties du protocole Limine sur les réponses, piles et cartes mémoire](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md)

## Sécurité exercée

Le programme utilisateur appelle les six opérations depuis le niveau de privilège inférieur. Le contrôleur vérifie son PID et son TID, les motifs de registres autour d’une cession, le résultat et les erreurs de chaque appel, la longueur écrite et le quota de lignes série. La sonde alterne identifiants valides, opérations inconnues et versions invalides jusqu’à observer une commutation matérielle pendant un appel. Chaque série comprend trois appels natifs.

Une copie valide a traversé deux pages RW. Après retrait de la seconde page, les lectures et écritures ont échoué avec leurs destinations intactes. Les tests couvrent également une adresse noyau, un trou, une destination RX, une plage débordant u64, un mauvais alignement, des tailles invalides, un diagnostic contenant des contrôles et la lecture à cheval entre code et données.

Un tiers a présenté la clé exacte détenue par un autre processus et reçu le refus prévu. Le banc a exercé l’attente bloquante, un objet déjà terminé en faute, la consommation unique du droit et son abandon lors de la terminaison du détenteur. Une destination valide a été retirée pendant une attente bloquée. Le noyau a rejeté cette destination au réveil, conservé le droit et accepté une nouvelle destination. Un SP invalide et un PC de reprise arrivant sur la page NX ont terminé uniquement les processus de sonde.

## Résultats du 10 novembre 2026

Rust stable 1.99.1 et QEMU 11.1.3, émulation TCG sur Mac ARM64. Les deux commandes `make test` ont chacune validé 23 démarrages à 256 Mio. La vérification comprend les anciens scénarios de MMU, de fautes utilisateur confinées, de pénurie, de corruption du Buddy, de mutex et de préemption, ainsi que la comparaison des captures de démarrage et l’édition AZERTY sur x86_64.

À **64 Mio**, les deux VM ont été réellement lancées et leur RAM a été vérifiée via QMP. Sur x86_64, Limine s’est arrêté dans son allocateur de mémoire haute (`High memory allocator: Out of memory`). Sur AArch64, EDK II a déclenché l’assertion `NewSize >= 0x08000000` dans MemoryInit. Aucun appel système ni bilan mémoire NovaOS n’a pu être mesuré dans ces deux VM.

| Machine | RAM VM | Séries de trois appels | Commutations timer dans un syscall | Blocages wait | Retours / protection |
| --- | ---: | ---: | ---: | ---: | --- |
| x86_64 | 128 | 6 400 | 1 | 2 | 19 444 SYSRET, 4 IRETQ, SMAP actif |
| x86_64 | 256 | 5 632 | 1 | 2 | 17 056 SYSRET, 4 IRETQ, SMAP actif |
| Cortex-A72 | 128 | 5 120 | 1 | 2 | ERET, PAN indisponible |
| Cortex-A72 | 256 | 5 120 | 1 | 2 | ERET, PAN indisponible |
| Cortex-A76 | 256 | 8 704 | 1 | 2 | ERET, PAN actif |

Les compteurs de séries sont échantillonnés par pas de 256. Les compteurs de préemption excluent les cessions volontaires. Les retours x86 sont relevés sur les deux branches assembleur. Les mêmes erreurs attendues ont été vérifiées sur chaque configuration ayant atteint le noyau.

La régression `sched-stress` a effectué 15 162 commutations en 15 162 ticks sur chacune des deux cibles. L’endurance courte a duré 15,822 s sur x86_64 et 20,734 s sur AArch64, amorçage exclu. Elle a collecté respectivement 7 539 et 7 527 threads et retrouvé les bilans initiaux.

## Bilan par phase

Chiffres en **pages physiques de 4 Kio**, sauf la RAM VM en Mio et les métadonnées actives en octets. « Repos » est le contrôleur sans processus utilisateur. « Reprise » suit la pénurie, la nouvelle création réussie et sa collecte. « Actifs » contient les deux processus simultanés. « Interface » est relevé depuis idle, au prompt x86 ou après la terminaison du thread principal ARM. Le passage à une seule pile superviseur au repos sur ARM est visible dans ce dernier bilan.

### x86_64

| RAM VM | Phase | ELF | Tables | Piles noyau / utilisateur | Tas / méta Buddy | Pages utilisateur hors pile | Processus / méta actives (o) | Libres | Chargeur retenu | RAM totale retenue |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 128 | Initialisation | 64 | 72 | 0 / 0 | 0 / 0 | 0 | 0 / 0 | 21 201 | 9 833 | 9 969 |
| 128 | Repos | 64 | 78 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 992 | 0 | 178 |
| 128 | Reprise | 64 | 78 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 992 | 0 | 178 |
| 128 | Actifs | 64 | 89 | 64 / 2 | 3 / 1 | 5 | 2 / 384 | 30 942 | 0 | 228 |
| 128 | Après destruction | 64 | 78 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 992 | 0 | 178 |
| 128 | Interface | 64 | 78 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 992 | 0 | 178 |
| 256 | Initialisation | 64 | 136 | 0 / 0 | 0 / 0 | 0 | 0 / 0 | 53 905 | 9 833 | 10 033 |
| 256 | Repos | 64 | 142 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 696 | 0 | 242 |
| 256 | Reprise | 64 | 142 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 696 | 0 | 242 |
| 256 | Actifs | 64 | 153 | 64 / 2 | 3 / 1 | 5 | 2 / 384 | 63 646 | 0 | 292 |
| 256 | Après destruction | 64 | 142 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 696 | 0 | 242 |
| 256 | Interface | 64 | 142 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 696 | 0 | 242 |

À 256 Mio, le noyau a restitué 9 833 cadres Limine. La table des processus occupe 6 168 octets dans l’image et le vecteur des TCB réserve 4 096 octets dans le tas. Le suffixe récupérable du Buddy représente 0 page à ce point de repos. Le framebuffer couvre 469 pages, dont 0 en RAM système.

### aarch64

| RAM VM | Phase | ELF | Tables | Piles noyau / utilisateur | Tas / méta Buddy | Pages utilisateur hors pile | Processus / méta actives (o) | Libres | Chargeur retenu | RAM totale retenue |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 128 | Initialisation | 52 | 74 | 0 / 0 | 0 / 0 | 0 | 0 / 0 | 23 784 | 6 661 | 7 256 |
| 128 | Repos | 52 | 80 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 403 | 0 | 637 |
| 128 | Reprise | 52 | 80 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 403 | 0 | 637 |
| 128 | Actifs | 52 | 91 | 64 / 2 | 3 / 1 | 5 | 2 / 384 | 30 353 | 0 | 687 |
| 128 | Après destruction | 52 | 80 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 30 403 | 0 | 637 |
| 128 | Interface | 52 | 80 | 16 / 0 | 3 / 1 | 0 | 0 / 0 | 30 419 | 0 | 621 |
| 256 | Initialisation | 52 | 138 | 0 / 0 | 0 / 0 | 0 | 0 / 0 | 56 486 | 6 663 | 7 322 |
| 256 | Repos | 52 | 144 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 107 | 0 | 701 |
| 256 | Reprise | 52 | 144 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 107 | 0 | 701 |
| 256 | Actifs | 52 | 155 | 64 / 2 | 3 / 1 | 5 | 2 / 384 | 63 057 | 0 | 751 |
| 256 | Après destruction | 52 | 144 | 32 / 0 | 3 / 1 | 0 | 0 / 0 | 63 107 | 0 | 701 |
| 256 | Interface | 52 | 144 | 16 / 0 | 3 / 1 | 0 | 0 / 0 | 63 123 | 0 | 685 |

À 256 Mio, le noyau a restitué 6 663 cadres Limine. La table des processus occupe 6 168 octets dans l’image et le vecteur des TCB réserve 4 096 octets dans le tas. Le suffixe récupérable du Buddy représente 0 page à ce point de repos. Le framebuffer couvre 469 pages, dont 469 en RAM système.

| RAM physique de l’interface au repos | x86_64 | AArch64 Cortex-A72 |
| --- | ---: | ---: |
| VM de 128 Mio | 712 Kio | 2 484 Kio |
| VM de 256 Mio | 968 Kio | 2 740 Kio |

Le seuil provisoire de **8 192 Kio** est respecté dans les quatre configurations mesurées. Les instantanés de repos, de reprise et d’après destruction coïncident champ par champ à taille de VM identique.

## Portée

Les essais portent sur Q35/x86_64 et virt/AArch64 mono-CPU. L’essai complémentaire Cortex-A76 vérifie PAN sur un cœur qui l’implémente. Les campagnes de deux heures archivées correspondent à la v0.5.1. Les suites v0.7.0 reprennent la sonde de préemption de 15 000 ticks et l’endurance courte. Les mesures de résidence ne constituent pas une borne de consommation pour une charge arbitraire ou pour le firmware.
