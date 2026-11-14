# Présélection des optimisations IPC et coût mémoire

Les deux expériences x86_64 comparent d’abord les choix de la phase 3 un par un, puis les trois modifications proposées pour la phase 4. La vérification mémoire emploie les noyaux ordinaires sur les deux architectures. Les données contiennent les empreintes ELF, la chauffe, chaque durée invitée et hôte, les quatre durées individuelles de chaque lot, la charge hôte et les quatre relevés mémoire complets. [Données et synthèse](validation/ipc-diagnostic-phase4-v0.17.0.json) · [Qualification finale](ipc-smp-phase4-v0.17.0.md)

## Méthode

Chaque série comporte trois démarrages par variante, quatre vCPU, quatre couples épinglés, 1 000 allers-retours DATA128 par couple, un lot de chauffe et sept lots mesurés. Le débit d’un lot vaut 4 000 divisés par la durée invitée du couple le plus lent, en secondes. Chaque médiane globale porte sur 21 lots. L’ordre des variantes est mélangé à chaque répétition avec une graine reproductible. La charge hôte à une minute relevée en fin de VM allait de 12,47 à 19,60 pour la première série et de 9,65 à 16,17 pour la seconde. Ces campagnes courtes servent à sélectionner le candidat. Les essais appariés finaux qualifient d’autres répartitions des acteurs et donnent les deux horloges. [Protocole de phase 3](ipc-smp-phase3-v0.17.0.md) · [Mesures finales](ipc-smp-phase4-v0.17.0.md)

QEMU direct 11.1.3, TCG, x86 `q35`/`max`, 256 Mio. Hôte Darwin arm64, macOS 27.2 (26B123). Construction Rust stable 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), `release --locked --offline`, option `ipc-smp-lab`. Les deux séries emploient le lecteur hôte de `4e741fb`. L’ELF historique venait du banc `f356970`. Sa sortie de série précède l’ajout du compteur `requetes_bloc`. La charge hôte totale et l’étalonnage TSC restent propres à chaque exécution.

## Variantes x86_64

Les débits sont en allers-retours par seconde, arrondis à l’unité. Les extrêmes incluent chaque lot mesuré.

| Première série | Médianes des trois démarrages | Médiane globale | Minimum – maximum |
|---|---|---:|---:|
| Phase 3 (`base`) | 16 001 / 14 670 / 16 118 | 15 895 | 13 108 – 16 518 |
| Allocation ordinaire des attentes (`pool-fixe`) | 15 960 / 15 238 / 13 371 | 15 525 | 7 139 – 16 515 |
| Ancienne acquisition de garde (`gate-avant`) | 16 910 / 16 765 / 16 238 | 16 705 | 14 961 – 17 060 |
| Ancienne entrée syscall (`syscall-avant`) | 16 270 / 14 907 / 16 441 | 16 216 | 13 997 – 16 586 |
| Réveil par diffusion (`diffusion`) | 15 061 / 14 946 / 14 449 | 14 946 | 13 369 – 15 845 |
| Réemploi de la garde et de son propriétaire (`garde`) | 16 980 / 17 148 / 17 067 | 17 089 | 14 476 – 17 456 |
| Témoin précédant les optimisations (`historique`) | 16 999 / 16 246 / 16 872 | 16 785 | 15 294 – 17 203 |

| Deuxième série | Médianes des trois démarrages | Médiane globale | Minimum – maximum |
|---|---|---:|---:|
| `base` | 15 725 / 15 993 / 16 078 | 15 932 | 15 390 – 16 383 |
| `historique` | 16 262 / 16 832 / 17 045 | 16 701 | 15 611 – 17 311 |
| `garde` | 17 280 / 17 560 / 16 866 | 17 367 | 8 206 – 17 815 |
| Cache de la dernière région RAM (`ram`) | 17 478 / 17 084 / 17 352 | 17 304 | 15 816 – 18 030 |
| Tables locales candidates au retrait (`loc`) | 16 428 / 16 002 / 18 631 | 16 428 | 15 755 – 19 175 |
| `g-ram` | 17 957 / 18 061 / 17 545 | 17 890 | 14 953 – 18 427 |
| `g-ram-loc` | 19 179 / 18 489 / 18 536 | 18 784 | 8 915 – 19 271 |

Le réemploi de la garde améliore la médiane de 7,5 % dans la première série. La diffusion la réduit de 6,0 %. Dans la seconde, le cache RAM seul apporte 8,6 %, le contrôle ciblé des tables locales seul 3,1 % et leur combinaison avec la garde 17,9 %. Le minimum de `g-ram-loc` justifie de conserver les durées individuelles et de répéter la comparaison. Ces écarts de débit n’isolent pas un temps CPU par fonction. [Profilage des chemins individuels](ipc-smp-phase4-profil.md)

## Mémoire

Quatre VM QEMU à quatre vCPU et 128 Mio ont exécuté `ipc.py test --ram 128`. Chaque ligne donne les cadres de 4 096 octets comptabilisés par l’invité. Les mesures avant réutilisent les ELF ordinaires de phase 3 (`86a9aba`). Les mesures après emploient `a8dc253` et les deux sondes mémoire ensuite commises dans `17e8de2`. Une exécution de chaque configuration a été relevée. [Champs bruts, chronométrages et écarts par phase](validation/ipc-diagnostic-phase4-v0.17.0.json)

| Architecture | Résidents au repos avant / après | Résidents en saturation avant / après | Cadres IPC repos / actifs / saturés / détruits, avant et après | Cadres libres au repos avant / après |
|---|---:|---:|---:|---:|
| AArch64 | 794 / 797 | 1 275 / 1 278 | 0 / 3 / 71 / 0 | 29 777 / 29 774 |
| x86_64 | 856 / 858 | 1 337 / 1 339 | 0 / 3 / 71 / 0 | 30 290 / 30 288 |

Dans chacun des quatre relevés, tous les compteurs au repos sont retrouvés après destruction et reprise. Le nombre de cadres IPC utilisés en saturation et le nombre de métadonnées actives restent identiques entre versions. L’image du noyau mesurée avec les sondes occupe trois cadres de plus sur ARM (12 Kio) et deux sur x86 (8 Kio). Le nombre de cadres résidents au repos augmente d’autant. Ces relevés ne séparent pas le coût binaire des sondes de celui du correctif.

## Reproduction

Les sous-commandes travaillent sur des exports Git neufs. Les sources des exports sont immuablement identifiées dans le lanceur. Les ablations `gate-avant` et `syscall-avant` reprennent un seul fichier de `f356970`. `pool-fixe` reprend la seule déclaration du pool des attentes. `diffusion` garde le test sur les CPU admissibles avant l’IPI diffusée. `garde` applique la fixture de phase 3. `ram` reprend la version relue du cache de `a8dc253`, avec un prédicat d’admission et des variables nommés. `loc` reproduit la marque posée **avant** la tentative de création d’une table. Le candidat final la pose **après** l’insertion réussie, ce qui évite un contrôle après échec. Le JSON conserve les SHA-256 réellement mesurés. Les chemins de compilation et les différences de présentation du Rust peuvent changer ceux d’une reconstruction. [Lanceur](../tests/ipc_phase4_ablations.py) · [Fixture de garde](../tests/fixtures/ipc-smp-phase3-garde-x86.patch)

Depuis la racine du dépôt, avec Python 3.12+ et les dépendances de compilation et QEMU de NovaOS, les commandes suivantes reconstruisent les deux séries et les quatre mesures mémoire dans un répertoire neuf. `preparer` seul permet d’inspecter la source sans compilation. Les 13 reconstructions de sources ont été vérifiées. Pour l’archive initiale, les 336 durées de lot (dont 294 hors chauffe) et les 1 344 durées de couple ont été recoupées avec les 42 journaux série. Cette vérification a réutilisé les quatre relevés mémoire originaux. La construction choisit `RUSTC` via `rustup which --toolchain stable rustc` afin que le compilateur appelé par le script Cargo soit également celui du banc.

```sh
a=novaOS/tests/ipc_phase4_ablations.py
s=novaOS/build/rejeu-ablations
python3 -B "$a" preparer g-ram-loc "$s/source-inspection" --compiler
python3 -B "$a" mesurer mesures "$s"
python3 -B "$a" mesurer comparaison "$s"
python3 -B "$a" mesurer-cadres "$s"
python3 -B "$a" archiver "$s" "$s/resultats.json"
```

Pour relancer seulement `ram`, utiliser `python3 -B novaOS/tests/ipc_phase4_ablations.py mesurer comparaison novaOS/build/rejeu-ram --variante ram`. Le rejeu ciblé produit trois démarrages. `archiver` attend les 42 démarrages et les quatre mesures mémoire d’une campagne complète.
