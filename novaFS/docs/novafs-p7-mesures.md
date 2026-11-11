# NovaFS P7 — Mesures sur images Darwin

Le même Mach-O par architecture exécute le chemin de compatibilité P6 et les deux profils du format P7, volume en clair et domaine chiffré. Les valeurs P6 mesurent ainsi un volume de format P6 avec le moteur de cette livraison. Chaque processus démarre avec une image temporaire neuve de 4 096 blocs de 4 Kio. Il crée un volume de 256 blocs, écrit un fichier de 16 Kio, crée un snapshot et un clone, puis alterne 32 écritures de 8 Kio dans le clone et 32 relectures vérifiées de 16 Kio. Les secteurs de 512 et 4 096 octets, les politiques brute et LZ4 et les contenus répétitifs et pseudo-aléatoires ont les mêmes graines, tailles et opérations sur les deux Mach-O.

Deux répétitions ont été exécutées en série par architecture. L'ordre P6/P7 s'inverse entre les répétitions. Les percentiles prennent le rang supérieur sans interpolation. Chaque ligne de la première table regroupe les deux géométries et les deux répétitions, soit 128 appels en lecture et 128 en écriture. Les débits rapportent les octets logiques à la somme des durées murales. Les durées intègrent les attentes de l'hôte. Le cache du système de fichiers macOS est actif. Les autres processus de la machine ne sont pas isolés. Le banc conserve le moteur monté entre les appels et vérifie le résultat de chaque lecture hors chronométrage.

## Lectures et écritures des clones

| ISA | Format | Encodage | Données | Lecture Mio/s | Lecture p50/p99 ms | Écriture Mio/s | Écriture p50/p99 ms | Amplification physique écriture |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 | P6 | brut | répétitives | 88,10 | 0,09 / 0,52 | 0,77 | 9,78 / 20,32 | 5,5× |
| ARM64 | P6 | brut | aléatoires | 127,89 | 0,08 / 0,45 | 0,83 | 9,30 / 11,46 | 5,5× |
| ARM64 | P6 | LZ4 | répétitives | 86,21 | 0,09 / 0,57 | 0,78 | 9,82 / 12,34 | 5,0× |
| ARM64 | P6 | LZ4 | aléatoires | 84,73 | 0,08 / 1,21 | 0,59 | 10,22 / 146,17 | 5,5× |
| ARM64 | P7 clair | brut | répétitives | 2,97 | 4,25 / 10,73 | 0,35 | 20,83 / 42,81 | 14,5× |
| ARM64 | P7 clair | brut | aléatoires | 3,18 | 4,25 / 10,14 | 0,36 | 20,88 / 48,04 | 14,5× |
| ARM64 | P7 clair | LZ4 | répétitives | 3,76 | 4,13 / 4,63 | 0,38 | 20,60 / 26,29 | 13,5× |
| ARM64 | P7 clair | LZ4 | aléatoires | 3,76 | 4,15 / 4,88 | 0,39 | 20,17 / 22,37 | 14,5× |
| ARM64 | P7 chiffré | brut | répétitives | 3,13 | 4,83 / 10,68 | 0,36 | 22,01 / 26,94 | 14,5× |
| ARM64 | P7 chiffré | brut | aléatoires | 2,47 | 4,92 / 15,56 | 0,33 | 22,37 / 45,76 | 14,5× |
| ARM64 | P7 chiffré | LZ4 | répétitives | 3,10 | 4,90 / 10,74 | 0,35 | 22,16 / 34,84 | 13,5× |
| ARM64 | P7 chiffré | LZ4 | aléatoires | 3,18 | 4,87 / 5,29 | 0,35 | 22,30 / 28,66 | 14,5× |
| Rosetta | P6 | brut | répétitives | 134,46 | 0,11 / 0,16 | 0,77 | 10,05 / 11,62 | 5,5× |
| Rosetta | P6 | brut | aléatoires | 139,59 | 0,11 / 0,12 | 0,77 | 10,06 / 12,48 | 5,5× |
| Rosetta | P6 | LZ4 | répétitives | 139,22 | 0,11 / 0,12 | 0,78 | 10,02 / 12,28 | 5,0× |
| Rosetta | P6 | LZ4 | aléatoires | 136,00 | 0,11 / 0,14 | 0,79 | 9,65 / 11,58 | 5,5× |
| Rosetta | P7 clair | brut | répétitives | 2,21 | 6,15 / 22,45 | 0,26 | 26,54 / 57,67 | 14,5× |
| Rosetta | P7 clair | brut | aléatoires | 2,25 | 6,16 / 13,79 | 0,25 | 26,78 / 166,89 | 14,5× |
| Rosetta | P7 clair | LZ4 | répétitives | 2,55 | 6,12 / 6,55 | 0,29 | 26,71 / 32,21 | 13,5× |
| Rosetta | P7 clair | LZ4 | aléatoires | 2,55 | 6,13 / 6,39 | 0,30 | 26,41 / 30,22 | 14,5× |
| Rosetta | P7 chiffré | brut | répétitives | 2,07 | 7,44 / 10,59 | 0,25 | 30,49 / 44,28 | 14,5× |
| Rosetta | P7 chiffré | brut | aléatoires | 2,00 | 7,42 / 20,86 | 0,25 | 30,23 / 44,12 | 14,5× |
| Rosetta | P7 chiffré | LZ4 | répétitives | 1,83 | 7,36 / 19,10 | 0,25 | 30,02 / 69,47 | 13,5× |
| Rosetta | P7 chiffré | LZ4 | aléatoires | 1,91 | 7,49 / 19,45 | 0,26 | 29,98 / 48,49 | 14,5× |

P7 relit le graphe de la vue et ses métadonnées de contenu à chaque lecture. Son chemin d'écriture remonte et vérifie le graphe des vues protégées avant de muter l'image. Ces contrôles sont également présents sur le volume en clair au format P7. L'écart P6/P7 comprend donc ce remontage, les lectures, l'authentification des liens et, suivant le profil, l'AEAD. La création d'un snapshot ou d'un clone partage les unités existantes. Son temps inclut néanmoins la relecture du graphe protégé et peut croître avec la quantité de données du pool. L'amplification compte les blocs de 4 Kio soumis au backend, y compris objets et publications. Le backend enregistre deux synchronisations pour chacune des écritures chronométrées.

## Opérations de contrôle et de sauvegarde

Le tableau suivant porte sur le contenu répétitif et la politique LZ4. Les opérations ponctuelles ont quatre échantillons. Les restaurations P7 provisionnent toujours un volume chiffré neuf et exécutent Argon2id. L'export et la restauration indiquent le débit du fichier utile de 16 Kio. L'archive transporte aussi les pages de métadonnées et le contenu nécessaire aux deux générations internes.

| ISA | Format | Snapshot / clone p50 ms | Montage / vérification p50 ms | Export / restauration Mio/s | F1+F2 moyen par écriture ms |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM64 | P6 | 8,11 / 8,12 | 5,27 / 5,22 | 5,14 / 1,07 | 8,55 |
| ARM64 | P7 clair | 12,07 / 11,08 | 0,34 / 9,33 | 0,68 / 0,19 | 9,23 |
| ARM64 | P7 chiffré | 13,52 / 11,96 | 0,33 / 11,10 | 0,57 / 0,19 | 9,07 |
| Rosetta | P6 | 8,20 / 8,62 | 8,07 / 8,00 | 4,44 / 0,98 | 8,81 |
| Rosetta | P7 clair | 14,54 / 12,38 | 0,61 / 14,40 | 0,44 / 0,10 | 9,46 |
| Rosetta | P7 chiffré | 16,12 / 12,73 | 0,59 / 17,40 | 0,37 / 0,10 | 9,33 |

## CPU et mémoire

Le microbanc CPU mesure 128 scellements et 128 ouvertures de 4 Kio par ISA. Le scellement inclut le tirage CSPRNG. Les pointes de tas mesurent les allocations vivantes de l'allocateur Rust selon leur taille Darwin, arrondi compris. On y ajoute toute la pile réservée du thread de travail, dont la taille est interrogée dans chaque processus. Le banc relève le RSS maximal de chaque processus fils avec `wait4`. Cette mesure inclut le laboratoire, les bibliothèques et, sur x86_64, Rosetta. Le tas initial et final est comparé après destruction de chaque pool et de chaque archive.

| ISA | AEAD CPU scellement / ouverture µs par 4 Kio | Pic travail hors KDF Mio | Pic Argon2id au déverrouillage Mio | Déverrouillage Argon2id p50 ms | RSS max du processus Mio | Processus avec tas intégralement restitué |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ARM64 | 7,71 / 6,44 | 2,26 | 66,06 | 63,64 | 68,39 | 48/48 |
| Rosetta | 9,88 / 9,48 | 2,28 | 66,05 | 116,74 | 70,77 | 48/48 |

La création d'un domaine avec phrase et la restauration d'une archive dans un domaine neuf incluent le travail Argon2id. Le fichier de mesures détaille aussi ces pics combinés. Le plafond de 7 Mio s'applique aux charges de travail hors KDF. Les 64 Mio du profil Argon2id ont un budget distinct. Les mesures couvrent la charge et la taille de vue décrites ici. Les pointes disque sur des charges plus longues, la charge concurrente, la résistance physique aux coupures et la taille maximale théorique de tout graphe restent hors du corpus.

## Reproduction

Depuis la racine du dépôt, sur Mac Apple Silicon avec Rosetta, les deux cibles Rust Darwin et les dépendances du Cargo.lock disponibles hors ligne, lancer le banc ci-dessous. Le rapport JSON de distributions contient p50, p95, p99, maximum, CPU, flux, amplification et mémoire par groupe. Les calculs restent possibles à partir du journal individuel conservé avec la version.

```sh
resultats=$(mktemp -d)
python3 novaFS/lab/p7-validation.py tout --sortie "$resultats"
```

[260 distributions avec p50, p95, p99, maximum et nombre d’appels](validation/v0.9.9/p7-distributions.json) · [Observations, environnement, empreintes des Mach-O, RSS et restitution du tas](validation/v0.9.9/p7-mesures.jsonl.gz) · [Échanges P7 entre architectures](validation/v0.9.9/p7-echanges.json)
