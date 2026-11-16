# Matrice GNU tr 9.12

La clef d’un cas est son champ `id` dans le JSONL. Les enregistrements 1 à 1 826 utilisent C. Les suivants rejouent les mêmes arguments et entrées en en_US.UTF-8. Chaque ligne conserve les arguments, la taille de lecture, stdin, stdout, stderr et le statut des deux outils. Les flux sont en hexadécimal. Les arguments qui contiennent des octets invalides se reconstituent avec `os.fsencode` après lecture du JSON. Le générateur emploie la graine 270240. [Comparaisons individuelles](preuves/differentiel.jsonl.gz) · [Générateur](outils/options.py) · [Résultat de chaque cas par cible](preuves/matrice-par-cible.jsonl.gz)

| Forme ou comportement | Cas discriminants dans le JSONL |
|---|---|
| `-c`, `-C`, `--complement` | 1, 2, 3. Les octets hors de a–d sont traduits. |
| `-d`, `--delete` | 4, 5. Les octets de a–d disparaissent. |
| `-s`, `--squeeze-repeats` | 6, 7. Comparaison du flux répété. |
| `-t`, `--truncate-set1` | 8, 9. Un seul octet de destination pour quatre octets source. |
| `-A` | 10 sur le profil C et 2 011 pour la conversion en locale UTF-8 forcée à C. |
| Options groupées et arité | 11 à 74. Les 16 combinaisons de c/d/s/t reçoivent zéro, un, deux ou trois opérandes. |
| Préfixes, valeurs accolées et séparateur | À partir de 75, tous les préfixes des six noms longs et des valeurs interdites avec `=`. Cas 139 pour `-- -a XY`, 147 pour le préfixe ambigu vide. |
| Suppression puis compression | 148 et 251. Un octet supprimé rend deux a adjacents. |
| Traduction puis compression | 150. La traduction de deux octets distincts produit deux x adjacents. |
| Ensembles | Plages, 12 classes, équivalence, répétitions finies/indéfinies, classes de casse, octaux, échappements, ambiguïtés et rejets. Les arguments exacts se recherchent dans le JSONL. |
| Documentation | 199 et 200, abréviations à partir de 75. Comparaison séparée de l’identité Nova, de l’option affichée, de stderr et du succès. |
| Flux bruts et frontières | 256 valeurs d’octet, NUL, UTF-8 valide ou invalide, flux vide et sans LF. Lectures de 1, 2, 3, 7, 127, 128, 129, 4 096 et 65 536 octets. |
| Option inconnue non ASCII | 1 823 à 1 826 en C et 3 649 à 3 652 en UTF-8. stderr contient les octets d’origine pour une option courte, longue ou ambiguë. |
| Entrée intacte, lecture en erreur et sortie fermée | Décalage du descripteur avant et après aide, version et rejets. EISDIR réel et SIGPIPE sur le Mac. [Observations](preuves/flux.json) |

GNU sur Darwin emploie parfois `gtr:` pour un diagnostic même quand argv[0] vaut `tr`. Le comparateur conserve les deux stderr bruts et neutralise ce seul préfixe par ligne. Pour l’aide et la version, le texte original de GNU et la sortie du projet sont conservés séparément. Les deux programmes utilisent leur propre documentation.

Dans chacun des quatre invités, le témoin SDK reprend la totalité des enregistrements dans l’ordre. Il compare dans le processus utilisateur les octets, le mode documentaire ou le statut et les diagnostics du moteur. Une différence interrompt la série en indiquant l’identifiant du premier écart. Les tests de l’adaptateur vérifient aussi trame entièrement supprimée, EOF, lecture révoquée et propagation d’une erreur à la fermeture. Le noyau ordinaire exécute séparément l’aide, la version et le manuel depuis nsh. Les résultats de production des opérations métier restent ouverts pour tous les cas, car le lanceur refuse le pipeline.
