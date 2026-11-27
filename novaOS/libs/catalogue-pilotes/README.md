# NovaOS v0.25.0 — Catalogue de pilotes hors ligne, phase 4B

**Révision Rust qualifiée** `621321850cffdd92fd54682ce04b42bb273e5752`, depuis l’architecture 4A `05b5788476735a97450ca490c0b86ad07c3ff8d7`. La bibliothèque `no_std + alloc` vérifie et sélectionne des pilotes pour un inventaire fourni par le banc. Le catalogue publie un plan en mémoire seulement après vérification de l’éligibilité, des droits, des dépendances et des quotas. L’admission vérifie les images de toutes les ISA annoncées. Un refus conserve le catalogue antérieur.

[Schéma v1 et protocole de confiance](SCHEMA.md) · [Exemple appelant l’API publique](examples/selection.rs) · [Vecteurs signés](vecteurs/) · [Décision durable proposée et prérequis 4C](AUTORITE.md) · [Architecture de référence](../../docs/gestionnaire-pilotes/README.md)

## Matrice hors ligne

18 tests fonctionnels réussis sur Darwin ARM64 natif et 18 sur Darwin x86_64 sous Rosetta. Le test de mesure marqué `ignore` dans la suite fonctionnelle a été exécuté séparément trois fois par architecture. Les bibliothèques release sont également compilées pour les deux cibles nues. Les exemples hôtes ont obtenu un plan pour chacune des deux ISA.

| Scénario | Essais exécutés et point d’effet observé | Preuve restant à fournir |
|---|---|---|
| T01 — Authenticité et compatibilité | Vecteurs produits par Python, vérifiés par Rust. Signatures détachées altérées ou tronquées, paquet ELF modifié même après recalcul de son empreinte et signature du manifeste, membre ou révision ELF erroné, condensat du membre, ISA, OS, ABI, rôles matériels, TOML malformé ou tronqué, 16 384/16 385 octets. Les admissions refusées ne publient aucune révision candidate | Mesure par le chargeur des octets effectivement exécutés et validation finale des segments |
| T02 — Politique de confiance | Catalogues fictifs de développement et de distribution avec clés distinctes. Clé étrangère, clé révoquée, transition vers un nouveau signataire, plancher et fenêtre d’époque, empreinte révoquée, profil obsolète ou différent. Un candidat local pourtant signé est refusé en distribution | Enrôlement réel, sortie du mode développeur au redémarrage et autorité survivante |
| T03 — Partie volatile | Admission idempotente, conflit de contenu pour le même ID/version, sélection de la nouvelle version et retour explicite à l’empreinte précédente, refus sous le plancher courant. L’échec du 65e paquet laisse 64 entrées | **T03 reste partiel.** Transactions persistantes d’installation/retrait, coupures, restauration et preuve indépendante de fraîcheur restent ouvertes |
| T04 — Résolution et classement | Absence de candidat, épingle indisponible, deux éditeurs signés à égalité, ordre inversé, version et priorité, graphe en diamant et cycle, dépendance manquante/facultative, permissions et quotas, correspondances PCI, USB et plateforme et variante de bus inconnue. Les inversions donnent le même plan ou le même conflit | L’inventaire, la topologie réelle et les ressources disponibles sont des entrées injectées. L’attestation d’un groupe matériel relève d’une autre phase |

Le catalogue ne demande aucune racine ni boîte IPC et ne touche aucun périphérique. La validation utilise le vérificateur de distribution existant par dépendance de bibliothèque. Le noyau, le manifeste interne v0.20.0, NovaAuth, le registre ABI, la garde Q35, Ethernet et NovaFS restent à leur révision de référence. Les suites historiques et QEMU n’ont pas été rejoués.

## Temps et mémoire

Médiane des p50 et médiane des p99 de trois processus de mesure indépendants par architecture. Une série compte 501 opérations pour les mesures cryptographiques et 1 001 pour les autres. Le catalogue de 64 paquets vérifie 64 manifestes distincts avant le chronométrage de sélection. Pour cette cellule un seul périphérique correspond, avec un profil système requis. Les exemples utilisent deux petits ELF fictifs. Un paquet atteignant la limite de 4 Mio demanderait un autre temps de hachage. [Sorties des six séries](qualification.txt)

| Opération, en µs (p50 / p99) | ARM64 natif | x86_64 Rosetta |
|---|---:|---:|
| Signature Ed25519 du manifeste, construction du message incluse | 31,250 / 44,375 | 52,333 / 63,417 |
| Admission complète, manifeste et deux paquets ELF | 112,459 / 148,125 | 187,375 / 213,666 |
| Résolution d’une dépendance | 0,125 / 0,250 | 0,209 / 0,334 |
| Sélection parmi 1 | 0,333 / 0,542 | 0,959 / 1,375 |
| Sélection parmi 64 | 0,875 / 1,084 | 2,291 / 2,375 |

| Mémoire, en octets | ARM64 | x86_64 |
|---|---:|---:|
| Objet de la bibliothèque pour cible nue, colonne `text` de llvm-size (code et constantes, avant édition de liens) | 198 509 | 266 590 |
| Données statiques modifiables de cet objet (`data + bss`) | 0 | 0 |
| Exemple Darwin lié, segments Mach-O `__TEXT / __DATA`, bibliothèque standard et dépendances incluses | 819 200 / 16 384 | 909 312 / 24 576 |
| Parseur, pic du tas pour 1 293 octets de TOML, dont 1 962 retenus après décodage | 31 024 | 31 024 |
| Parseur, même entrée complétée par des espaces jusqu’à 16 Kio | 134 536 | 134 536 |
| Catalogue retenu, 1 / 64 manifestes décodés | 2 492 / 159 488 | 2 492 / 159 488 |
| Admission de 64 paquets, pic au-dessus du niveau initial | 192 752 | 192 752 |
| Sélection, pic du tas / octets retenus par le plan | 1 527 / 280 | 1 527 / 279 |

L’allocateur du banc compte les octets de tas demandés pendant l’opération. Le relevé exclut les entrées préparées avant la mesure, la pile, l’arrondi interne de malloc, le journal et le reste du processus. Le pic sur 16 Kio décrit l’entrée exercée. Des dispositions TOML différentes peuvent allouer davantage. L’objet nu exclut le code apporté uniquement à l’édition de liens. Aucun ELF NovaOS du gestionnaire n’est livré.

**Écart avec 4A.** Le prototype garde 2 492 octets par paquet de cette série, métadonnées décodées incluses. À 64 paquets, les 159 488 octets représentent 19,47 fois les 8 192 octets prévus pour un index compact. Le pic du parseur sur le manifeste ordinaire représente déjà 1,89 fois les 16 Kio de lecture envisagés, et atteint 8,21 fois cette somme sur l’entrée complétée. La limite d’entrée de 16 Kio est appliquée avant décodage. Le prototype hors ligne dépasse le budget résident proposé pour NovaOS. Pour envisager la résidence, séparer les objets vérifiés d’un index compact et qualifier un décodage à allocation plafonnée, y compris l’échec d’allocation. Les temps de sélection ne mesurent ni découverte, ni activation, ni restitution du matériel.

## Reproduction

Depuis la racine du worktree sur le Mac de laboratoire, donner à chaque campagne un répertoire de construction privé. La recette utilise les dépendances du verrou Cargo hors ligne, Rust stable, les deux cibles Darwin et les deux cibles nues, Rosetta et `llvm-size`. Les backends cryptographiques logiciels sont sélectionnés pour la compilation nue. Le banc n’acquiert pas le verrou QEMU.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export CARGO_TARGET_DIR="$(mktemp -d "$NOVAOS_TMPDIR/catalogue-4b.XXXXXX")"
python3 -B novaOS/libs/catalogue-pilotes/qualifier.py
```

Les vecteurs versionnés suffisent aux essais. Le [générateur](vecteurs/generer.py) utilise Python et cryptography pour les reconstruire octet pour octet. Les trois clés de chaque profil sont publiques ou dérivées de graines fictives publiées à cet effet. La proposition d’autorité durable traite le retour conjoint d’un disque et d’une politique ancienne, que les seules signatures ne peuvent détecter. [Décision durable et prérequis](AUTORITE.md)
