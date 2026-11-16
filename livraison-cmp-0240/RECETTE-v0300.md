# Recette macOS de cmp pour v0.30.0

Le 17 novembre 2026, le banc a comparé le Mach-O ARM64 Nova 0.24.0 de la livraison au véritable GNU cmp de Diffutils 3.12, sous macOS 27.2.1 ARM64. Pour exercer une future construction, définir `NOVA_CMP` au chemin absolu de son Mach-O. `GNU_CMP` permet de désigner une autre construction de GNU 3.12. La version de la référence est contrôlée au démarrage. Les empreintes des deux exécutables figurent dans les résultats.

## Relance

Depuis la racine du worktree, avec Python 3.11 ou ultérieur sur le Mac ARM64, utiliser une de ces deux commandes.

```sh
./livraison-cmp-0240/recette-macos.sh
./livraison-cmp-0240/recette-macos.sh --large
```

Le profil rapide compare des fichiers de 1 Mio. Le profil complet ajoute 100 Mio et 1 Gio ainsi qu’un cas fonctionnel de saut au-delà de 4 Gio dans deux fichiers creux. Les fichiers temporaires sont générés dans `OS-worktrees/tmp`, configurable par `NOVAOS_TMPDIR`. Chaque invocation crée dans `OS-worktrees/livraisons` un répertoire neuf dont elle affiche le chemin. `CMP_RESULTATS` permet de choisir un autre répertoire parent. Le manifeste SHA-256 est vérifié après écriture.

Les deux entrées autonomes restent [tester-options-cmp.sh](tester-options-cmp.sh) et [mesurer-performances-cmp.sh](mesurer-performances-cmp.sh). Elles acceptent `--large` et `--json chemin`. La recette commune exécute aussi [les contrôles Darwin de stdin et du pipe](flux-darwin.py).

## Résultats rapides

- **482 cas par outil**, dont 25 documentaires. Les options simples ouvrent la suite, puis viennent les combinaisons, les positions de différence, les limites et les erreurs. Les entrées sont identiques pour les deux processus. Le banc relève le statut de chacun, stdout et stderr indépendamment, octet par octet. Pour l’aide et la version, les textes attendus dépendent de l’outil et de la langue. Nova a rendu 154 fois 0, 204 fois 1 et 124 fois 2. [Mesures fonctionnelles](preuves/bancs-v0300/options.json)
- Le témoin `-l` de 256 Kio diffère au seul premier octet. La sortie complète vaut `20202020203120202030203133300a` en hexadécimal pour les deux outils, stderr est vide. Le processus Nova rend **1** et GNU rend **0**. En performance, la liste des trois différences début/milieu/fin sur les tailles de bloc exercées présente aussi cet écart. Le banc vérifie les positions affichées, les flux et chaque statut. [Matrice de l’arbitrage](MATRICE.md)
- **12 contrôles de flux** sur les deux outils. Un stdin ouvert en écriture seule donne 2. Le pipe Darwin avec le véritable processus cat donne 0/0 à égalité et SIGPIPE/1 sur différence précoce. [Relevés de processus](preuves/bancs-v0300/flux.json)

| Taille ou lignes | Traitement | Nova cmp | GNU cmp |
|---|---|---:|---:|
| 1 Mio | Différence au premier octet | 1,530 ms | 1,437 ms |
| 1 Mio | Fichiers distincts égaux | 1,802 ms · 554,9 Mio/s | 1,769 ms · 565,3 Mio/s |

Deux fichiers écrits indépendamment ont des inodes distincts. Le cas « même objet » mesure séparément le raccourci d’identité. Chaque ligne comporte un échauffement par outil et trois passages chronométrés en ordre alterné. Le temps va de la création du processus à sa collecte. La génération des fichiers et la vérification des sorties se font hors chronométrage. Les deux outils écrivent vers les mêmes fichiers ordinaires, le cache reste chaud. Le débit utilise les octets comparés dans un fichier. La médiane et le maximum des pics RSS des trois processus complètent les mesures individuelles. [Dix traitements](preuves/bancs-v0300/tableau.txt) · [Temps bruts, débits et RSS en octets](preuves/bancs-v0300/performances.json)

## Portée des preuves

La nouvelle exécution couvre le Mach-O ARM64. Les campagnes complètes précédentes à 1 Mio, 100 Mio et 1 Gio sont conservées avec leurs valeurs originales. Les sondes SDK exercent le moteur sur entrées mémoire. La recette historique sous nsh et P9 a chronométré le parcours intégral jusqu’à 1 Mio. L’archive de pause du 16 novembre porte sur quatre profils de nsh avec cinq racines IPC. Le correctif à quatre racines reste non qualifié. Aucun des résultats de cette recette macOS ne mesure le lanceur commun ou une image v0.30.0. [Campagne complète Darwin](preuves/performances.json) · [Mesures invitées historiques](preuves/tableau-invites.txt) · [Qualification SDK et P9](README.md#qualification)
