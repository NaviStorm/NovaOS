# Qualification de sha256sum 0.24.0 · 16 novembre 2026

La branche `codex/sha256sum-natif-0240` part de `ea2d0c3cd5a38671ebdf263c8eeca72b3cabcff2`. Le Mach-O, les deux ELF et les médias ordinaires livrés ont été vérifiés sur les sources `7c45dd05bdb77149a615137e3bb2b2003986acce`. Rust stable 1.99.1, GNU coreutils 9.12, QEMU 11.1.3.

## Calcul et manifestes

| Cible | Calcul | Vérification |
|---|---|---|
| macOS ARM64 natif, Apple M4 Max | Vecteurs indépendants vide, `abc` et un million de `a`, chacun en un bloc, octet par octet et par fragments irréguliers. Erreur de lecture injectée après un préfixe. | 208 comparaisons de statut, stdout et stderr avec GNU en locale C. Émission et relecture croisées des formats usuel et BSD, chemins relatifs au répertoire courant, noms échappés, silence et échec après altération d’un bit. |
| NovaOS AArch64 | Fichier P9 binaire et `cat fichier.bin \| sha256sum` jusqu’à EOF. | Manifeste GNU sur P9. Deux concordances sur le volume intact, statut 0 en mode silencieux. Un bit modifié donne uniquement `fichier.bin: FAILED`, `notice: OK`, statut 1. |
| NovaOS x86_64 | Même corpus, lecture directe et pipeline exécutés par les ELF natifs. | Même verdict sur les deux géométries et sur les deux volumes. |

L’attendu de `fichier.bin` est `cee3b28d18c79e728bc6d7ff21a913890113a6e1c97ab9f133f7e0f899978a64`. La recette comporte 32 commandes enregistrées avec le catalogue ordinaire et 33 avec le profil de contrôle, pour chacun des quatre couples architecture/secteur. Les 260 observations sont livrées avec les traces série et les identités des médias. Les ELF distribués correspondent octet par octet à ceux des noyaux ordinaires exercés.

## Capacités et non-régression

Dans les quatre profils P9 à deux vCPU et 256 Mio, le lecteur du pipe a reçu zéro capacité fichier. Le producteur et le lecteur étaient deux PID distincts, collectés avec statut 0. Le refus du voisin figure à côté de la concordance du fichier autorisé. `--ignore-missing` conserve le refus. Les deux orthographes `/voisin` et `../voisin` restent refusées. Après révocation au passage des premiers 4 096 octets, le statut vaut 1 et aucune empreinte du préfixe ou du fichier entier n’est émise. Les deux pipes historiques `cat notice | wc -l` et `cat notice | head -n 1` ont donné respectivement `1` et `livraison`.

Les appels documentaires anglais et français, la version et le manuel ont réussi. Les compteurs QMP d’écritures et de FLUSH ainsi que le SHA-256 de l’image avant et après la fenêtre documentaire sont identiques dans chaque profil. Les autorisations explicites, les plafonds d’arguments, le tube sans option, le terminal texte, les noms de fichiers et les conditions de comparaison avec GNU sont détaillés dans [la matrice](MATRICE.md).

Les cibles de régression ont été lancées individuellement par `make -C novaOS ARCH=…` avec `test`, `elf-test`, `lifecycle-test`, `ipc-races`, `block-test`, `cli-test`, `cli-p9-test`, `nsh-test`, `nsh-scripts-test`, `nsh-terminal-test` et `vi-test`. Les 22 exécutions ont terminé avec le statut 0. La vérification de cohérence de l’ABI a réussi. Les cinq tests du catalogue documentaire ont aussi réussi sur Darwin ARM64 natif et x86_64 sous Rosetta. Les résultats NovaOS proviennent de QEMU direct. L’application UTM et le matériel physique n’ont pas été exercés.

## Performances sur macOS ARM64

Les trois séries usuelles hachent 1, 16 et 64 Mio, puis vérifient 1 000 fichiers de 1 024 octets. Les mesures ci-dessous indiquent la médiane et les extrêmes de trois exécutions en Mio/s. Le temps réel inclut le lancement et la sortie. Pour le pipe, il inclut le générateur. Le CPU et le maximum résident proviennent de `wait4` sur le seul processus de calcul. Les fichiers ont été préparés hors chronométrage, sans purger le cache de macOS.

| Exécutable | Fichier 64 Mio | Flux 64 Mio | Manifeste de 1 000 fichiers |
|---|---:|---:|---:|
| Mach-O distribué | 1 919 [1 768–1 928] | 1 955 [1 949–1 984] | 45,6 [41,4–48,1] |
| Même source, SHA logiciel forcé | 537 [531–537] | 534 [532–545] | 56,0 [55,0–58,5] |
| GNU Homebrew | 2 029 [1 962–2 072] | 2 067 [2 029–2 116] | 57,1 [56,6–58,0] |

Sur le même tampon réutilisé pendant cinq calculs de 256 Mio, le moteur SHA2 ARM a produit 2 269 Mio/s en médiane [2 267–2 352], le moteur logiciel 523 Mio/s [519–530]. Tous deux ont donné `cd9dd169dde4047ac96a92eee5873890506fb71cf9aec657c5403629bb402f82`. Le M4 Max expose FEAT_SHA256 et le Mach-O distribué contient les instructions `sha256h` et `sha256h2`.

Le Mac dispose de 137 438 953 472 octets de RAM et de 16 CPU logiques. Le fichier creux de 137 438 957 569 octets a occupé 2 105 344 octets physiques. Les deux outils ont aussi reçu exactement 137 438 957 569 octets par stdin. Les quatre empreintes concordent avec la référence indépendante `908f3ecb4ae2cabb10964464475e7c604f5600229d8fbb204562c7b099a36be7`.

| Profil au-delà de la RAM, un passage | Temps réel (s) | CPU du processus (s) | Débit (Mio/s) | Pic RSS (octets) |
|---|---:|---:|---:|---:|
| Mach-O, fichier | 67,09 | 65,86 | 1 954 | 2 048 000 |
| Mach-O, flux | 113,24 | 67,93 | 1 157 | 2 064 384 |
| GNU, fichier | 61,68 | 58,26 | 2 125 | 2 342 912 |
| GNU, flux | 56,04 | 54,51 | 2 339 | 2 392 064 |

Le corpus porte un marqueur SHA-512 dérivé de l’offset à chaque Gio ainsi qu’une marque finale. Pour 4 294 971 393 octets, le chemin fichier et le flux ont donné `142d09ad957fac7d6a0c338329ff8af0c182fb3608e39e3c99333acae6376b7c`. Inverser le dernier bit a donné `1d2f9fbc2f54201862779af289c4a25b4ab21051778fbc4b6b9a79a4e3c88ed0` après relecture indépendante par hashlib. Les séries complètes et les scripts du générateur permettent de reproduire ces valeurs.

## Performances sur NovaOS

Les invités TCG utilisent deux vCPU, 128 Mio de RAM, le chemin SHA logiciel et un disque VirtIO P9 de 2 Mio en cache writeback. La machine AArch64 est une `virt` avec Cortex-A72 et GICv2. La machine x86_64 est une Q35 avec CPU `max`. Le lecteur traite le fichier directement ou passe par un second processus `cat`. Le chronométrage inclut les IPC, les processus, le transport et la sortie série. Le fichier creux porte un octet `b7` en dernière position, après 134 217 728 zéros. Les huit lectures ordinaires du grand fichier ont donné `94e4f6bfe17464430d9aaa3328a1bf5d7b12586b6f8f8c05f6885c2580d33559`.

| Architecture · secteur | Lecture P9 (s) | Lecture P9 (Mio/s) | Pipeline (s) | Pipeline (Mio/s) |
|---|---:|---:|---:|---:|
| aarch64 · 512 | 527,630 | 0,243 | 692,050 | 0,185 |
| aarch64 · 4096 | 457,950 | 0,280 | 545,149 | 0,235 |
| x86_64 · 512 | 421,203 | 0,304 | 601,600 | 0,213 |
| x86_64 · 4096 | 408,057 | 0,314 | 454,148 | 0,282 |

Chaque grande mesure couvre exactement 134 217 729 octets et un passage. Les relevés en secteurs de 512 proviennent du commit d’intégration `04f7af8`. Les relevés en secteurs de 4 096 utilisent les noyaux ordinaires livrés de `7c45dd0`. Chaque JSON de VM identifie son noyau et son ISO. Les durées dépendent de l’émulation, du stockage et de la charge du Mac, qui exécutait aussi des régressions. Les trois répétitions à 16 385 et 131 073 octets et la vérification des douze petits fichiers figurent dans les JSON avec les médianes et les extrêmes. Le profil à 64 Mio s’arrête avant le noyau dans EDK II sur AArch64 et dans Limine sur x86_64.

Le profil instrumenté additionne les pages privées à l’admission et le pic d’extension du tas. En secteurs de 4 096 octets, le grand fichier et son pipeline atteignent chacun **380 928 octets sur AArch64** et **385 024 octets sur x86_64**. Les fichiers de 16 et 128 Kio donnent les mêmes pics. La lecture et le hachage utilisent une fenêtre de 4 Kio. Le processus QEMU AArch64 a consommé 705,5 s de CPU pour le grand fichier et 767,3 s pour le pipe. Sur x86_64, ces durées sont de 589,7 et 690,4 s.

En secteurs de 512, les temps médians réel/CPU du processus **QEMU entier** pour 131 073 octets sont de 0,778/1,057 s en lecture et de 1,121/1,281 s en pipe sur AArch64. Sur x86_64, ils valent 0,603/0,784 s et 0,739/1,056 s. La vérification des douze fichiers prend respectivement 0,136/0,163 s et 0,102/0,142 s. L’ABI utilisée ne fournit pas le temps CPU propre de l’ELF invité. Le compteur hôte cumule tous les threads du moteur.

## Relancer et vérifier la livraison

Les trois binaires annoncent `sha256sum (NovaOS) 0.24.0`. Depuis le répertoire livré sur Mac, exécuter les contrôles suivants. GNU Homebrew est l’oracle du test différentiel.

```sh
./sha256sum --quiet -c SHA256SUMS
/opt/homebrew/bin/gsha256sum --quiet -c SHA256SUMS
./tester-options-sha256sum.sh --novaOS
./mesurer-performances-sha256sum.sh --help
man -M ./man sha256sum
```

Les scripts, les versions et les licences accompagnent les exécutables. Pour l’administrateur sur NovaOS, [la recette depuis nsh](RECETTE-NSH.md) fonctionne sur les images P9 intacte et altérée incluses. La concordance avec le manifeste et l’authenticité de sa provenance sont deux vérifications distinctes.
