# Créneau CUT sur Darwin ARM64 — 17 novembre 2026

Le banc vérifie séparément chaque médiane Nova ≤ 1,10 × GNU Coreutils 9.12, avec les valeurs non arrondies. Le [bilan des deux séries](confirmation.txt) donne, pour chaque charge, les trois temps, le rapport candidat/GNU et le verdict commun. Les quatre fichiers de dispersion donnent la médiane et les extrêmes de chaque outil, ainsi que le rapport publié/GNU. [TSV série 1](dispersion-r3-tsv-1.txt) · [TSV série 2](dispersion-r3-tsv-2.txt) · [Lignes longues série 1](dispersion-r3-longues-1.txt) · [Lignes longues série 2](dispersion-r3-longues-2.txt)

**Résultat Darwin.** Les 33 charges passent dans chacune des deux séries indépendantes. Le rapport maximal sur les 66 comparaisons est 1,072922137041015 (1 million de lignes, `-f1,3` sur fichier, série 1). La série 2 culmine à 1,0656665322456995. À 65 millions de lignes, la projection `-f1,3,8,9` sur fichier prend 3,836505 s pour le candidat et 6,982947 s pour GNU en série 2. Sur stdin, les temps correspondants sont 3,895300 et 6,913619 s.

## Exécutables et contrôles

| Exécutable | SHA-256 |
|---|---|
| Nova 0.24.0 publié à `008d62b82d5d7c7ffb601bb15a0bdee09af1b6c1` | `5e88e6f2522096e9d2ef2e894effbf0265a250091299ad1443da2f799a588e94` |
| Candidat initial à `f1767efaa0dd38bcd90405e0178cf3a5dce9a058` | `64397fc6446ae26a8c0e7bbb594f114776f80d2d248dbfcdfddb0e761bfb5d9d` |
| Candidat intermédiaire (r2) | `88834e0e7ad3b312499e04269f5630e5fdb2d255f16763e06d1770788cfadd8b` |
| Candidat final (r3) | `54796e3f5244ec30c6a82ba0b4a498f27d758c066d2930c835ca48c9494e8923` |
| GNU 9.12 (`/opt/homebrew/bin/gcut`) | `d84adc05e56f93e57fabbfdd555d443d3d5475947d3374ac0a68f461c7ef8335` |

Le commit local des sources finales est `f2a0485388679d44e53b19f4455b96aa3a516135` sur `codex/cut-bancs-v0300`.

Le premier candidat atteignait 1,2614 × GNU sur 64 Mio sans TAB avec `-f1,3`. Après recherche des séparateurs, le moteur recopiait chaque bloc sélectionné vers un tampon de sortie. L’adaptateur Darwin transmet désormais directement les blocs restitués à l’identique et lit les champs par blocs de 256 Kio. À 1 Mio, le candidat intermédiaire dépassait encore le seuil dans la première série (1,1218). Sur stdout redirigé, la couche de sortie standard de Rust cherchait encore les LF. Le candidat final écrit avec tampon sur un descripteur dupliqué, conserve le comportement de la console et évite le chargement de locale quand LC_ALL impose C ou POSIX. Les séries de diagnostic sont conservées sous `initial-*` et `r2-*`.

Le différentiel final compare stdout et stderr en octets, le statut et les cas d’entrée non consommée. Pour les deux lignes TSV avec de vraies tabulations, `-f1,3` rend `41 09 43 0a 44 09 46 0a`, stderr vide, statut 0 sur fichier et pipe Darwin. Les cas ajoutés traversent les frontières de lecture de 256 Kio avec et sans séparateur, y compris EOF et NUL. La suite différentielle complète a réussi 2 023 cas et la suite GNU amont 933. [Journal fonctionnel](options-r3.txt) · [Octets et statuts par cas](options-r3.jsonl.gz) · [Suite GNU](amont-r3.txt)

## Mesures

- Machine Mac16,5, Apple M4 Max, 16 processeurs logiques, 128 Gio de RAM, sur secteur. Cache fichiers chaud après un échauffement par outil et par charge, destination commune `/dev/null`. Le temps inclut lancement et sortie du processus. Les entrées stdin de performance sont des redirections du même fichier régulier. [Conditions et versions relevées](conditions-darwin.txt)
- Projections `-f1,3` et `-f1,3,8,9` sur fichier et stdin. TSV SORT/UNIQ en lecture seule, graine 27027, neuf colonnes. Les trois paliers comptent 23 175 529, 231 755 463 et 1 506 410 411 octets. Le complément de champs et la sélection de caractères sur UTF-8 sont aussi contrôlés sur fichier.
- Chaque ligne longue contient 1, 16 ou 64 Mio de `x`. Le suffixe est TAB + `fin` + LF pour le séparateur tardif, ou LF seul pour la ligne sans TAB. Les sorties de `-sf1` et `-sf2` sont toutes deux vérifiées. Sans TAB, `-f1,3` doit aussi restituer toute la ligne. Le contrôle de chaque sortie s’effectue avant chronométrage, par SHA-256 contre la projection construite depuis le générateur, statut 0 et stderr vide.
- Les processus des trois outils passent à tour de rôle. Chaque charge compte neuf passages par outil sous 100 000 000 octets et trois au-delà. La série 2 utilise une nouvelle invocation du banc, son propre échauffement et un ordre initial décalé. Les JSON conservent ordre, durée, statut, pic RSS, SHA d’entrée et de sortie, médiane et débit. Les ratios des deux séries sont jugés séparément. [TSV 1](r3-tsv-1.json) · [TSV 2](r3-tsv-2.json) · [Lignes longues 1](r3-longues-1.json) · [Lignes longues 2](r3-longues-2.json)

### Rejouer depuis livraison-cut-0240

```sh
python3 -B preuves/creneau/confirmer.py
export CUT_NOVA_BIN="$PWD/preuves/creneau/cut-darwin-r3"
./tester-options-cut.sh --novaOS --large
./tester-amont-cut.sh --novaOS
d=$(mktemp -d ../../tmp/cut-creneau.XXXXXXXX)
for famille in longues tsv; do
  for serie in 1 2; do
    ./mesurer-performances-cut.sh --large --publie --famille "$famille" --serie "$serie" --json "$d/$famille-$serie.json" || exit 1
  done
done
```

## Fenêtre NovaOS à réserver

La livraison historique a validé 1 954 vecteurs moteur par profil SDK. Dans les quatre profils historiques du vrai nsh, les deux invocations métier ont renvoyé 3 avant le lancement. Pour le candidat, le prévol vérifie la compilation statique des ELF CUT et de la sonde pour les deux ISA. La sonde ajoute trois contrôles de champs longs sur tranches de 1 Kio, en plus des 1 954 vecteurs. Le passage dans les quatre invités demande une fenêtre dédiée. [Prévol des deux ISA](prevol-qemu.json) · [Quatre ELF du prévol relus](prevol-elfs.tar.gz)

Depuis la racine du worktree, pour cette future fenêtre uniquement, construire la commande hôte de préparation des P9, puis exécuter la sonde successivement sur AArch64/512, AArch64/4096, x86_64/512 et x86_64/4096. Conserver les journaux série, les JSON et les SHA du noyau extrait de chaque ISO, des sondes et des vecteurs.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
(cd novaFS && CARGO_TARGET_DIR="$PWD/target" rustup run stable cargo build --release --locked --offline --target aarch64-apple-darwin --bin novafs)
rejeu=$(mktemp -d "$NOVAOS_TMPDIR/cut-sdk-creneau.XXXXXXXX")
for arch in aarch64 x86_64; do
  for secteur in 512 4096; do
    python3 -B livraison-cut-0240/lab/sonde.py "$arch" "$secteur" "$rejeu/$arch-$secteur" --large || exit 1
  done
done
```

Le lancement installé par nom ou chemin, l’ouverture autorisée de fichier et le transport de stdout brut via le pipe du vrai nsh relèvent du chantier du lanceur commun.
