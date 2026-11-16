# Matrice CUT 0.24.0

Chaque numéro désigne une ligne de [la capture différentielle](preuves/options.jsonl.gz), à partir de 1. Chaque ligne indique les arguments, la locale et, pour le différentiel métier, les octets fournis, stdout, stderr et le statut des deux outils. Le nom d’invocation des diagnostics est normalisé pour la comparaison initiale. Les octets originaux sont conservés. La [matrice machine par cible](preuves/matrice-par-cible.jsonl.gz) associe chaque ligne aux quatre profils. Pour lire un cas, par exemple le premier, exécuter `gzip -dc preuves/options.jsonl.gz | sed -n '1p'` dans le dossier de livraison.

| Fonction | Cas discriminants en locale C | Résultat Darwin | Résultat moteur QEMU, quatre profils |
|---|---|---|---|
| `-f`, `--fields`, valeur séparée, accolée et abréviation | 1–4, TSV avec vides et TAB | accord GNU | accord brut |
| `-d`, `--delimiter`, `-s`, `--only-delimited` | 5–8, ligne avec et sans séparateur | accord GNU | accord brut |
| `--complement`, `-n` avec champs | 9–10 | accord GNU | accord brut |
| `-O`, `--output-delimiter`, valeur vide et multioctet | 11–12 | accord GNU | accord brut |
| Positions désordonnées, doublons, chevauchement, hors ligne | 13–14 | accord GNU | accord brut |
| `--`, nom avec tiret, plusieurs fichiers, stdin répété, absent, répertoire, permutation | 15–20 | accord GNU | opérandes réels hors sonde, cas 20 rejoué |
| `-c`, `--characters`, `-b`, `--bytes`, `-n`, `--no-partial`, listes adjacentes, ouvertes ou préfixes | 21–30 | accord GNU | accord brut |
| `-w`, `--whitespace-delimited[=trimmed]`, `-F`, blancs extrêmes, sortie implicite ou explicite, conflits | 31–40 | accord GNU | accord brut |
| `-z`, `--zero-terminated`, délimiteurs NUL, LF, multioctets et invalides | 41–48 | accord GNU | accord brut |
| Erreur sans mode, option inconnue, abréviation ambiguë, modes incompatibles, zéro, plage inversée, dépassements | 49–70 | accord GNU, statut et diagnostic | diagnostic du parseur comparé, `--bad absent` hors sonde |
| Noms non ASCII, option complément sur tout le domaine et délimiteur LF | 71–75 | accord GNU | cas 74–75 rejoués |
| Fichier vide, caractère coupé aux frontières 65 536 et 262 144 | 76–86 | accord GNU | accord brut par tranches de 1 024 octets |
| Combinaisons générées | 87–986, graine 27027 | accord GNU | accord brut |
| Même corpus UTF-8 | ajouter 986 aux numéros ci-dessus | accord GNU | mêmes exclusions de fichiers |
| `--help`, `--version`, abréviations, français et repli anglais | 1973–1996 | sortie documentaire contrôlée, stdin intact | vrai catalogue `nsh`, essais détaillés ci-dessous |
| Sortie fermée | 1997–1998 | SIGPIPE pour les deux outils | production à qualifier |

La sonde sélectionne 1 954 vecteurs sur les 1 972 invocations métier. Les identifiants réellement réussis figurent dans `cas_reussis` du JSON SDK de chaque profil. Les messages d’erreur y sont comparés au diagnostic utile de GNU, après retrait du préfixe d’invocation et de l’invite finale d’aide. Le test couvre la décision du parseur avant lecture. Le statut d’erreur et le formatage intégral via l’adaptateur natif restent à contrôler par lancement de production. Les fichiers réels et la fermeture de sortie sont exercés sur Darwin. Les 933 essais amont sont consignés dans [le journal Nova](preuves/amont-nova.txt) et [le journal GNU](preuves/amont-gnu.txt). Les autres essais Darwin couvrent GB18030, le refus d’accès, POSIXLY_CORRECT et les grandes bornes de plages. [Essais complémentaires](preuves/limites-darwin.json)

| Vrai nsh | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
|---|---:|---:|---:|---:|
| `cut -f1,3,8,9 donnees.tsv` | 3 | 3 | 3 | 3 |
| `cat donnees.tsv \| cut -f1,3,8,9` | 3 | 3 | 3 | 3 |
| `cut --inconnue fichier-absent` | 3 | 3 | 3 | 3 |
| `cut donnees.tsv` | 127 | 127 | 127 | 127 |
| `cat donnees.tsv \| wc -l`, résultat 1 | 0 | 0 | 0 | 0 |
| `cut --help`, `cut --version`, `man cut` | 0 | 0 | 0 | 0 |

Pour la documentation, `fr-CA` sélectionne le français, `fr--CA` et `en-US` l’anglais. Chaque trace conserve la commande, les octets série et le statut. Le témoin SDK et le catalogue ordinaire sont deux images distinctes pour chaque profil. Les sorties des commandes métier de production demeurent à qualifier. [Traces, catalogues ELF et empreintes](preuves/qemu)
