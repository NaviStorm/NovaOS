# Matrice des options, alias et combinaisons

Chaque ligne donne le statut de GNU et de Nova sur Darwin, puis l’observation dans le travailleur NovaOS sur les quatre géométries/ISA. La colonne effet contient le motif de stdout avant LF ou le diagnostic GNU en locale C. Pour un succès métier, le contrôle inclut l’existence, le type et le mode sur Darwin, ou le type et la réouverture dans la vue P9. Pour un échec métier de stockage, NovaOS contrôle le statut et la présence du diagnostic natif. La colonne scénario mentionne toute variation d’environnement ou fermeture de stdout. Les octets effectivement lus sur Darwin figurent dans [les sorties brutes](preuves/options.json).

| Cas | Arguments | Scénario | GNU / Nova Darwin | Effet ou diagnostic | ELF NovaOS |
|---:|---|---|---:|---|---|
| 1 | `(aucun)` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 2 | `-d` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 3 | `--directory` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 4 | `--dry-run` | C, TMPDIR=env | 0 / 0 | `sans création · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 5 | `-q` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 6 | `--quiet` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 7 | `-u` | C, TMPDIR=env | 0 / 0 | `sans création · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 8 | `-t` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 9 | `-dqu` | C, TMPDIR=env | 0 / 0 | `sans création · env/tmp\.[a-zA-Z0-9]{10}` | 4/4 |
| 10 | `-p p XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 11 | `-pp XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 12 | `--tmpdir=p XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 13 | `--tmpdir XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 14 | `--tmpdir= XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 15 | `-tp p XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 16 | `--tmpdir=p/// XYZ.XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p/XYZ\.[a-zA-Z0-9]{3}` | 4/4 |
| 17 | `-tp p XXX` | TMPDIR= | 0 / 0 | `fichier 0600 · p/[a-zA-Z0-9]{3}` | 4/4 |
| 18 | `--tmpdir=p sub/zXXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p/sub/z[a-zA-Z0-9]{3}` | 4/4 |
| 19 | `--suffix=X aXXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · a[a-zA-Z0-9]{3}X` | 4/4 |
| 20 | `aXXX --suff .ok` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · a[a-zA-Z0-9]{3}\.ok` | 4/4 |
| 21 | `--suffix= XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · [a-zA-Z0-9]{3}` | 4/4 |
| 22 | `aXXX-bXXXX.end` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · aXXX-b[a-zA-Z0-9]{4}\.end` | 4/4 |
| 23 | `-- -XXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · -[a-zA-Z0-9]{3}` | 4/4 |
| 24 | `--dry absent/aXXX` | C, TMPDIR=env | 0 / 0 | `sans création · absent/a[a-zA-Z0-9]{3}` | 4/4 |
| 25 | `-p` | C, TMPDIR=env | 1 / 1 | `mktemp: option requires an argument -- 'p'` | 4/4 |
| 26 | `--suffix` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--suffix' requires an argument` | 4/4 |
| 27 | `--non` | C, TMPDIR=env | 1 / 1 | `mktemp: unrecognized option '--non'` | 4/4 |
| 28 | `-h` | C, TMPDIR=env | 1 / 1 | `mktemp: invalid option -- 'h'` | 4/4 |
| 29 | `--d` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--d' is ambiguous; possibilities: '--directory' '--dry-run'` | 4/4 |
| 30 | `--=x` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--=x' is ambiguous; possibilities: '--directory' '--quiet' '--dry-run' '--suffix' '--tmpdir' '--help' '--version'` | 4/4 |
| 31 | `--directory=1` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--directory' doesn't allow an argument` | 4/4 |
| 32 | `--quiet=1` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--quiet' doesn't allow an argument` | 4/4 |
| 33 | `--dry-run=1` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--dry-run' doesn't allow an argument` | 4/4 |
| 34 | `--help=x` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--help' doesn't allow an argument` | 4/4 |
| 35 | `--version=x` | C, TMPDIR=env | 1 / 1 | `mktemp: option '--version' doesn't allow an argument` | 4/4 |
| 36 | `-- --help` | C, TMPDIR=env | 1 / 1 | `gmktemp: too few X's in template '--help'` | 4/4 |
| 37 | `XX` | C, TMPDIR=env | 1 / 1 | `gmktemp: too few X's in template 'XX'` | 4/4 |
| 38 | `''` | C, TMPDIR=env | 1 / 1 | `gmktemp: too few X's in template ''` | 4/4 |
| 39 | `-q a b` | C, TMPDIR=env | 1 / 1 | `gmktemp: too many templates` | 4/4 |
| 40 | `--suffix= XXX.txt` | C, TMPDIR=env | 1 / 1 | `gmktemp: with --suffix, template 'XXX.txt' must end in X` | 4/4 |
| 41 | `--suffix=/t XXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: invalid suffix '/t', contains directory separator` | 4/4 |
| 42 | `-t dir/XXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: invalid template, 'dir/XXX', contains directory separator` | 4/4 |
| 43 | `--tmpdir=p /XXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: invalid template, '/XXX'; with --tmpdir, it may not be absolute` | 4/4 |
| 44 | `aXXX/b` | C, TMPDIR=env | 1 / 1 | `gmktemp: invalid suffix '/b', contains directory separator` | 4/4 |
| 45 | `--tmpdir p XXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: too many templates` | 4/4 |
| 46 | `missing/aXXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: failed to create file via template 'missing/aXXX': No such file or directory` | 4/4 |
| 47 | `-d missing/aXXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: failed to create directory via template 'missing/aXXX': No such file or directory` | 4/4 |
| 48 | `-q missing/aXXX` | C, TMPDIR=env | 1 / 1 | `stderr vide` | 4/4 |
| 49 | `--quiet refus/fXXX` | C, TMPDIR=env | 1 / 1 | `stderr vide` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 50 | `refus/fXXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: failed to create file via template 'refus/fXXX': Permission denied` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 51 | `-u /dev/null/zXXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: failed to create file via template '/dev/null/zXXX': Not a directory` | 4/4 |
| 52 | `XXX --suffix=z` | POSIXLY_CORRECT=1 | 1 / 1 | `gmktemp: too many templates` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 53 | `--help` | LC_ALL=fr-CA | 0 / 0 | `Créer un fichier` | 4/4 |
| 54 | `--he` | LC_ALL=fr-CA | 0 / 0 | `Créer un fichier` | 4/4 |
| 55 | `--help` | LC_ALL=fr-Zzzz | 0 / 0 | `Créer un fichier` | 4/4 |
| 56 | `--he` | LC_ALL=fr-Zzzz | 0 / 0 | `Créer un fichier` | 4/4 |
| 57 | `--help` | LC_ALL=fr--CA | 0 / 0 | `Create a temporary` | 4/4 |
| 58 | `--he` | LC_ALL=fr--CA | 0 / 0 | `Create a temporary` | 4/4 |
| 59 | `--help` | LC_ALL=fr-zzzzz | 0 / 0 | `Create a temporary` | 4/4 |
| 60 | `--he` | LC_ALL=fr-zzzzz | 0 / 0 | `Create a temporary` | 4/4 |
| 61 | `--help` | LC_ALL=C | 0 / 0 | `Create a temporary` | 4/4 |
| 62 | `--he` | LC_ALL=C | 0 / 0 | `Create a temporary` | 4/4 |
| 63 | `--version` | C, TMPDIR=env | 0 / 0 | `mktemp (nova-mktemp) 0.24.0` | 4/4 |
| 64 | `-V` | C, TMPDIR=env | 0 / 0 | `mktemp (nova-mktemp) 0.24.0` | 4/4 |
| 65 | `-dqV` | C, TMPDIR=env | 0 / 0 | `mktemp (nova-mktemp) 0.24.0` | 4/4 |
| 66 | `--suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 67 | `-t --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 68 | `-u --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 69 | `-ut --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 70 | `-q --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 71 | `-qt --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `fichier 0600 · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 72 | `-qu --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 73 | `-qut --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 74 | `-d --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 75 | `-dt --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 76 | `-du --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 77 | `-dut --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 78 | `-dq --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 79 | `-dqt --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 80 | `-dqu --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 81 | `-dqut --suffix=.txt pXXX` | C, TMPDIR=env | 0 / 0 | `sans création · env/p[a-zA-Z0-9]{3}\.txt` | 4/4 |
| 82 | `(aucun)` | stdout fermé | 1 / 1 | `gmktemp: write error: Bad file descriptor` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 83 | `-d` | stdout fermé | 1 / 1 | `gmktemp: write error: Bad file descriptor` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 84 | `-q` | stdout fermé | 1 / 1 | `stderr vide` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 85 | `-dq` | stdout fermé | 1 / 1 | `stderr vide` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 86 | `-u` | stdout fermé | 1 / 1 | `gmktemp: write error: Bad file descriptor` | descripteur POSIX, environnement POSIX ou chmod Darwin |
| 87 | `'--suffix=\xff' XXX` | C, TMPDIR=env | 1 / 1 | `gmktemp: failed to create file via template 'XXX\377': Illegal byte sequence` | argv non UTF-8 |
| 88 | `--dir XXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · [a-zA-Z0-9]{3}` | 4/4 |
| 89 | `--dire XXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · [a-zA-Z0-9]{3}` | 4/4 |
| 90 | `--director XXX` | C, TMPDIR=env | 0 / 0 | `dossier 0700 · [a-zA-Z0-9]{3}` | 4/4 |
