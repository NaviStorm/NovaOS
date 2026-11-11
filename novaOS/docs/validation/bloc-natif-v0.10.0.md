# Mesures et preuves natives du transport v0.10.0

Valeurs série brutes (ns pour les histogrammes, cadres de 4 Kio pour RAM). Les colonnes du rapport principal sont calculées à partir de ces lignes.

## x86_64 — 512 octets

```text
BLOC chaine novaFS=2 bloc=1 pilote=3 client=4 generation=1
BLOC parcours i=0 octets=16384 ns=29119836
BLOC parcours i=1 octets=16384 ns=27381398
BLOC parcours i=2 octets=16384 ns=26860667
BLOC parcours i=3 octets=16384 ns=26470120
BLOC mesure type=read n=64 p50=131256 p95=180477 p99=609855 max=609855 moyenne=142245
BLOC mesure type=write n=64 p50=36915 p95=73831 p99=369518 max=369518 moyenne=45578
BLOC mesure type=flush n=2 p50=114849 p95=367515 p99=367515 max=367515 moyenne=238334
BLOC mesure type=validation n=130 p50=1025 p95=2050 p99=32814 max=170238 moyenne=2618
BLOC mesure type=copie_read n=384 p50=1025 p95=1025 p99=1025 max=42058 moyenne=635
BLOC mesure type=copie_write n=384 p50=1025 p95=1025 p99=1025 max=46064 moyenne=677
IPC hist type=irq n=14428 p50=6152 p95=8203 p99=9228 max=1674348
IPC hist type=livraison n=3086 p50=14356 p95=24610 p99=28712 max=1527141
IPC hist type=aller_retour n=1029 p50=98442 p95=147663 p99=246105 max=1619270
IPC hist type=reprise n=4132 p50=15381 p95=22559 p99=61526 max=1521133
IPC hist type=appel n=6174 p50=41017 p95=73831 p99=123052 max=4455247
IPC hist type=commutation n=2057 p50=3076 p95=4005 p99=4005 max=4005
RAM phase=bloc_repos image=143 tables=142 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63617 cache=0 chargeur=0 restitue=9948 extra_elf=0 video=469 ram_video=0 resident=321 retenu=321 dma=0
RAM phase=bloc_publie image=143 tables=167 kpile=96 upile=4 heap=12 buddy=1 ipc=9 utilisateur=8 processus=4 meta_octets=6168 meta_actifs=768 tcb_octets=4096 heap_va=65536 libres=63482 cache=0 chargeur=0 restitue=9948 extra_elf=0 video=469 ram_video=0 resident=456 retenu=456 dma=16
RAM phase=bloc_collecte image=143 tables=145 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63605 cache=0 chargeur=0 restitue=9948 extra_elf=0 video=469 ram_video=0 resident=333 retenu=333 dma=9
BLOC memoire cadres_init=63617 cadres_fin=63605 allocations=135 pic_cadres=135 buddy_init=9280 buddy_fin=9280 hz=998597656 retrait_ns=561787
BLOC bilan mode=6 service=Exit(0) bloc=Exit(0) pilote=Exit(0) client=Exit(0) retrait=Ok(()) cycles_retrait=561000
```

```text
dma-fence: DMA protection droits=1 IRQ=1 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA protection droits=0 IRQ=2 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA interruption masquee en attente retiree et nouveau proprietaire confirme
dma-fence: DMA IRQ tardive acquittee sans livraison
dma-flight: PROC pid=3 niveau=Ring3 pc=0x400720 faute=#UD adresse=0x0 acces=instruction cause=invalide syndrome=0x0
dma-flight: DMA pilote mort transfert_en_vol=true
dma-flight: DMA quarantaine cadres epingles
dma-flight: DMA utilisateurs mode=2 generation=1 resultats=(Exit(11), Exit(11), Fault(Fault { trap: "#UD", access: "instruction", cause: "invalide", address: 0, syndrome: 0 }))
dma-flight: DMA utilisateurs mode=1 generation=2 resultats=(Exit(0), Exit(0), Exit(0))
storage-write: DEBUG pid=2 NVFS WRITE accepte
storage-write: DEBUG pid=2 NVFS FLUSH confirme
storage-read: DEBUG pid=2 NVFS READ octets compares
eio-read: BLOC erreur peripherique op=0 statut=1 longueur=513
eio-write: BLOC erreur peripherique op=1 statut=1 longueur=1
eio-flush: BLOC erreur peripherique op=2 statut=1 longueur=1
lecture-seule: BLOC bilan mode=4 service=Exit(5) bloc=Exit(11) pilote=Exit(11) client=Exit(5) retrait=Ok(()) cycles_retrait=580000
```

## x86_64 — 4096 octets

```text
BLOC chaine novaFS=2 bloc=1 pilote=3 client=4 generation=1
BLOC parcours i=0 octets=131072 ns=151181000
BLOC parcours i=1 octets=131072 ns=150019457
BLOC parcours i=2 octets=131072 ns=147458081
BLOC parcours i=3 octets=131072 ns=149064300
BLOC mesure type=read n=64 p50=147018 p95=179689 p99=284154 max=284154 moyenne=142030
BLOC mesure type=write n=64 p50=53089 p95=81676 p99=585257 max=585257 moyenne=65990
BLOC mesure type=flush n=2 p50=392049 p95=415762 p99=415762 max=415762 moyenne=389341
BLOC mesure type=validation n=130 p50=1020 p95=2041 p99=36754 max=170492 moyenne=2752
BLOC mesure type=copie_read n=2752 p50=1020 p95=1020 p99=1020 max=39881 moyenne=550
BLOC mesure type=copie_write n=2752 p50=1020 p95=1020 p99=1020 max=63810 moyenne=586
IPC hist type=irq n=80765 p50=6125 p95=8167 p99=8167 max=1679002
IPC hist type=livraison n=17294 p50=15314 p95=24503 p99=26544 max=1254266
IPC hist type=aller_retour n=5765 p50=98012 p95=114347 p99=163353 max=1289162
IPC hist type=reprise n=23109 p50=15314 p95=20419 p99=24503 max=1179489
IPC hist type=appel n=34590 p50=40838 p95=73509 p99=81676 max=4486645
IPC hist type=commutation n=11555 p50=3062 p95=4083 p99=4083 max=76771
RAM phase=bloc_repos image=143 tables=142 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63617 cache=0 chargeur=0 restitue=10061 extra_elf=0 video=469 ram_video=0 resident=321 retenu=321 dma=0
RAM phase=bloc_publie image=143 tables=167 kpile=96 upile=4 heap=12 buddy=1 ipc=9 utilisateur=8 processus=4 meta_octets=6168 meta_actifs=768 tcb_octets=4096 heap_va=65536 libres=63482 cache=0 chargeur=0 restitue=10061 extra_elf=0 video=469 ram_video=0 resident=456 retenu=456 dma=16
RAM phase=bloc_collecte image=143 tables=145 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63605 cache=0 chargeur=0 restitue=10061 extra_elf=0 video=469 ram_video=0 resident=333 retenu=333 dma=9
BLOC memoire cadres_init=63617 cadres_fin=63605 allocations=135 pic_cadres=135 buddy_init=9280 buddy_fin=9280 hz=1002976562 retrait_ns=623145
BLOC bilan mode=6 service=Exit(0) bloc=Exit(0) pilote=Exit(0) client=Exit(0) retrait=Ok(()) cycles_retrait=625000
```

```text
dma-fence: DMA protection droits=1 IRQ=1 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA protection droits=0 IRQ=2 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA interruption masquee en attente retiree et nouveau proprietaire confirme
dma-fence: DMA IRQ tardive acquittee sans livraison
dma-flight: PROC pid=3 niveau=Ring3 pc=0x400720 faute=#UD adresse=0x0 acces=instruction cause=invalide syndrome=0x0
dma-flight: DMA pilote mort transfert_en_vol=true
dma-flight: DMA quarantaine cadres epingles
dma-flight: DMA utilisateurs mode=2 generation=1 resultats=(Exit(5), Exit(11), Fault(Fault { trap: "#UD", access: "instruction", cause: "invalide", address: 0, syndrome: 0 }))
dma-flight: DMA utilisateurs mode=1 generation=2 resultats=(Exit(0), Exit(0), Exit(0))
storage-write: DEBUG pid=2 NVFS WRITE accepte
storage-write: DEBUG pid=2 NVFS FLUSH confirme
storage-read: DEBUG pid=2 NVFS READ octets compares
eio-read: BLOC erreur peripherique op=0 statut=1 longueur=4097
eio-write: BLOC erreur peripherique op=1 statut=1 longueur=1
eio-flush: BLOC erreur peripherique op=2 statut=1 longueur=1
lecture-seule: BLOC bilan mode=4 service=Exit(5) bloc=Exit(11) pilote=Exit(11) client=Exit(5) retrait=Ok(()) cycles_retrait=793000
```

## aarch64 — 512 octets

```text
BLOC chaine novaFS=2 bloc=1 pilote=3 client=4 generation=1
BLOC parcours i=0 octets=16384 ns=35817008
BLOC parcours i=1 octets=16384 ns=34068992
BLOC parcours i=2 octets=16384 ns=33126000
BLOC parcours i=3 octets=16384 ns=31364000
BLOC mesure type=read n=64 p50=163840 p95=180224 p99=208000 max=208000 moyenne=160688
BLOC mesure type=write n=64 p50=53248 p95=81920 p99=161008 max=161008 moyenne=56464
BLOC mesure type=flush n=2 p50=360448 p95=431008 p99=431008 max=431008 moyenne=388000
BLOC mesure type=validation n=130 p50=1024 p95=2048 p99=147456 max=273008 moyenne=4400
BLOC mesure type=copie_read n=384 p50=1024 p95=1024 p99=1024 max=21008 moyenne=624
BLOC mesure type=copie_write n=384 p50=1024 p95=1024 p99=1024 max=31008 moyenne=688
IPC hist type=irq n=14429 p50=7168 p95=10240 p99=12288 max=802000
IPC hist type=livraison n=3086 p50=18432 p95=30720 p99=40960 max=1414000
IPC hist type=aller_retour n=1029 p50=131072 p95=196608 p99=245760 max=1520992
IPC hist type=reprise n=4133 p50=18432 p95=26624 p99=90112 max=1406992
IPC hist type=appel n=6174 p50=45056 p95=90112 p99=163840 max=3306000
IPC hist type=commutation n=2055 p50=5120 p95=6144 p99=6144 max=9008
RAM phase=bloc_repos image=120 tables=144 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63039 cache=0 chargeur=0 restitue=6752 extra_elf=0 video=469 ram_video=469 resident=300 retenu=769 dma=0
RAM phase=bloc_publie image=120 tables=169 kpile=96 upile=4 heap=12 buddy=1 ipc=9 utilisateur=8 processus=4 meta_octets=6168 meta_actifs=768 tcb_octets=4096 heap_va=65536 libres=62904 cache=0 chargeur=0 restitue=6752 extra_elf=0 video=469 ram_video=469 resident=435 retenu=904 dma=16
RAM phase=bloc_collecte image=120 tables=147 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63036 cache=0 chargeur=0 restitue=6752 extra_elf=0 video=469 ram_video=469 resident=303 retenu=772 dma=0
BLOC memoire cadres_init=63039 cadres_fin=63036 allocations=135 pic_cadres=135 buddy_init=10304 buddy_fin=10304 hz=62500000 retrait_ns=541008
BLOC bilan mode=6 service=Exit(0) bloc=Exit(0) pilote=Exit(0) client=Exit(0) retrait=Ok(()) cycles_retrait=33813
```

```text
dma-fence: DMA protection droits=1 IRQ=1 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA protection droits=0 IRQ=2 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA interruption masquee en attente retiree et nouveau proprietaire confirme
dma-fence: DMA IRQ tardive acquittee sans livraison
dma-flight: PROC pid=3 niveau=EL0 pc=0x400654 faute=exception adresse=0x0 acces=instruction cause=exception syndrome=0xf2000000
dma-flight: DMA pilote mort transfert_en_vol=true
dma-flight: DMA quarantaine cadres epingles
dma-flight: DMA utilisateurs mode=2 generation=1 resultats=(Exit(11), Exit(11), Fault(Fault { trap: "exception", access: "instruction", cause: "exception", address: 0, syndrome: 4060086272 }))
dma-flight: DMA utilisateurs mode=1 generation=2 resultats=(Exit(0), Exit(0), Exit(0))
storage-write: DEBUG pid=2 NVFS WRITE accepte
storage-write: DEBUG pid=2 NVFS FLUSH confirme
storage-read: DEBUG pid=2 NVFS READ octets compares
eio-read: BLOC erreur peripherique op=0 statut=1 longueur=513
eio-write: BLOC erreur peripherique op=1 statut=1 longueur=1
eio-flush: BLOC erreur peripherique op=2 statut=1 longueur=1
lecture-seule: BLOC bilan mode=4 service=Exit(5) bloc=Exit(11) pilote=Exit(11) client=Exit(5) retrait=Ok(()) cycles_retrait=33688
```

## aarch64 — 4096 octets

```text
BLOC chaine novaFS=2 bloc=1 pilote=3 client=4 generation=1
BLOC parcours i=0 octets=131072 ns=183870000
BLOC parcours i=1 octets=131072 ns=175822000
BLOC parcours i=2 octets=131072 ns=179612000
BLOC parcours i=3 octets=131072 ns=175242000
BLOC mesure type=read n=64 p50=163840 p95=180224 p99=198000 max=198000 moyenne=164608
BLOC mesure type=write n=64 p50=73728 p95=98304 p99=230000 max=230000 moyenne=74576
BLOC mesure type=flush n=2 p50=384992 p95=384992 p99=384992 max=384992 moyenne=378496
BLOC mesure type=validation n=130 p50=2048 p95=2048 p99=147456 max=207008 moyenne=4144
BLOC mesure type=copie_read n=2752 p50=1024 p95=1024 p99=1024 max=20000 moyenne=656
BLOC mesure type=copie_write n=2752 p50=1024 p95=1024 p99=1024 max=28992 moyenne=624
IPC hist type=irq n=80779 p50=7168 p95=9216 p99=10240 max=696000
IPC hist type=livraison n=17294 p50=18432 p95=28672 p99=32768 max=1284992
IPC hist type=aller_retour n=5765 p50=122880 p95=147456 p99=196608 max=1160992
IPC hist type=reprise n=23123 p50=18432 p95=24576 p99=26624 max=1042000
IPC hist type=appel n=34590 p50=45056 p95=90112 p99=98304 max=3192000
IPC hist type=commutation n=11527 p50=4096 p95=5120 p99=6144 max=16000
RAM phase=bloc_repos image=120 tables=144 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63039 cache=0 chargeur=0 restitue=6752 extra_elf=0 video=469 ram_video=469 resident=300 retenu=769 dma=0
RAM phase=bloc_publie image=120 tables=169 kpile=96 upile=4 heap=12 buddy=1 ipc=9 utilisateur=8 processus=4 meta_octets=6168 meta_actifs=768 tcb_octets=4096 heap_va=65536 libres=62904 cache=0 chargeur=0 restitue=6752 extra_elf=0 video=469 ram_video=469 resident=435 retenu=904 dma=16
RAM phase=bloc_collecte image=120 tables=147 kpile=32 upile=0 heap=3 buddy=1 ipc=0 utilisateur=0 processus=0 meta_octets=6168 meta_actifs=0 tcb_octets=4096 heap_va=65536 libres=63036 cache=0 chargeur=0 restitue=6752 extra_elf=0 video=469 ram_video=469 resident=303 retenu=772 dma=0
BLOC memoire cadres_init=63039 cadres_fin=63036 allocations=135 pic_cadres=135 buddy_init=10304 buddy_fin=10304 hz=62500000 retrait_ns=582000
BLOC bilan mode=6 service=Exit(0) bloc=Exit(0) pilote=Exit(0) client=Exit(0) retrait=Ok(()) cycles_retrait=36375
```

```text
dma-fence: DMA protection droits=1 IRQ=1 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA protection droits=0 IRQ=2 statut=0 erreur=Err(Fault) faute=Err(Fault) temoin=true
dma-fence: DMA interruption masquee en attente retiree et nouveau proprietaire confirme
dma-fence: DMA IRQ tardive acquittee sans livraison
dma-flight: PROC pid=3 niveau=EL0 pc=0x400654 faute=exception adresse=0x0 acces=instruction cause=exception syndrome=0xf2000000
dma-flight: DMA pilote mort transfert_en_vol=true
dma-flight: DMA quarantaine cadres epingles
dma-flight: DMA utilisateurs mode=2 generation=1 resultats=(Exit(11), Exit(11), Fault(Fault { trap: "exception", access: "instruction", cause: "exception", address: 0, syndrome: 4060086272 }))
dma-flight: DMA utilisateurs mode=1 generation=2 resultats=(Exit(0), Exit(0), Exit(0))
storage-write: DEBUG pid=2 NVFS WRITE accepte
storage-write: DEBUG pid=2 NVFS FLUSH confirme
storage-read: DEBUG pid=2 NVFS READ octets compares
eio-read: BLOC erreur peripherique op=0 statut=1 longueur=4097
eio-write: BLOC erreur peripherique op=1 statut=1 longueur=1
eio-flush: BLOC erreur peripherique op=2 statut=1 longueur=1
lecture-seule: BLOC bilan mode=4 service=Exit(5) bloc=Exit(11) pilote=Exit(11) client=Exit(5) retrait=Ok(()) cycles_retrait=33125
```

## Régressions dans le checkout de livraison

Les commandes `make -C novaOS ARCH=x86_64 test` et `make -C novaOS ARCH=aarch64 test` ont chacune terminé avec 25 scénarios réussis. Le scénario d’endurance inclus, demandé pour 15 secondes, a duré respectivement 15,738 s et 16,962 s ; le nombre de cadres libres a retrouvé sa valeur initiale sur chaque cible. Les deux commandes `block-test` ont également terminé avec 20 groupes réussis par architecture. Cette campagne ne vaut pas un essai d’endurance de deux heures.
