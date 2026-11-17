# NovaOS ARC-027 · date

| Cible | Exécutable | Version annoncée | SHA-256 |
|---|---|---|---|
| Darwin ARM64 | [bin/date](bin/date) | nova-cli 0.20.0 | `00622a28d5596cdb3fbad1b92af119979fac010564250caa789466b0b19114ef` |
| NovaOS AArch64 | [bin/date-aarch64.elf](bin/date-aarch64.elf) | nova-cli 0.20.0 | `fa05951ce12b494e3fd4b57b531b5e31d3f9805711dd4a45dd751e48ec5ca833` |
| NovaOS x86_64 | [bin/date-x86_64.elf](bin/date-x86_64.elf) | nova-cli 0.20.0 | `4ee4e49d19bee31ea31a79a96ba73d5143063225963f58d40ca16ff5cbdc9a53` |

La branche `codex/date-natif-0240` part de `origin/main` vérifié à `425e4e7cc5c03e246efb64d3638376906344bf47`. Le commit d’implémentation est `07c26c00dd31cd56ea6d64749bc412671eb8eb1c`. Le correctif de locale et ses tests sont dans `d545ecb4cca70aaf8fa552807981986313e9fa0b`. Les premiers bancs sont dans `1074209074cae98d757547297c01ec127f458aea`. Les exécutables visent l’ABI native v1 à 48 sélecteurs. Le numéro du chantier ARC-027 et la version des binaires sont indépendants.

## Résultats

- **Darwin ARM64** — 718 contrôles en mode complet et 192 en mode rapide. Les flux bruts, le statut et les effets du double de réglage sont conservés. Les deux suites Perl de GNU ont passé leurs 1 079 invocations annoncées. Huit scripts shell supplémentaires ont réussi. Deux ont rendu 77. Darwin ne fournit pas `locale date_fmt` et la locale thaïe UTF-8 demandée par le script est absente. [Suite amont](preuves/amont/statuts.txt)
- **ELF NovaOS** — 603 scénarios par couple ISA/secteur dans le catalogue SDK. Chaque enfant exécute l’ELF de livraison, attesté dans le noyau de recette. Les observations isolent stdout, stderr, statut et PID. Le volume de ce profil est en lecture seule et son empreinte est contrôlée. La RTC est comparée à l’intervalle réel du démarrage, avec une marge d’une seconde de granularité. [Matrice et limites](MATRICE.md)
- **Session ordinaire** — 19 contrôles par profil, dont 15 consultations de documentation, trois appels externes qui rendent 127 et un pipe qui rend 3. Les lectures de documentation conservent l’empreinte du disque et les compteurs QMP d’écritures et de flush. La commande n’est pas installable dans cette session sur la base du mandat. La branche du lanceur `f8966d38fa5ef021e4b2354780e0a0c4dd69f946` qualifie les deux contrats SPAWN et indique que le raccord de la session réelle de nsh à NovaAuth reste ouvert. [Décision du lanceur](https://github.com/NaviStorm/NovaOS/blob/f8966d38fa5ef021e4b2354780e0a0c4dd69f946/novaOS/docs/lancement-generique-tickets-tmp.md)
- **Régression du système** — 11 suites sur AArch64 et 11 sur x86_64, toutes réussies. Les tests ciblés du composant ajoutent trois tests unitaires, un exemple compilé et cinq tests du catalogue documentaire.
- **Lots, flux et mesures** — voir les mesures par charge, les échantillons et la définition du pic mémoire. Dans chacun des quatre profils, les deux lots, le pipe, le mtime absent, le mtime du fichier créé et le mtime exact `978307200.800000001` ont réussi. Le lot de recette remplace les exécutables de deux emplacements historiques dans une copie jetable, réutilise les droits de lecture et les extrémités de pipe existantes et vérifie les octets. [Performances](PERFORMANCES.md) · [Régressions système](preuves/regressions/statuts.txt)

## Correctifs vérifiés

Sur Darwin, le champ de fuseau de la structure transmise à strftime était vide. Le format local persan `%c` pouvait afficher CET pour une date demandée en UTC. Le transfert de l’abréviation et de l’indicateur d’heure d’été rend désormais les mêmes octets que GNU.

La première sonde de lot x86_64 calculait la fréquence TSC à l’intérieur de la section critique de l’ordonnanceur. La calibration attend des interruptions PIT. La trace s’arrête après les 32 dates valides et le banc expire après 180 secondes. Le correctif de recette `680fffe` capture les compteurs dans la section critique et calibre une seule fois après sa sortie. [Trace du premier passage](preuves/incident-calibration-x86.serial.gz)

## Références et provenance

L’oracle est `/opt/homebrew/opt/coreutils/libexec/gnubin/date`, Coreutils 9.12, bouteille Homebrew 9.12_1, SHA-256 `9287253f3a643f42d40966e37c30b2cf5cb33f947f56ee5efd6d053db4a8a54f`. Les deux exécutables reçoivent le même argv[0], `date`, ce qui permet de comparer les diagnostics octet par octet. L’aide documentaire suit ARC-026.

- GNU Coreutils v9.12, commit `c0f8514d989184921d9b12a4d103a7b23abc5af8`, GPL-3.0-or-later. Consultation de `src/date.c`, `doc/coreutils.texi`, `tests/date/` et de l’aide de la bouteille. Les scripts amont sont exécutés depuis un checkout externe. [Source officielle](https://git.savannah.gnu.org/cgit/coreutils.git/tree/src/date.c?h=v9.12)
- Gnulib `106e9b2384d08a1696fcbd40cbab52237943f208`. Consultation de `lib/parse-datetime.y`, `lib/time_rz.c`, `lib/nstrftime.c` et de l’implémentation de formatage incluse dans `lib/strftime.c`. [Sources Gnulib](https://git.savannah.gnu.org/cgit/gnulib.git/tree/lib?id=106e9b2384d08a1696fcbd40cbab52237943f208)
- uutils Coreutils `4735bbe40d764a523c5ece37d47465b32452d1a2`, MIT, candidat étudié dans `src/uu/date/`. Le noyau de calcul Nova adapte `parse_datetime` 0.17.0, source MIT `eb444cc124d55e10f9e11bb3a043f3fb11c13b69`, vers `no_std + alloc`, et ajoute la trace de consommation, les règles de fraction et l’adaptateur de plateforme. [uutils date](https://github.com/uutils/coreutils/tree/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/date) · [Analyseur](https://github.com/uutils/parse_datetime/tree/eb444cc124d55e10f9e11bb3a043f3fb11c13b69)
- Le calcul civil emploie Jiff 0.2.45, la base IANA embarquée Jiff TZDB 0.1.10 sur cible nue et les routines de calendrier Darwin sur macOS. Le verrou Cargo, les notices des paquets résolus et les mentions de la bibliothèque Rust accompagnent les binaires. Les sources GNU servent d’oracle externe. [Dépendances et licences](licences/dependances.tsv) · [Notices](licences/NOTICES.txt)

## Rejeu depuis la racine du worktree

L’environnement de qualification était macOS 27.2.1 (26B131) ARM64, Rust stable 1.99.1, LLVM 23.1.1 et QEMU 11.1.3. Les scripts de construction utilisent Rust stable, les trois cibles annoncées, un cache Cargo préparé, le SDK système Darwin, les outils QEMU/UEFI/Limine du dépôt, xorriso, Python 3, Perl et les Coreutils Homebrew. Sur un cache neuf, préparer les dépendances verrouillées avec `rustup run stable cargo fetch --locked --manifest-path components/coreutils/date/Cargo.toml`. Conserver une copie des sources épinglées pour le rejeu hors ligne. Le banc amont attend le checkout de Coreutils et un dépôt Git de son sous-module gnulib avec la révision citée ci-dessus.

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$NOVAOS_TMPDIR/date-rejeu"
d=./livraison-date-0240
for cible in macOS aarch64 x86_64; do "$d/reconstruire.sh" "$cible"; done
"$d/tester-options-date.sh" --rapide --out "$NOVAOS_TMPDIR/date-rejeu/rapide.jsonl"
"$d/tester-options-date.sh" --complet --out "$NOVAOS_TMPDIR/date-rejeu/options.jsonl"
"$d/tester-options-date.sh" --amont "$NOVAOS_TMPDIR/sort-arc027-gnu" "$NOVAOS_TMPDIR/date-rejeu/amont"
"$d/mesurer-performances-date.sh" --rapide --out "$NOVAOS_TMPDIR/date-rejeu/perf-rapide.json"
"$d/mesurer-performances-date.sh" --complet --out "$NOVAOS_TMPDIR/date-rejeu/perf-complete.json"
for arch in aarch64 x86_64; do
    "$d/tester-options-date.sh" --qemu "$arch" sdk --vecteurs "$NOVAOS_TMPDIR/date-rejeu/options.jsonl" --out "$NOVAOS_TMPDIR/date-rejeu"
    "$d/tester-options-date.sh" --qemu "$arch" nsh --out "$NOVAOS_TMPDIR/date-rejeu"
    "$d/mesurer-performances-date.sh" --qemu "$arch" "$NOVAOS_TMPDIR/date-rejeu"
done
sh "$d/lab/regressions.sh" "$NOVAOS_TMPDIR/date-rejeu/regressions"
LC_ALL=fr-CA "$d/manuel-date.sh"
python3 -B "$d/lab/archiver.py" --verifier
```

Les tests de réglage doivent passer par `tester-options-date.sh`. Le banc refuse le mode privilégié et vérifie l’interposition de l’horloge pour chaque binaire avant les essais d’écriture. Les appels sont enregistrés dans un fichier privé. Pour le contrôle de l’heure réelle de Darwin, chaque lecture de la commande est encadrée par deux observations indépendantes. Sur NovaOS, le service de consultation reste en lecture seule.

Les images de preuve `media-*.tar.xz` contiennent le disque de départ, le disque relu après le scénario et l’ISO exécutée. Le SHA-256 du noyau extrait de l’ISO est comparé à celui du noyau construit. Les tables et traces compressées sont lisibles depuis [preuves](preuves/). L’[archive](date-arc027.tar.gz) et son [empreinte](date-arc027.tar.gz.sha256) permettent de contrôler l’ensemble après extraction.
