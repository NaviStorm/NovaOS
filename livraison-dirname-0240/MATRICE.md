# Matrice de conformité dirname

L’oracle exécutable est GNU coreutils 9.12 sur Darwin ARM64. Le port analyse tous les mots avant la première écriture métier. Il conserve l’ordre des opérandes, la graphie des séparateurs intérieurs et les octets des noms. Son moteur emprunte le préfixe de l’opérande en mémoire. Il ne consulte aucun objet fichier.

| Forme ou propriété | Cas discriminant | Résultat |
|---|---|---|
| Absence de nom | Aucun mot, `--` seul ou `-z` seul | stdout vide, diagnostic et statut 1 |
| Chemins | Chaîne vide, nom simple, `.`, `..`, `-` | Un point par nom |
| Racine du Mac | `/`, `//`, `///`, `//a//` | `/` suivi du séparateur de sortie |
| Conservation interne | `///a///b`, `a/../b`, `/a//.//` | Respectivement `///a`, `a/..`, `/a` |
| Accès aux chemins | Chemin absent et parent privé de permissions | Même résultat lexical, statut 0 |
| Lot ordonné | `a/b c/d` | Octets `61 0a 63 0a` |
| Mode NUL | `-z a/b c/d` ou `--zero a/b c/d` | Octets `61 00 63 00` |
| Préfixes longs acceptés | `--z`, `--ze`, `--zer`, `--zero` | Même lot binaire |
| Répétitions et regroupement | `-zz --zero -z a/b` | Une seule terminaison NUL |
| Fin des options | `-- --zero --help --version` | Trois points suivis chacun de LF |
| Permutation | `a/b -z two/y` | Deux résultats terminés par NUL. Avec `POSIXLY_CORRECT`, `-z` après le premier nom est un opérande |
| Aide | `--h`, `--he`, `--hel`, `--help` | Aide et statut 0, y compris après opérande et `-zz` |
| Version | Tout préfixe non vide de `--version` à partir de `--v` | Version et statut 0 |
| Arrêt documentaire | `--version --help`, ordre inverse, option invalide avant/après | La première action ou erreur rencontrée décide de l’issue |
| Options sans valeur | `--z=x`, `--he=x`, `--ver=`, `-z=2` | Statut 1, aucune sortie métier |
| Options invalides | `-h`, `-v`, `-zzq`, `---`, `--=`, `--zerox` | Statut 1, diagnostic différencié selon la faute |
| Octets des chemins | Espace, LF, octets hors UTF-8, 20 lots pseudo-aléatoires reproductibles | Flux identiques octet par octet à GNU |
| Terminal | Pseudo-terminal brut, dix styles GNU, préfixes de style uniques et ambigus, variable invalide | La citation s’applique au texte interactif. En mode NUL ou redirigé, les octets des résultats sont conservés |
| Encodage de présentation | C, UTF-8, Latin-1, Shift-JIS | Les octets de continuation sont traités dans le caractère qui les contient |
| Erreurs d’écriture Darwin | Descripteur fermé, descripteur en lecture seule, pipe sans lecteur, SIGPIPE ignoré ou par défaut | Même statut ou signal, même diagnostic après normalisation du nom installé |
| Documentation ARC-026 | `fr`, `Fr-Latn-CA`, `fr-x-demo`, `fr--CA`, `frr`, `en-US`, `C` | Français pour une étiquette française admise, anglais en repli, version du paquet 0.24.0 |

**Lecture par cible.** Chacun des 233 identifiants possède un résultat Darwin et une colonne pour chacun des quatre profils de sonde dans la [matrice détaillée](preuves/matrice.json). Les argv et les deux flux sont encodés en hexadécimal. Les 187 vecteurs communs couvrent le traitement avec un adaptateur mémoire. Les autres cas exercent les interfaces hôte, dont le signal, le descripteur, les encodages historiques, le nom d’invocation et les diagnostics d’environnement du pseudo-terminal. Le chemin interdit est régénéré dans un dossier privé à chaque campagne. L’oracle exact utilisé pour construire les vecteurs invités est conservé dans l’archive. [Captures GNU et Nova](preuves/options.json)

**ELF livré et vrai shell.** Les lignes `elf-*` des [résultats QEMU](preuves/qemu.json) identifient l’image par son SHA-256 et chaque écriture refusée par sa longueur et son port. Les lignes `nsh-*` identifient les commandes documentaires réussies et les appels métier en attente d’admission. La comparaison GNU porte sur stdout, stderr et le statut. Pour le diagnostic global, seul le nom installé `gdirname` devient `dirname`. L’aide et la version suivent les assertions bilingues ARC-026. La suite officielle Perl est exécutée sans modification. [Trace de la suite amont](preuves/perl-gnu.txt)

**Références et licence.** Lecture de `src/dirname.c`, des options documentaires, du choix de citation dans `src/system.h`, du chapitre dirname du manuel, de `man/dirname.x`, de `tests/misc/dirname.pl` et de `tests/misc/tty-quoting.sh`. Les fonctions de longueur, de dernière composante, de citation et de fermeture de stdout ont été relues dans les objets Git de gnulib épinglés par coreutils. L’oracle Darwin ramène `//` à `/`. Le manuel GNU autorise aussi une racine double sur les systèmes qui la distinguent. [Source GNU 9.12](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/dirname.c) · [Empreintes des références](preuves/references.txt)

L’implémentation MIT de uutils a été examinée à `4735bbe40d764a523c5ece37d47465b32452d1a2`. Son composant dirname utilise `std::ffi`, clap, fluent et uucore. Le port emploie un moteur indépendant utilisable en `no_std`, le catalogue NovaOS pour les textes et libc sur Darwin pour les descripteurs et l’encodage actif. Les sources et les tests GNU ont servi de spécification et d’oracle externe. Le code livré relève de la licence MIT des composants. [Implémentation uutils examinée](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/dirname/src/dirname.rs) · [Licence des composants](../components/LICENSE)
