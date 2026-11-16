# Recette ARC-027 · sort 0.1.0 · 16 novembre 2026

**macOS ARM64 : tri supérieur à la RAM démontré. NovaOS AArch64 et x86_64 : fichier P9 supérieur à la RAM : non qualifié.** Les trois binaires sont archivés avec leurs empreintes. L’acceptation globale reste ouverte sur NovaOS. [Matrice par option](MATRICE.md) · [Arbitrages requis](ADMISSION.md)

## MacBook Pro M4 Max

Le fichier pseudoaléatoire occupe 171 798 691 840 octets logiques et 171 811 274 752 octets alloués. Il représente 1,25 fois les 137 438 953 472 octets de RAM physique. Ses 41 943 040 enregistrements de 4 096 octets portent une clé pseudoaléatoire et un numéro de séquence. La génération (graine 27) a pris 97,22 s, sur SSD interne APFS avec 574 436 589 568 octets disponibles au départ. Le grand fichier de travail a été supprimé après vérification. [Manifeste du corpus](results/corpus-grand.json) · [Environnement](results/environnement.json)

| Darwin ARM64, livraison | Temps réel | CPU utilisateur + système | Débit d’entrée | RSS du tri | Pic temporaire échantillonné |
|---|---:|---:|---:|---:|---:|
| Nova sort 0.1.0 | 228,745 s | 79,069 + 64,580 s | 716,26 Mio/s | 2 167 554 048 octets (2,02 Gio) | 171 822 088 192 octets (160,02 Gio) |
| GNU sort 9.12 | 340,596 s | 50,763 + 170,726 s | 481,04 Mio/s | 4 299 997 184 octets (4,00 Gio) | 172 457 070 592 octets (160,61 Gio) |

Un passage par exécutable dans cette comparaison finale, GNU puis Nova, en locale C, cache non purgé. Les deux commandes prennent `-S 2G --parallel=1 --batch-size=128`. Elles écrivent dans le même type de pipe, dont le consommateur vérifie l’ordre, le nombre de lignes, la taille et le SHA-256. Le temps réel inclut la contre-pression du consommateur. Le CPU et le RSS proviennent de `wait4` pour le processus de tri seul. Des qualifications QEMU se déroulaient sur le Mac pendant une partie des campagnes. Ces valeurs caractérisent les exécutions consignées. [Mesures brutes de livraison](results/livraison-160g.json)

Les deux sorties font 171 798 691 840 octets et ont pour SHA-256 `0175f7b0b711d06b569d0446c2a8f15744dbae66a7e7e9f5225d0cd3ba1d76bb`. Le candidat a écrit exactement 160 Gio dans 81 lots, fusionnés en une passe. Les deux répertoires de lots étaient vides à la sortie. Le rapport brut donne le pic physique échantillonné toutes les 50 ms, les tailles maximales observées par fichier, les défauts de page et les compteurs Darwin. Les octets physiques transférés par le SSD et les passes internes de GNU ne sont pas instrumentés. Les trois objectifs préalables de Nova (200 Mio/s, 4 Gio RSS, 165 Gio temporaires) sont atteints. [Objectifs du pilote](results/objectifs.md)

Quatre comparaisons de 160 Gio ont été exécutées au cours du développement. La première a utilisé des fichiers de sortie réguliers (252,330 s et 323,985 s pour Nova et GNU). Les deux comparaisons intermédiaires en flux portaient sur des Mach-O successifs. Le tableau ci-dessus identifie la comparaison effectuée avec le Mach-O livré. [Sortie régulière](results/grand.json) · [Premier binaire en flux](results/grand-final.json) · [Deuxième binaire en flux](results/recette-160g.json)

| Petit palier sur le Mach-O livré | Passages par moteur | Nova, médiane [min – max] | GNU, médiane [min – max] |
|---|---:|---:|---:|
| 1 Gio, aléatoire, tampon de 8 Mio | 3 | 1,053 [1,052 – 1,199] s | 4,254 [4,217 – 4,318] s |
| 16 Mio pseudoaléatoires | 3 | 0,0217 [0,0213 – 0,0218] s | 0,0924 [0,0924 – 0,0961] s |
| 16 Mio déjà triés | 3 | 0,0231 [0,0221 – 0,0233] s | 0,0967 [0,0924 – 0,0979] s |
| 16 Mio inversés | 3 | 0,0214 [0,0213 – 0,0227] s | 0,0932 [0,0911 – 0,0994] s |
| 16 Mio, 17 clés répétées | 3 | 0,0229 [0,0221 – 0,0231] s | 0,0942 [0,0929 – 0,1030] s |

À 1 Gio, le candidat a créé 149 fichiers temporaires au total, effectué trois passes et écrit 3 209 166 848 octets temporaires. Pour les distributions de 16 Mio, le budget était de 256 Kio avec huit lecteurs. Un passage distinct a exercé deux travailleurs. Les tests de stabilité et d’unicité comparent les représentants entre lots avec les octets attendus de GNU. [Palier 1 Gio](results/final-1g.json) · [Deux travailleurs](results/final-parallele.json) · [Tests de stabilité](results/options.json)

## Invités QEMU

Les profils emploient 2 vCPU et 256 Mio configurés, confirmés par QMP (268 435 456 octets), avec inventaire des pages du noyau sur UART. Trente vecteurs de l’oracle ont réussi dans le témoin SDK sur les deux ISA, avec sortie 0 et restitution du processus. Les quatre démarrages ordinaires valident l’aide, le manuel et la version. L’appel direct et les deux pipes demandés avec cat et head sont refusés par le lanceur de la branche sort. [Preuves des invités](results/qemu)

L’essai de bout en bout utilise un **catalogue récepteur de test**, dans un checkout indépendant au commit `6b94f2f6349add9fc6ac0bc8276ab1b8747a5e36`. Son noyau embarque les ELF exacts de la livraison. Le contrôle ordinaire compare la totalité des 64 comptes de `sort donnees | uniq -c` à GNU. Le passage de diagnostic vérifie les deux statuts et le retrait du pipe. Une relecture de P9 vérifie le fichier source et l’absence de lots de sort résiduels.

| ISA / secteur | Entrée unique triée | Temps du pipeline ordinaire | Fichier de 320 Mio avec 256 Mio de RAM |
|---|---:|---:|---|
| AArch64 / 512 | 256 Kio / 8 192 lignes | 4,849 s | Service P9 en E/S avant nsh |
| AArch64 / 4 096 | 256 Kio / 8 192 lignes | 4,638 s | Même issue |
| x86_64 / 512 | 256 Kio / 8 192 lignes | 4,344 s | Même issue |
| x86_64 / 4 096 | 256 Kio / 8 192 lignes | 4,381 s | Même issue |

Le tri de 128 Kio réussit également dans les quatre profils. Les deux enfants rendent 0 à chaque palier réussi. Ces chronomètres couvrent le pipeline complet. Le pic mémoire et le pic temporaire de sort seul n’ont pas été mesurés dans ces petits invités. Le RSS du processus QEMU sur Mac n’entre pas dans les mesures invitées.

Le fichier P9 dense de 335 544 320 octets et 81 920 lignes a été écrit en 52,012 s, puis vérifié intégralement sur le Mac après remontage en lecture seule (0,853 s). Son SHA-256 est `022d496ef92432d735d3dd57e05bca40fc1710c876ee8b659b0ae27f2c1bef6c`. Quatre clones de l’image de 1 Gio ont été attachés aux invités. Le service de fichiers a rendu 26 et le client 8 avant le lancement de sort dans les quatre cas. **Aucune durée de tri, aucun RSS de sort et aucun pic de temporaire ne peuvent être attribués à ces essais de 320 Mio.** L’intervalle entre les paliers réussis et refusés ne constitue pas une limite universelle du format P9. [Création et relecture P9](results/qemu/creation-p9.log) · [Empreinte du fichier P9](results/qemu/corpus-320m.sha256)

## Pannes et restitution

| Essai sur le catalogue récepteur | Résultat observé sur les quatre profils |
|---|---|
| 2 Mio de volume, 40 blocs disponibles avant lancement | La création des lots rencontre `FULL=23`. Sort rend 2, uniq rend 0. La source est intacte, les lots résiduels sont au nombre de zéro. |
| Arrêt forcé de QEMU pendant la génération des lots | L’inspection de l’image arrêtée compte 11 lots. Après redémarrage, elle en compte zéro. La source garde son empreinte. |
| Lecture VirtIO suspendue après la première réception de données triées | La fusion a déjà commencé. Sort signale `IO=26` et rend 2. Uniq consomme un préfixe et rend 0. Le pipe est retiré, la session de stockage se termine en erreur. Les trois lots présents après l’arrêt sont récupérés au redémarrage et la source est intacte. |

Sur la branche uniq et la version antérieure des ELF épinglée à `fe65061`, les journaux à 4 Mio sur AArch64/512 et x86_64/4 096 montrent une expiration du lecteur avant la première trame. Sort constate la révocation du port (11) et rend 2. Uniq rend 8. Ce palier complémentaire ne qualifie pas la fermeture de head ni la révocation d’un handle P9 sur les ELF de livraison. [Preuves du palier à 4 Mio](https://github.com/NaviStorm/NovaOS/tree/0539455d3d32c865d5b3deafc943fffe4bf13402/livraison-uniq-0240/preuves/palier-4Mio)

Sur macOS, deux tris simultanés dans le même répertoire de temporaires réussissent avec des sorties exactes et sans résidu. Une erreur de décompression pendant la fusion rend 2. Sur NovaOS, le test de deux tris actifs simultanément dans une même session et le retrait explicite du droit de lecture P9 restent ouverts. [Témoins Darwin](results/limites.json) · [Décisions d’admission](ADMISSION.md)

## Régressions

Le catalogue documentaire a réussi ses cinq tests. Les cibles `cli-p9-test`, `nsh-test` et `nsh-scripts-test` ont réussi pour AArch64 et x86_64. Les manuels français et anglais ont été rendus avec mandoc. Les campagnes noyau générales (25 démarrages, bloc, DMA, UTM) n’ont pas été relancées dans le lot sort. Le noyau, l’ABI v1 et les sources du moteur NovaFS P9 ne figurent pas dans le diff publié. [Catalogue](results/catalogue-doc.txt) · [AArch64](results/qemu/aarch64/regression.log) · [x86_64](results/qemu/x86_64/regression.log)

Le différentiel a révélé une perte de données dans `sort -m -o f < f`. Sur Darwin, l’identité de `/dev/stdin` ne permettait pas d’identifier le fichier redirigé. Le correctif utilise `fstat(0)` avant toute troncature. Les suites ont aussi détecté le comptage des descripteurs hérités, les diagnostics dépendants de la locale et le traitement d’un compresseur absent. Les témoins amont et les jeux d’octets de référence ont été rejoués après correction.
