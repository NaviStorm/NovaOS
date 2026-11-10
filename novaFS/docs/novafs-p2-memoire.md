# NovaFS P2 v0.9.2 — mémoire du vérificateur

La campagne utilise les binaires macOS de release sur Apple Silicon, ARM64 natif et x86_64 sous Rosetta, construits avec Rust 1.99.1. Pour chaque couple architecture/secteur, elle lance deux processus indépendants par image, une CLI standard et une CLI construite avec `--features check-metrics`. Elle exige le même rapport JSON et le même code de sortie dans les deux cas.

## Images et périmètres

- **LV sain de 8 Gio.** Le banc crée un vrai fichier ordinaire creux de 2 097 152 blocs de 4 Kio. Le formateur vérifie les blocs vierges. Le générateur crée un répertoire contenant 96 fichiers, puis un second fichier écrit deux fois pour obtenir deux versions distinctes de ses données.
- **LV corrompu de 8 Gio.** Une copie APFS de l’image subit une modification d’un octet dans la page appartenant exclusivement à la dernière génération. Le banc exige le code 1, l’adresse précise du bloc corrompu et une seule génération en erreur. Le résultat standard doit reproduire le diagnostic instrumenté.
- **Pool sain de 17 Mio.** L’image de 4 352 blocs contient 16 LV prêts de 256 blocs avec données. Les seize diagnostics de contenu doivent conclure à l’intégrité. Le catalogue et la construction du rapport JSON font partie du pic. Le programme d’orchestration, les images et leurs copies de référence sont extérieurs au processus mesuré.
- Les entrées de la CLI sont ouvertes en lecture seule. Après les deux campagnes, `cmp -s` relit et compare chaque fichier à une copie prise avant les contrôles. Le fichier corrompu est comparé à la copie **déjà corrompue**.

Le pic du tas vient d’un allocateur global qui délègue à `System`, additionne les tailles effectives des allocations Rust vivantes (`malloc_size`) et conserve le maximum observé. Les réservations des deux exécutables sont contrôlées dans la commande Mach-O `LC_MAIN`. La CLI instrumentée interroge en outre `pthread_get_stacksize_np` pour la pile du thread qui effectue la vérification. Chaque exécution est mesurée par `/usr/bin/time -l`, qui rend la RSS maximale et le maximum comptabilisé comme `peak memory footprint` par Darwin. [Allocateur de mesure](../lab/heap_meter.rs) · [Orchestration et vérifications](../lab/measure-check.py)

## Résultats sur les deux architectures

Les 24 analyses ont donné les diagnostics attendus dans les deux géométries. Le fichier de 8 589 934 592 octets occupait 81 920 octets sur le système de fichiers hôte avant la mesure. La corruption a été localisée au bloc logique 5. Les trois comparaisons intégrales avant/après ont réussi. Les chiffres suivants, en octets, prennent le maximum de chaque métrique sur les deux géométries. Le relevé conserve aussi chaque code de sortie, les deux empreintes Mach-O, les valeurs de RSS et les `footprint` individuels. [Résultats mesurés](validation/v0.9.2/memoire.json)

| CLI / entrée | Pic du tas Rust instrumenté | Tas + pile réservée | RSS max instrumentée | RSS max standard |
| --- | ---: | ---: | ---: | ---: |
| ARM64, LV sain | 551 984 | 2 649 136 | 3 112 960 | 3 162 112 |
| ARM64, LV corrompu | 289 616 | 2 386 768 | 3 063 808 | 3 129 344 |
| ARM64, pool 16 LV | 212 240 | 2 309 392 | 2 965 504 | 2 883 584 |
| x86_64, LV sain | 551 888 | 2 649 040 | 5 345 280 | 5 251 072 |
| x86_64, LV corrompu | 289 520 | 2 386 672 | 4 648 960 | 4 562 944 |
| x86_64, pool 16 LV | 212 144 | 2 309 296 | 4 255 744 | 4 276 224 |

Le vérificateur réserve un bitmap de 256 Kio pour une génération de 8 Gio. La vérification de la paire valide nécessite simultanément deux bitmaps, soit 512 Kio. Lors du rejet d’une génération indépendante, la phase de comparaison de la paire est omise. Les rapports JSON et les allocations temporaires de décodage sont compris dans le pic. Les quatre exécutables réservent 2 097 152 octets de pile principale. Les deux exécutables instrumentés ont retrouvé cette valeur via Darwin.

La cible de 7 Mio vaut 7 340 032 octets. Le maximum mesuré du tas Rust et de la pile réservée vaut 2 649 136 octets. La RSS du binaire de distribution atteint 5 251 072 octets au maximum sur ces charges. Le binaire standard ne mesure pas la ventilation de son tas. La RSS comprend notamment les pages de l’exécutable et des bibliothèques et, sous x86_64, la traduction. Un cache du système de fichiers ou une réservation d’adresse virtuelle ne peut être attribué à une allocation Rust à partir de cette métrique. Le format borne le nombre de blocs mais cette campagne ne mesure pas tous les arbres remplissant un volume de 8 Gio.

Les 768 cycles de restitution de rapports ont réussi, soit 64 cycles × 3 issues × 2 secteurs × 2 architectures. Après chaque cycle sain, les allocations vivantes ont retrouvé les 79 696 octets initiaux sur ARM64 et 71 856 sur x86_64 ; après chaque cycle corrompu ou pauvre en crédit, les bases respectives étaient 80 784 et 72 944 octets. Le refus du crédit des bitmaps a conservé le pic à la ligne de base sur les quatre couples architecture/secteur. Ces comparaisons exercent les allocations Rust des chemins mesurés.

## Reproduction

Depuis la racine du dépôt, sur macOS avec un répertoire temporaire APFS, Rust stable, les cibles `aarch64-apple-darwin` et `x86_64-apple-darwin`, Rosetta et les dépendances Cargo disponibles hors ligne. L’environnement doit permettre à `/usr/bin/time -l` de consulter les compteurs Darwin. La commande génère les trois images sous un répertoire temporaire, y construit les exécutables et imprime un JSON. L’option `--pool` accepte également une fixture existante du scénario `p2_pool_image_de_mesure` en lecture seule.

```sh
python3 novaFS/lab/measure-check.py
```

Pour isoler la restitution dans la bibliothèque, le banc conserve un petit LV sain et un second avec une ancre corrompue. Après une chauffe, il exécute 64 analyses du même type dans le même processus, détruit chaque rapport et exige immédiatement le retour du tas à sa valeur initiale. Le troisième scénario fixe le crédit des bitmaps à zéro et exige deux diagnostics `NoMemory` sans sélection de génération. Les fichiers images, le runtime déjà initialisé et le simulateur encore vivant sont compris dans la ligne de base.

```sh
for arch in aarch64 x86_64; do
    for secteur in 512 4096; do
        NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" restitution-controle
    done
done
```
