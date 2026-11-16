# Qualification ARC-027 · 16 novembre 2026

## Sources et profils

La branche `codex/sed-natif-0240` part du SHA distant de `main` vérifié avant modification, `816843fa6f5a8ae35f64ef0c3f96a67652bbc384`. Le checkout principal se trouvait à `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e` avec une modification de `.gitignore` et deux entrées `linux*` non suivies. Le travail s’est déroulé dans un worktree dédié.

| Révision du composant | Contenu |
|---|---|
| `fed57f326cad435efef1e778b16a9b2812483cf6` | Moteur adapté et banc Darwin |
| `2d1af481c7fec64380cb079c56b6929ae57fdca8` | ELF et documentation ARC-026 |
| `48b6493fcffe31535769306e69c3c993cd9fa725` | Métadonnées, erreurs de vidange, adresses multiligne et vecteurs de validation |
| `e8a85017f5c069b61b9958e7ec78a795519b2578` | Sources de production livrées. Ports lecteur et producteur distincts et fermeture explicite |

**Darwin ARM64 natif.** Oracle `/opt/homebrew/bin/gsed`, GNU sed 4.10. Chaque exécutable a son propre répertoire de fichiers fictifs recréé à l’identique. Le banc compare les octets de stdout et stderr, les statuts, l’inventaire final, le contenu des fichiers, le mode, le propriétaire, le groupe et les cibles des liens. Il exige la première ligne de version GNU 4.10. Les données binaires sont encodées en hexadécimal et les longues sorties comparées par taille et SHA-256. [Oracle](results/gnu-version.txt) · [Banc](differential.py)

| Locale | Cas nommés identiques / 111 | Fuzz identiques / 100 | Identiques sur les quatre critères | Dont écarts de stderr seul parmi les autres |
|---|---:|---:|---:|---:|
| C | 93 | 35 | 128/211 | 76 |
| en_US.UTF-8 | 91 | 34 | 125/211 | 76 |

Les cinq témoins obligatoires donnent les résultats de GNU dans les deux locales. Les captures donnent `1a\n2b\n`, le cycle avec N donne `un + deux\n`, la substitution sans correspondance donne `abc\n` et 0, `q42` imprime sa première ligne et renvoie 42. Les deux `-e` avec `--` impriment la première et la dernière ligne du fichier. Un script invalide renvoie 1, un fichier absent 2 et une erreur d’écriture 4. Les écarts connus incluent les diagnostics, les ancres TRE, les regex multioctets et la trace `--debug`. Le tableau de compatibilité indique aussi les fonctions sans preuve. [Matrice et scripts](matrice.md)

**Effets et limites de publication sur Darwin.** Une édition prépare ses données dans le répertoire cible. Le descripteur d’origine est ouvert en lecture/écriture, le candidat reçoit les permissions, ACL et attributs étendus puis est synchronisé. En présence d’un suffixe, un lien préserve l’entrée ancienne, puis le candidat est renommé sur la destination. Les essais comparent l’état relu après chaque échec.

| Essai sur fichier jetable | Résultat observé |
|---|---|
| Mode 0444, ou ACL explicite qui interdit l’écriture | Statut 4, original intact, aucune sauvegarde créée. GNU accepte le cas 0444 dans ce répertoire. |
| Mode 0640, ACL de refus d’exécution, xattr, suffixe | Résultat modifié, droits et xattr conservés, ancienne donnée exacte dans la sauvegarde. |
| Volume HFS+ de 16 Mio rempli | ENOSPC, statut 4, même SHA-256 de l’original, aucun résultat ni sauvegarde publié. |
| EIO injectée sur un binaire d’essai avant synchronisation | Statut 4, original intact et aucune sauvegarde. |
| SIGKILL après publication de la sauvegarde, avant renommage | Original et sauvegarde contiennent les anciens octets. Le candidat temporaire contient les nouveaux. |
| Coupure physique du stockage ou modification concurrente entre le dernier contrôle et le renommage | Hors modèle exercé. Pas de garantie de durabilité après panne matérielle ni de transaction atomique couvrant les deux noms. |

L’aide localisée de Darwin et ses statuts sont archivés. Les pages Darwin peuvent se lire par `man -M components/sed/man sed` ou `man -M components/sed/man/fr sed` depuis la racine. Les tests `e` utilisent des commandes fixes. Les 100 constructions générées ont toutes `--sandbox`. Les cas de rejet tardif contrôlent le contenu et l’absence de création ou de troncature avant exécution. [Mesures et pannes](results/resources.json) · [Langues](results/lang-darwin.json)

## Mémoire et temps sur Darwin

Une exécution par charge, via `/usr/bin/time -l` sur macOS 27.2.1 ARM64. Le pic est le maximum résident du processus sed, en octets. Une empreinte indépendante vérifie la sortie de chaque exécution. Les temps réels individuels ne constituent pas une estimation de débit garanti.

| Charge | Entrée en Mio | RSS maximal en octets | Temps réel en secondes |
|---|---|---|---|
| Lignes de 64 octets, `s/^a/Z/` | 1 / 8 / 32 | 2 244 608 / 2 244 608 / 2 244 608 | 0,01 / 0,07 / 0,67 |
| Une ligne, `s/^a/Z/` | 1 / 8 | 5 488 640 / 20 201 472 | 0,00 / 0,06 |
| Accumulation, `-n '1h;1!H;$ {g;p;}'` | 1 / 8 | 4 390 912 / 20 185 088 | 0,01 / 0,07 |

Avec une limite d’essai de 65 536 octets par allocation du moteur, 400 000 octets accumulés entraînent le statut 4, un diagnostic mémoire et zéro octet de sortie. Les ELF de production excluent l’injection. Le pic privé du processus en exécution sur un fichier P9 demeure non mesuré. [Protocole](resources.py)

## Invités, ARC-026 et capacités

QEMU, 2 vCPU, AArch64 EL0 et x86_64 Ring 3. Le témoin SDK exécute sept scénarios internes, quatre rejets et 64 vecteurs dérivés directement des sorties GNU en locale C. Le noyau confirme la sortie 0 et la restitution après collecte pour les deux architectures. Le témoin utilise le code source des ELF livrés, avec la fonctionnalité de qualification et sans port de terminal ni canal fichier. Les essais de documentation et de refus tournent séparément sur le noyau ordinaire avec quatre volumes NovaFS P9 jetables.

Sur chaque architecture et chaque taille de secteur (512 et 4 096 octets), `nsh` confirme le statut 0 de l’aide, de `-i --help`, de la version et du manuel, pour les tags `en-US`, `fr-CA` et `fr--CA`. Le dernier choisit l’anglais. Le SHA-256 brut, les octets écrits, les opérations d’écriture et les flush du disque ont été comparés avant et après les consultations et les refus. Les quatre compteurs/empreintes restent identiques dans chaque session.

L’édition en place et l’exécution sur un fichier depuis `nsh` renvoient 4. Chacune des trois commandes de pipe renvoie 3 avant la création des enfants. Il n’existe donc pour sed sur cette base ni statut de cat bloqué, ni événement de réveil, ni décompte de wc à présenter. La restitution SDK porte sur le processus témoin. Les refus préalables, les accès nommés bloqués et les protections du volume sont les résultats disponibles. [Banc invité](guests.py) · [Prérequis MOA/MOE](../README.md#architecture-et-décision-demandée-à-la-moamoe)

## Non-régression et requalification de stabilisation

Sur la branche de livraison, `elf-test`, `lifecycle-test`, `cli-test` et `nsh-test` réussissent sur les deux architectures. `nsh-terminal-test` réussit sur AArch64 et au rejeu isolé sur x86_64. Le premier essai x86_64 avait dépassé l’attente de la session. Le catalogue documentaire passe ses six tests sur Darwin ARM64 et sur x86_64 sous Rosetta.

Sur la base initiale, `cli-p9-test` échoue au parcours `ls -latr --color /` et `nsh-scripts-test` au parcours `**/*.txt`. Ces échecs se reproduisent sur une extraction intacte du SHA de départ dans les deux architectures. La stabilisation publiée sur `main` au SHA `e5d7124a53e9b0d6a896b7001f717f655c371367` (arbre `e73ce9e2209449bda3e1e1475ac15053ac3e312e`) modifie la pagination et le traitement des continuations de lecture. Le chemin prévu par sed délègue un unique handle, appelle STAT et `Client::read`, sans énumérer de répertoires. La lecture non groupée utilise des fragments indépendants. Le modèle de délégation et les sélecteurs de pipe restent ceux du mandat.

La requalification a appliqué le delta source sed à une extraction isolée de cette stabilisation. `cli-p9-test` et `nsh-scripts-test` réussissent alors sur les deux architectures. Le témoin SDK, la restitution, les consultations ARC-026, les trois refus de pipe, les refus fichier et les instantanés P9 passent également pour chaque invité et les deux secteurs sur la base stabilisée. L’exécution de sed avec fichier ou pipe attend toujours l’arbitrage d’admission.

## Rejouer

Depuis la racine, définir `NOVAOS_TMPDIR`, `TMPDIR` et `PATH` selon le [guide de reconstruction](../README.md#reconstruction). Le banc invité requiert les dépendances de construction NovaOS et l’outil hôte NovaFS P9.

```sh
export RUSTC="$(rustup which --toolchain stable rustc)"
mac=$(components/sed/build.sh macos)
probe=$(CARGO_TARGET_DIR="$NOVAOS_TMPDIR/arc027-sed-fault" components/sed/build.sh macos --features fault-tests)
out=$(mktemp -d "$NOVAOS_TMPDIR/recette-sed.XXXXXX")
(cd components/sed && CARGO_TARGET_DIR="$NOVAOS_TMPDIR/arc027-sed-target" rustup run stable cargo test --locked --offline --target aarch64-apple-darwin)
python3 components/sed/validation/differential.py "$mac" "$out"
python3 components/sed/validation/minimise.py "$mac" "$out"
python3 components/sed/validation/resources.py "$mac" "$probe" "$out/resources.json"
(cd novaFS && NOVAFS_TARGET=aarch64-apple-darwin lab/run build --offline --bin novafs)
for arch in aarch64 x86_64; do
  python3 components/sed/validation/guests.py "$arch" "$out/$arch"
  for suite in elf-test lifecycle-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test; do
    make -C novaOS ARCH="$arch" "$suite"
  done
done
```

Les ELF de production, les ELF témoins, les noyaux témoins, les configurations QEMU, les images P9/ISO, les traces UART, les résultats Darwin, les journaux de faisabilité et les régressions se trouvent dans les livraisons. Chaque archive contient un manifeste interne par fichier. [Livraison et SHA-256](../../../distribution/sed-arc027/)
