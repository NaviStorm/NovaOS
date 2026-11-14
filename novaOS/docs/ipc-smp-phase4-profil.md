# Profil détaillé des appels IPC sous contention

Le banc utilise un client et un serveur épinglés par vCPU, 1 000 allers-retours de 128 octets par couple, avec départ commun et sortie horodatée du client. Les copies de sources de `f356970`, `4e741fb` et `a8dc253` portent les mêmes bornes de chronométrage. Les trois versions utilisent la même compilation release `ipc-smp-lab`. Les correctifs de laboratoire interviennent exclusivement dans les copies d’essai. [Banc de mesure](../tests/ipc_multicoeur.py) · [Sources et analyseur des profils](../tests/ipc_phase4_sites.py) · [Résultats individuels](validation/ipc-smp-phase4-sites.json)

## Fenêtres de mesure

Après un lot de chauffe, chacun des sept lots active un groupe de sondes. Les lots 1/4/7 mesurent l’IPC et ses parcours. Les lots 2/5 mesurent la mémoire, les copies et la durée de l’étape IPC. Les lots 3/6 mesurent l’ordonnanceur, les interruptions et les invalidations. Le contrôleur dort pendant les échanges. Il active les sondes avant le départ des acteurs et les arrête après la sortie de tous les clients. L’horloge invitée est le compteur ARM ou le TSC x86 étalonné par le PIT. Chaque démarrage conserve aussi la durée murale mesurée par le lecteur série et le temps CPU utilisateur/système du processus QEMU entier sur le Mac.

Les sites utilisent deux lectures d’horloge et des compteurs atomiques. Un appel vide de 128 répétitions par lot mesure le coût plancher des lectures. À quatre vCPU x86, la médiane de la paire de lectures à vide vaut 23,4 ns sur le témoin de phase 4 et 23,5 ns sur le candidat. La mise à jour des compteurs suit la seconde lecture et perturbe le temps du lot. Le tableau donne les temps bruts. Une opération très courte peut avoir le même ordre de grandeur que la lecture de l’horloge. Les durées inclusives se recouvrent. La résolution des droits est incluse dans la recherche de capacité, et l’appartenance RAM dans la validation des segments. Une recherche de tête peut être incluse dans la notification. Le chemin d’émission sous forme de dépôt et la publication d’une réception après la copie sont tous deux comptés. Le changement de pile est chronométré entre la sauvegarde de la profondeur et l’arrivée sur la pile entrante, avant la reprise du verrou. Les interruptions sont chronométrées jusqu’à l’acquittement avant ordonnancement, y compris le service des invalidations locales dans le vecteur dédié x86. Ces mesures n’attribuent pas le délai entre la demande d’IRQ et son arrivée.

## Résultats à quatre couples

Chaque comparaison comprend trois démarrages par version, soit 6 ou 9 lots par groupe et 21 lots pour le chronométrage global. Dans le tableau, le témoin historique est `f356970`, le candidat de phase 3 est `4e741fb` et le correctif de phase 4 est `a8dc253`. Chaque flèche correspond à une série alternée de VM distincte. Les durées sont les médianes des ms cumulées par lot, tous CPU confondus. Le nombre d’appels est la médiane de l’appariement phase 3 → phase 4. Les p95, p99, IQR et extrêmes des lots mesurés figurent dans le JSON.

| Site sur x86_64 | Historique → phase 3, ms | Phase 3 → phase 4, ms | Phase 3 → phase 4, appels |
|---|---:|---:|---:|
| Recherche de la capacité et de son détenteur | 7,6 → 8,5 | 9,2 → 11,3 | 16 000 → 16 000 |
| Génération du nœud, détenteur et masque de droits | 3,8 → 4,6 | 4,5 → 4,0 | 16 000 → 16 000 |
| Sélection du dépôt, de la notification ou de l’offre du canal | 34,6 → 49,0 | 47,0 → 47,7 | 132 180 → 127 998 |
| Sélection du premier receveur du canal | 43,8 → 63,2 | 60,3 → 60,0 | 132 180 → 127 998 |
| Recherche et notification des attentes livrables | 88,5 → 124,4 | 126,8 → 125,8 | 23 844 → 23 833 |
| Admission, quotas et inscription du dormeur | 11,7 → 16,4 | 18,3 → 14,5 | 7 840 → 7 829 |
| Dépôt ou livraison validée | 9,0 → 12,2 | 12,8 → 12,6 | 16 000 → 16 000 |
| Contrôle des tables de handles devenues vides | 6,7 → 9,4 | 10,5 → 1,7 | 23 844 → 23 833 |
| Restitution des pages de pools | 12,8 → 26,4 | 24,4 → 20,0 | 23 844 → 23 833 |
| Appartenance du cadre à une plage RAM admise | 20,6 → 21,1 | 24,2 → 9,5 | 40 000 → 40 000 |
| Validation des segments, pages, propriétés et droits mémoire | 54,8 → 47,4 | 53,1 → 40,2 | 40 000 → 40 000 |
| Copie des segments déjà validés vers le noyau | 3,8 → 3,3 | 3,7 → 4,5 | 16 000 → 16 000 |
| Copie des segments déjà validés vers le client | 5,5 → 3,6 | 4,2 → 4,6 | 8 000 → 8 000 |
| Copie de la charge utile de 128 octets dans la trame | 4,5 → 4,7 | 5,2 → 5,8 | 16 000 → 16 000 |
| Étape IPC complète sous la garde | 290,0 → 308,9 | 336,3 → 337,4 | 23 819 → 23 835 |
| Identification du CPU dans current, wake et select | 18,1 → 21,2 | 23,0 → 7,9 | 118 527 → 100 537 |
| Sélection dans la file des prêts | 1,5 → 3,5 | 4,7 → 1,9 | 12 786 → 9 204 |
| Passage du thread à READY et éventuel IPI | 7,9 → 7,8 | 8,7 → 5,6 | 7 858 → 7 836 |
| Changement de pile sur le même CPU | 7,3 → 4,5 | 5,2 → 6,0 | 8 028 → 8 022 |
| Activation de l’espace d’adressage | 18,4 → 20,3 | 23,1 → 23,4 | 8 028 → 8 022 |
| Émission de l’IPI de réveil | 0,0 → 0,0 | 0,0 → 0,0 | 0 → 0 |
| Service matériel de l’IRQ jusqu’à acquittement | 3,2 → 2,7 | 6,4 → 3,1 | 552 → 580 |
| Invalidation d’une adresse, attente distante comprise | 10,9 → 8,5 | 72,9 → 11,0 | 174 → 174 |

La mesure de l’identité couvre les appels de `Core::current`, celle du CPU qui effectue le réveil et celle de la sélection. La boucle de consultation des invalidations sous contention reste extérieure à ce compteur. Le sous-ensemble des valeurs instrumentées dépend des sollicitations du noyau induites par le banc. Les appels aux sites incluent les processus auxiliaires actifs pendant la fenêtre.

| Architecture et couples | Durée invitée p50 par lot, ms | Durée hôte p50 par lot, ms | CPU hôte p50 par VM, s | Fréquences invitées étalonnées, GHz |
|---|---:|---:|---:|---:|
| x86_64, 2 | 149,9 → 143,5 | 150,3 → 144,3 | 4,83 → 4,92 | 0,995–1,005 |
| x86_64, 4 | 642,5 → 574,8 | 675,7 → 583,0 | 17,13 → 16,46 | 0,999–1,031 |
| aarch64, 2 | 467,1 → 459,6 | 467,6 → 460,9 | 6,68 → 6,45 | 0,0625 |
| aarch64, 4 | 1 572,7 → 760,4 | 1 576,2 → 763,3 | 42,42 → 26,45 | 0,0625 |

La comparaison `mesures-deg` couvre des fréquences étalonnées x86 de 1,000–1,783 GHz. Le rapport des durées hôte/invité est dans l’intervalle 1,00–1,80, avec une médiane de 1,02.
La comparaison `mesures-corr` couvre des fréquences étalonnées x86 de 0,995–1,031 GHz. Le rapport des durées hôte/invité est dans l’intervalle 0,99–1,37, avec une médiane de 1,01.

Les campagnes de profilage se sont déroulées pendant d’autres régressions. La conversion x86 en ns dépend de l’étalonnage par 64 interruptions PIT. Dans ces conditions certains démarrages donnent des fréquences nettement supérieures à 1 GHz. La durée murale hôte, les ticks bruts, l’empreinte ELF et la charge par démarrage sont conservés pour vérifier cet effet. Le temps CPU de QEMU comprend l’amorçage, les vCPU et les threads d’émulation. Les ticks utilisateur/noyau/idle renseignent l’activité invitée. L’attribution d’un temps exact à TCG, à une désélection macOS ou à l’exécution physique sur un type de cœur reste ouverte. Les campagnes sans sonde à charge contrôlée fournissent les chiffres de décision.

## Vérification de l’appartenance RAM

Deux démarrages supplémentaires par version x86_64 à quatre vCPU comptent les appels issus du gestionnaire de cadres durant les lots mémoire. Un appel mesure une demande d’appartenance d’un cadre déjà extrait d’une PTE. Les recherches comptent les appels qui parcourent la carte des régions jusqu’à une éventuelle correspondance. La géométrie utilisateur, les PTE et leurs droits sont revérifiés à chaque copie. Un succès du cache ne court-circuite que la recherche de la plage physique immuable.

| Version | Appartenances contrôlées | Appels servis par le cache | Appels nécessitant une recherche |
|---|---:|---:|---:|
| Phase 3 | 160 000 | 0 | 160 000 |
| Phase 4 | 160 000 | 119 081 | 40 919 |

## Reproduction

Lancer les commandes depuis `novaOS/`, dans des répertoires neufs et avec la même version QEMU TCG pour tous les témoins.

```sh
python3 -B tests/ipc_phase4_sites.py construire historique build/sites-historique --arch x86_64
python3 -B tests/ipc_phase4_sites.py construire temoin build/sites-temoin
python3 -B tests/ipc_phase4_sites.py construire candidat build/sites-candidat
python3 -B tests/ipc_multicoeur.py build/sites-temoin/elf build/mesures-deg --temoin build/sites-historique/elf --cpus 4 2 --arch x86_64 --cas paires-sonde --repetitions 3
python3 -B tests/ipc_multicoeur.py build/sites-candidat/elf build/mesures-corr --temoin build/sites-temoin/elf --cpus 4 2 --arch x86_64 aarch64 --cas paires-sonde --repetitions 3
python3 -B tests/ipc_phase4_sites.py construire temoin build/sites-ram-temoin --arch x86_64 --compter-ram
python3 -B tests/ipc_phase4_sites.py construire candidat build/sites-ram-candidat --arch x86_64 --compter-ram
python3 -B tests/ipc_multicoeur.py build/sites-ram-candidat/elf build/cache-ram --temoin build/sites-ram-temoin/elf --cpus 4 --arch x86_64 --cas paires-sonde --repetitions 2
python3 -B tests/ipc_phase4_sites.py analyser build/rapport-sites.json build/mesures-deg build/mesures-corr build/cache-ram
```
