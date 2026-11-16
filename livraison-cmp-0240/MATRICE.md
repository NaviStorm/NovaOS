# Matrice de conformité de cmp 0.24.0

Pour les deux fichiers de 262 144 octets, le premier octet de grand2 vaut 0x58 et tous les autres octets valent zéro. Avec `cmp -l grand1 grand2`, GNU 3.12 retourne **0**. Nova retourne **1** sur le Mac et sur chaque ELF de recette. La sortie complète vaut `20202020203120202030203133300a` en hexadécimal et stderr est vide. Le test attend le statut propre à chaque cible et échoue si un seul octet change.

Chaque cellule QEMU indique le statut **nsh ordinaire / processus cmp en recette**. Le tiret signale un cas non exécuté sur ce chemin. Pour les comparaisons de fichiers ordinaires, le témoin minimal du catalogue donne 127. Le SDK exerce le moteur avec entrées mémoire sur les deux ISA. Les deux sorties brutes, les entrées et la locale figurent dans [options.json](preuves/options.json). Les archives `statut-*` associent la mesure brute au PID, à la commande et au média.

| Cas | Arguments | GNU | Mach-O Nova | SDK 2 ISA | ARM 512 | ARM 4096 | x86 512 | x86 4096 |
|---|---|---:|---:|---|---|---|---|---|
| <code>amont--a-a</code> | <code>a a</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--a-b</code> | <code>a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--a-vide</code> | <code>a vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--a-absent</code> | <code>a absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--b-a</code> | <code>b a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--b-b</code> | <code>b b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--b-vide</code> | <code>b vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--b-absent</code> | <code>b absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--vide-a</code> | <code>vide a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--vide-b</code> | <code>vide b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--vide-vide</code> | <code>vide vide</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--vide-absent</code> | <code>vide absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--absent-a</code> | <code>absent a</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--absent-b</code> | <code>absent b</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--absent-vide</code> | <code>absent vide</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--absent-absent</code> | <code>absent absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-a-a</code> | <code>-l a a</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--l-a-b</code> | <code>-l a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--l-a-vide</code> | <code>-l a vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--l-a-absent</code> | <code>-l a absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-b-a</code> | <code>-l b a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--l-b-b</code> | <code>-l b b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--l-b-vide</code> | <code>-l b vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--l-b-absent</code> | <code>-l b absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-vide-a</code> | <code>-l vide a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--l-vide-b</code> | <code>-l vide b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--l-vide-vide</code> | <code>-l vide vide</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--l-vide-absent</code> | <code>-l vide absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-absent-a</code> | <code>-l absent a</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-absent-b</code> | <code>-l absent b</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-absent-vide</code> | <code>-l absent vide</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--l-absent-absent</code> | <code>-l absent absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-a-a</code> | <code>-s a a</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--s-a-b</code> | <code>-s a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--s-a-vide</code> | <code>-s a vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--s-a-absent</code> | <code>-s a absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-b-a</code> | <code>-s b a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--s-b-b</code> | <code>-s b b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--s-b-vide</code> | <code>-s b vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--s-b-absent</code> | <code>-s b absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-vide-a</code> | <code>-s vide a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--s-vide-b</code> | <code>-s vide b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>amont--s-vide-vide</code> | <code>-s vide vide</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>amont--s-vide-absent</code> | <code>-s vide absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-absent-a</code> | <code>-s absent a</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-absent-b</code> | <code>-s absent b</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-absent-vide</code> | <code>-s absent vide</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>amont--s-absent-absent</code> | <code>-s absent absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>option--b</code> | <code>-b a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--c</code> | <code>-c a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---print-bytes</code> | <code>--print-bytes a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---print-chars</code> | <code>--print-chars a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--l</code> | <code>-l a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---verbose</code> | <code>--verbose a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--bl</code> | <code>-bl a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--lb</code> | <code>-lb a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--cl</code> | <code>-cl a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--bc</code> | <code>-bc a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--s</code> | <code>-s a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---silent</code> | <code>--silent a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---quiet</code> | <code>--quiet a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option--bs</code> | <code>-bs a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---prinT</code> | <code>--prinT a b</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>option---verbo</code> | <code>--verbo a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---print-b</code> | <code>--print-b a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---print-c</code> | <code>--print-c a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---q</code> | <code>--q a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>option---s</code> | <code>--s a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-[]</code> | <code></code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-s']</code> | <code>-s</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--']</code> | <code>--</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--', '-l', 'a']</code> | <code>-- -l a</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['--', 'absent']</code> | <code>-- absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['a', '-l', 'b']</code> | <code>a -l b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['--v']</code> | <code>--v</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--ve']</code> | <code>--ve</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--ver']</code> | <code>--ver</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--pr']</code> | <code>--pr</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--p=x']</code> | <code>--p=x</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--oops', 'absent']</code> | <code>--oops absent</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-x']</code> | <code>-x</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-h']</code> | <code>-h</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-z']</code> | <code>-z</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-ls']</code> | <code>-ls</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-sl']</code> | <code>-sl</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--silent', '--verbose']</code> | <code>--silent --verbose</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-i']</code> | <code>-i</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--ig']</code> | <code>--ig</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['-n']</code> | <code>-n</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--by']</code> | <code>--by</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--sil=0']</code> | <code>--sil=0</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['--help=0']</code> | <code>--help=0</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['a', 'b', '0', '0', 'plus']</code> | <code>a b 0 0 plus</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>argv-['.', 'a']</code> | <code>. a</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>argv-['-s', '.', 'a']</code> | <code>-s . a</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>argv-['interdit', 'b']</code> | <code>interdit b</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>argv-['absent', 'absent']</code> | <code>absent absent</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-n0', 'absent', 'b']</code> | <code>-n0 absent b</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-s', 'absent', 'b']</code> | <code>-s absent b</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['a', 'copie']</code> | <code>a copie</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['a', 'lien']</code> | <code>a lien</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['a']</code> | <code>a</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-', 'a']</code> | <code>- a</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['a', '-']</code> | <code>a -</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-', '-']</code> | <code>- -</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-i1:2', '-', '-']</code> | <code>-i1:2 - -</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-i1:2', 'a', 'a']</code> | <code>-i1:2 a a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['-i', '99', 'a', 'vide']</code> | <code>-i 99 a vide</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['-i', '1:2', '-n0', 'a', 'b']</code> | <code>-i 1:2 -n0 a b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['-i', '1:2', 'a', 'b', '3', '1']</code> | <code>-i 1:2 a b 3 1</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['-i2:0', '-i1', 'a', 'b']</code> | <code>-i2:0 -i1 a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['a', 'b', '2', '3']</code> | <code>a b 2 3</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['--bytes=2', 'a', 'b']</code> | <code>--bytes=2 a b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['--ignore-initial', '2:3', 'a', 'b']</code> | <code>--ignore-initial 2:3 a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['-i1:0', '-', 'a']</code> | <code>-i1:0 - a</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-l', 'a', '-']</code> | <code>-l a -</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-i', '1000000000000000000000000', '-', 'b']</code> | <code>-i 1000000000000000000000000 - b</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>argv-['-n4', '-n2', 'a', 'b']</code> | <code>-n4 -n2 a b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['-n2', '-n4', 'a', 'b']</code> | <code>-n2 -n4 a b</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>argv-['-bl', 'lf', 'a']</code> | <code>-bl lf a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['partiel', 'a']</code> | <code>partiel a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['lf', 'a']</code> | <code>lf a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['vide', 'a']</code> | <code>vide a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['a', 'vide']</code> | <code>a vide</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['-s', 'lf', 'a']</code> | <code>-s lf a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>argv-['-l', 'lf', 'a']</code> | <code>-l lf a</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>stdin-regulier-['-', '-', '1', '2']</code> | <code>- - 1 2</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>stdin-regulier-['-', '-', '1', '0']</code> | <code>- - 1 0</code> | 0 | 0 | — | — / — | — / — | — / — | — / — |
| <code>stdin-regulier-['-n0', '-', '-', '1', '2']</code> | <code>-n0 - - 1 2</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>stdin-regulier-['-s', '-', '-', '0', '1']</code> | <code>-s - - 0 1</code> | 1 | 1 | — | — / — | — / — | — / — | — / — |
| <code>stdin-regulier-['-l', 'a', '-']</code> | <code>-l a -</code> | 0 | 0 | — | — / — | — / — | — / — | — / — |
| <code>diagnostic-['--=x']</code> | <code>--=x</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>diagnostic-['-n0', '.', 'b']</code> | <code>-n0 . b</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>diagnostic-['--bad', 'interdit']</code> | <code>--bad interdit</code> | 2 | 2 | — | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>diagnostic-['-n', "x'y"]</code> | <code>-n 'x'"'"'y'</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>diagnostic-['-i', 'x\\ny']</code> | <code>-i 'x\ny'</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>diagnostic-['--', 'abc@host']</code> | <code>-- abc@host</code> | 2 | 2 | OK | — / — | — / — | — / — | — / — |
| <code>diagnostic-['--', 'fichier avec espace']</code> | <code>-- 'fichier avec espace'</code> | 2 | 2 | — | — / — | — / — | — / — | — / — |
| <code>nombre-['-n', '0']</code> | <code>-n 0 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=0']</code> | <code>--bytes=0 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '0']</code> | <code>-i 0 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=0']</code> | <code>--ignore-initial=0 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '1']</code> | <code>-n 1 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=1']</code> | <code>--bytes=1 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '1']</code> | <code>-i 1 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=1']</code> | <code>--ignore-initial=1 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '2']</code> | <code>-n 2 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=2']</code> | <code>--bytes=2 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '2']</code> | <code>-i 2 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=2']</code> | <code>--ignore-initial=2 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '3']</code> | <code>-n 3 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=3']</code> | <code>--bytes=3 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '3']</code> | <code>-i 3 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=3']</code> | <code>--ignore-initial=3 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '010']</code> | <code>-n 010 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=010']</code> | <code>--bytes=010 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '010']</code> | <code>-i 010 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=010']</code> | <code>--ignore-initial=010 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '0x10']</code> | <code>-n 0x10 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=0x10']</code> | <code>--bytes=0x10 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '0x10']</code> | <code>-i 0x10 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=0x10']</code> | <code>--ignore-initial=0x10 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '0X02']</code> | <code>-n 0X02 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=0X02']</code> | <code>--bytes=0X02 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '0X02']</code> | <code>-i 0X02 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=0X02']</code> | <code>--ignore-initial=0X02 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '0b10']</code> | <code>-n 0b10 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=0b10']</code> | <code>--bytes=0b10 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '0b10']</code> | <code>-i 0b10 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=0b10']</code> | <code>--ignore-initial=0b10 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '08']</code> | <code>-n 08 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=08']</code> | <code>--bytes=08 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '08']</code> | <code>-i 08 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=08']</code> | <code>--ignore-initial=08 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '-1']</code> | <code>-n -1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=-1']</code> | <code>--bytes=-1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '-1']</code> | <code>-i -1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=-1']</code> | <code>--ignore-initial=-1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '-0']</code> | <code>-n -0 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=-0']</code> | <code>--bytes=-0 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '-0']</code> | <code>-i -0 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=-0']</code> | <code>--ignore-initial=-0 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '+3']</code> | <code>-n +3 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=+3']</code> | <code>--bytes=+3 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '+3']</code> | <code>-i +3 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=+3']</code> | <code>--ignore-initial=+3 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', ' 2']</code> | <code>-n ' 2' k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes= 2']</code> | <code>'--bytes= 2' k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', ' 2']</code> | <code>-i ' 2' k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial= 2']</code> | <code>'--ignore-initial= 2' k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', 'K']</code> | <code>-n K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=K']</code> | <code>--bytes=K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', 'K']</code> | <code>-i K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=K']</code> | <code>--ignore-initial=K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', 'k']</code> | <code>-n k k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=k']</code> | <code>--bytes=k k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', 'k']</code> | <code>-i k k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=k']</code> | <code>--ignore-initial=k k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', 'kB']</code> | <code>-n kB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=kB']</code> | <code>--bytes=kB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', 'kB']</code> | <code>-i kB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=kB']</code> | <code>--ignore-initial=kB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', 'KB']</code> | <code>-n KB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=KB']</code> | <code>--bytes=KB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', 'KB']</code> | <code>-i KB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=KB']</code> | <code>--ignore-initial=KB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', 'KD']</code> | <code>-n KD k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=KD']</code> | <code>--bytes=KD k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', 'KD']</code> | <code>-i KD k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=KD']</code> | <code>--ignore-initial=KD k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', 'KiB']</code> | <code>-n KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=KiB']</code> | <code>--bytes=KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', 'KiB']</code> | <code>-i KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=KiB']</code> | <code>--ignore-initial=KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '1K']</code> | <code>-n 1K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1K']</code> | <code>--bytes=1K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1K']</code> | <code>-i 1K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=1K']</code> | <code>--ignore-initial=1K k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '1KiB']</code> | <code>-n 1KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1KiB']</code> | <code>--bytes=1KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1KiB']</code> | <code>-i 1KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=1KiB']</code> | <code>--ignore-initial=1KiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '1kD']</code> | <code>-n 1kD k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=1kD']</code> | <code>--bytes=1kD k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '1kD']</code> | <code>-i 1kD k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=1kD']</code> | <code>--ignore-initial=1kD k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '1MB']</code> | <code>-n 1MB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1MB']</code> | <code>--bytes=1MB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1MB']</code> | <code>-i 1MB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1MB']</code> | <code>--ignore-initial=1MB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '1MiB']</code> | <code>-n 1MiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1MiB']</code> | <code>--bytes=1MiB k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1MiB']</code> | <code>-i 1MiB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1MiB']</code> | <code>--ignore-initial=1MiB k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '0G']</code> | <code>-n 0G k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--bytes=0G']</code> | <code>--bytes=0G k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-i', '0G']</code> | <code>-i 0G k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=0G']</code> | <code>--ignore-initial=0G k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '1T']</code> | <code>-n 1T k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1T']</code> | <code>--bytes=1T k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1T']</code> | <code>-i 1T k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1T']</code> | <code>--ignore-initial=1T k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '1P']</code> | <code>-n 1P k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1P']</code> | <code>--bytes=1P k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1P']</code> | <code>-i 1P k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1P']</code> | <code>--ignore-initial=1P k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '1E']</code> | <code>-n 1E k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1E']</code> | <code>--bytes=1E k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1E']</code> | <code>-i 1E k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1E']</code> | <code>--ignore-initial=1E k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '1Z']</code> | <code>-n 1Z k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1Z']</code> | <code>--bytes=1Z k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1Z']</code> | <code>-i 1Z k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1Z']</code> | <code>--ignore-initial=1Z k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '1Y']</code> | <code>-n 1Y k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=1Y']</code> | <code>--bytes=1Y k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '1Y']</code> | <code>-i 1Y k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=1Y']</code> | <code>--ignore-initial=1Y k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '1Q']</code> | <code>-n 1Q k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1Q']</code> | <code>--bytes=1Q k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1Q']</code> | <code>-i 1Q k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1Q']</code> | <code>--ignore-initial=1Q k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1R']</code> | <code>-n 1R k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1R']</code> | <code>--bytes=1R k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1R']</code> | <code>-i 1R k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1R']</code> | <code>--ignore-initial=1R k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1b']</code> | <code>-n 1b k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1b']</code> | <code>--bytes=1b k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1b']</code> | <code>-i 1b k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1b']</code> | <code>--ignore-initial=1b k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1kib']</code> | <code>-n 1kib k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1kib']</code> | <code>--bytes=1kib k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1kib']</code> | <code>-i 1kib k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1kib']</code> | <code>--ignore-initial=1kib k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1m']</code> | <code>-n 1m k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1m']</code> | <code>--bytes=1m k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1m']</code> | <code>-i 1m k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1m']</code> | <code>--ignore-initial=1m k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1 ']</code> | <code>-n '1 ' k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1 ']</code> | <code>'--bytes=1 ' k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1 ']</code> | <code>-i '1 ' k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1 ']</code> | <code>'--ignore-initial=1 ' k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '']</code> | <code>-n '' k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=']</code> | <code>--bytes= k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '']</code> | <code>-i '' k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=']</code> | <code>--ignore-initial= k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '0x']</code> | <code>-n 0x k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=0x']</code> | <code>--bytes=0x k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '0x']</code> | <code>-i 0x k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=0x']</code> | <code>--ignore-initial=0x k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1:0']</code> | <code>-n 1:0 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1:0']</code> | <code>--bytes=1:0 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1:0']</code> | <code>-i 1:0 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=1:0']</code> | <code>--ignore-initial=1:0 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', '0:1']</code> | <code>-n 0:1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=0:1']</code> | <code>--bytes=0:1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '0:1']</code> | <code>-i 0:1 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--ignore-initial=0:1']</code> | <code>--ignore-initial=0:1 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-n', ':2']</code> | <code>-n :2 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=:2']</code> | <code>--bytes=:2 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', ':2']</code> | <code>-i :2 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=:2']</code> | <code>--ignore-initial=:2 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '2:']</code> | <code>-n 2: k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=2:']</code> | <code>--bytes=2: k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '2:']</code> | <code>-i 2: k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=2:']</code> | <code>--ignore-initial=2: k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '1::1']</code> | <code>-n 1::1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--bytes=1::1']</code> | <code>--bytes=1::1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-i', '1::1']</code> | <code>-i 1::1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['--ignore-initial=1::1']</code> | <code>--ignore-initial=1::1 k1 k2</code> | 2 | 2 | OK | 2 / — | 2 / — | 2 / — | 2 / — |
| <code>nombre-['-n', '4294967297']</code> | <code>-n 4294967297 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=4294967297']</code> | <code>--bytes=4294967297 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '4294967297']</code> | <code>-i 4294967297 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=4294967297']</code> | <code>--ignore-initial=4294967297 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '9223372036854775807']</code> | <code>-n 9223372036854775807 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=9223372036854775807']</code> | <code>--bytes=9223372036854775807 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '9223372036854775807']</code> | <code>-i 9223372036854775807 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=9223372036854775807']</code> | <code>--ignore-initial=9223372036854775807 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '9223372036854775808']</code> | <code>-n 9223372036854775808 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=9223372036854775808']</code> | <code>--bytes=9223372036854775808 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '9223372036854775808']</code> | <code>-i 9223372036854775808 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=9223372036854775808']</code> | <code>--ignore-initial=9223372036854775808 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['-n', '99999999999999999999999999']</code> | <code>-n 99999999999999999999999999 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['--bytes=99999999999999999999999999']</code> | <code>--bytes=99999999999999999999999999 k1 k2</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>nombre-['-i', '99999999999999999999999999']</code> | <code>-i 99999999999999999999999999 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>nombre-['--ignore-initial=99999999999999999999999999']</code> | <code>--ignore-initial=99999999999999999999999999 k1 k2</code> | 0 | 0 | OK | — / 0 | — / 0 | — / 0 | — / 0 |
| <code>C-b</code> | <code>-b a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>C-bl</code> | <code>-bl a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>C-l</code> | <code>-l a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>C-s</code> | <code>-s a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>C-i2</code> | <code>-i2 a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>C-n3</code> | <code>-n3 a b</code> | 1 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>en_US.UTF-8-b</code> | <code>-b a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>en_US.UTF-8-bl</code> | <code>-bl a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>en_US.UTF-8-l</code> | <code>-l a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>en_US.UTF-8-s</code> | <code>-s a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>en_US.UTF-8-i2</code> | <code>-i2 a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>en_US.UTF-8-n3</code> | <code>-n3 a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>fr_FR.UTF-8-b</code> | <code>-b a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>fr_FR.UTF-8-bl</code> | <code>-bl a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>fr_FR.UTF-8-l</code> | <code>-l a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>fr_FR.UTF-8-s</code> | <code>-s a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>fr_FR.UTF-8-i2</code> | <code>-i2 a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>fr_FR.UTF-8-n3</code> | <code>-n3 a b</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>en--help</code> | <code>--help</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>en--he</code> | <code>--he</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>en-v</code> | <code>-v</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>en--version</code> | <code>--version</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>en--versi</code> | <code>--versi</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-CA--help</code> | <code>--help</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-CA--he</code> | <code>--he</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-CA-v</code> | <code>-v</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-CA--version</code> | <code>--version</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-CA--versi</code> | <code>--versi</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>FR-Latn-FR--help</code> | <code>--help</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>FR-Latn-FR--he</code> | <code>--he</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>FR-Latn-FR-v</code> | <code>-v</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>FR-Latn-FR--version</code> | <code>--version</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>FR-Latn-FR--versi</code> | <code>--versi</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr_XX--help</code> | <code>--help</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr_XX--he</code> | <code>--he</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr_XX-v</code> | <code>-v</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr_XX--version</code> | <code>--version</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr_XX--versi</code> | <code>--versi</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-zzzz-invalid--help</code> | <code>--help</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-zzzz-invalid--he</code> | <code>--he</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-zzzz-invalid-v</code> | <code>-v</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-zzzz-invalid--version</code> | <code>--version</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>fr-zzzz-invalid--versi</code> | <code>--versi</code> | 0 | 0 | OK | 0 / — | 0 / — | 0 / — | 0 / — |
| <code>graine-0</code> | <code>-b -i1 -n220 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-1</code> | <code>-l -i2 -n168 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-2</code> | <code>-bl -i3 -n685 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-3</code> | <code>-s -i1 -n209 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-4</code> | <code>-b -i1 -n893 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-5</code> | <code>-l -i0 -n827 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-6</code> | <code>-bl -i3 -n253 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-7</code> | <code>-s -i2 -n92 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-8</code> | <code>-b -i1 -n423 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-9</code> | <code>-l -i2 -n731 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-10</code> | <code>-bl -i3 -n939 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-11</code> | <code>-s -i1 -n156 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-12</code> | <code>-b -i1 -n65 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-13</code> | <code>-l -i1 -n179 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-14</code> | <code>-bl -i2 -n271 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-15</code> | <code>-s -i2 -n416 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-16</code> | <code>-b -i1 -n5 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-17</code> | <code>-l -i1 -n169 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-18</code> | <code>-bl -i2 -n311 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-19</code> | <code>-s -i0 -n664 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-20</code> | <code>-b -i1 -n22 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-21</code> | <code>-l -i0 -n336 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-22</code> | <code>-bl -i0 -n60 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-23</code> | <code>-s -i3 -n29 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-24</code> | <code>-b -i0 -n564 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-25</code> | <code>-l -i3 -n155 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-26</code> | <code>-bl -i3 -n173 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-27</code> | <code>-s -i2 -n143 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-28</code> | <code>-b -i3 -n684 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-29</code> | <code>-l -i1 -n33 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-30</code> | <code>-bl -i1 -n296 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-31</code> | <code>-s -i3 -n469 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-32</code> | <code>-b -i1 -n387 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-33</code> | <code>-l -i0 -n52 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-34</code> | <code>-bl -i0 -n184 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-35</code> | <code>-s -i1 -n8 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-36</code> | <code>-b -i3 -n182 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-37</code> | <code>-l -i2 -n1 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-38</code> | <code>-bl -i1 -n100 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-39</code> | <code>-s -i3 -n129 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-40</code> | <code>-b -i1 -n15 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-41</code> | <code>-l -i0 -n722 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-42</code> | <code>-bl -i3 -n307 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-43</code> | <code>-s -i0 -n1 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-44</code> | <code>-b -i3 -n138 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-45</code> | <code>-l -i0 -n226 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-46</code> | <code>-bl -i0 -n131 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-47</code> | <code>-s -i2 -n45 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-48</code> | <code>-b -i3 -n636 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-49</code> | <code>-l -i3 -n170 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-50</code> | <code>-bl -i0 -n308 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-51</code> | <code>-s -i0 -n554 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-52</code> | <code>-b -i2 -n141 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-53</code> | <code>-l -i2 -n22 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-54</code> | <code>-bl -i0 -n136 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-55</code> | <code>-s -i2 -n685 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-56</code> | <code>-b -i3 -n25 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-57</code> | <code>-l -i2 -n131 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-58</code> | <code>-bl -i1 -n184 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-59</code> | <code>-s -i0 -n990 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-60</code> | <code>-b -i1 -n677 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-61</code> | <code>-l -i0 -n521 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-62</code> | <code>-bl -i1 -n396 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-63</code> | <code>-s -i1 -n196 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-64</code> | <code>-b -i3 -n9 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-65</code> | <code>-l -i2 -n87 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-66</code> | <code>-bl -i0 -n652 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-67</code> | <code>-s -i1 -n189 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-68</code> | <code>-b -i2 -n374 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-69</code> | <code>-l -i1 -n104 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-70</code> | <code>-bl -i0 -n168 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-71</code> | <code>-s -i0 -n959 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-72</code> | <code>-b -i2 -n574 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-73</code> | <code>-l -i1 -n523 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-74</code> | <code>-bl -i2 -n75 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-75</code> | <code>-s -i0 -n60 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-76</code> | <code>-b -i2 -n497 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-77</code> | <code>-l -i0 -n6 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-78</code> | <code>-bl -i0 -n231 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-79</code> | <code>-s -i2 -n767 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-80</code> | <code>-b -i1 -n167 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-81</code> | <code>-l -i0 -n214 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-82</code> | <code>-bl -i0 -n706 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-83</code> | <code>-s -i2 -n195 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-84</code> | <code>-b -i1 -n209 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-85</code> | <code>-l -i2 -n233 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-86</code> | <code>-bl -i0 -n240 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-87</code> | <code>-s -i0 -n94 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-88</code> | <code>-b -i2 -n458 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-89</code> | <code>-l -i2 -n531 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-90</code> | <code>-bl -i3 -n50 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-91</code> | <code>-s -i1 -n118 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>graine-92</code> | <code>-b -i0 -n432 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-93</code> | <code>-l -i1 -n126 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-94</code> | <code>-bl -i0 -n212 g h</code> | 1 | 1 | OK | — / — | — / — | — / — | — / — |
| <code>graine-95</code> | <code>-s -i2 -n117 g h</code> | 0 | 0 | OK | — / — | — / — | — / — | — / — |
| <code>verbose-ecart-premier-bloc</code> | <code>-l grand1 grand2</code> | 0 | 1 | OK | — / 1 | — / 1 | — / 1 | — / 1 |
| <code>position-64-bits</code> | <code>-i4294967296 sparse1 sparse2</code> | 1 | 1 | — | — / — | — / — | — / — | — / — |

Le chemin ordinaire documente aussi `man cmp` et la stabilité du disque. Le pipe `cat a.bin | cmp -s b.bin -` y est refusé avant création des enfants. Les archives de recette établissent également le statut 1 de `cmp a.bin b.bin` sur deux fichiers explicitement délégués.
