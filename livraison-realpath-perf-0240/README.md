# REALPATH — qualification du seuil de 1,10 × GNU

Le candidat `cddb17a8e144b264e32b1b3d0457964215885cf6` respecte le seuil sur les six charges de 512 chemins, dans deux séries indépendantes sur macOS ARM64. La charge d’erreurs donne les deux ratios maximaux, **1,0584334371** et **1,0637380447**. Le chemin logique `lien/..` donne **1,0546673585** et **1,0500847366**. La première campagne avait mesuré 10,799 ms pour Nova et 9,665 ms pour GNU sur cette charge. Les nouvelles séries remesurent aussi l’exécutable publié dans les mêmes conditions que le candidat. [Tableau publié / candidat / GNU et dispersion](preuves/tableau-darwin.md) · [Série 1](preuves/cddb17a/serie-1.json) · [Série 2](preuves/cddb17a/serie-2.json)

## Parcours et modification

En mode physique, le moteur suit le lien pendant la traversée puis applique `..` à la cible. En mode logique, il vérifie le répertoire, applique `..`, puis traverse le chemin absolu résultant pour résoudre les liens. Ces deux passes correspondent à la référence GNU 9.12. Sur `lien/..`, la deuxième passe parcourt ainsi les composantes du cwd. L’ancienne file copiait chaque composante dans sa propre allocation et dupliquait le chemin du lien dans le marqueur de fin de récursion.

Le moteur emprunte désormais les composantes de l’opérande, possède celles des cibles de liens et réserve la place de l’opérande dans le chemin de travail. Le marqueur de fin retire le dernier lien actif. Sur Darwin, le même couple de tampons sert aux appels successifs de readlink, avec agrandissement si la cible occupe tout le tampon. Chaque nom et chaque lien font toujours l’objet des vérifications prévues. Le calcul du cwd reste effectué pour chaque opérande relatif. [Moteur](../components/realpath/src/path.rs) · [Adaptateur Darwin](../components/realpath/src/darwin.rs)

## Protocole et portée

- Les trois programmes traitent exactement les mêmes arguments, le même répertoire et la même fixture, en locale C. L’oracle Homebrew est GNU Coreutils 9.12. Le binaire de comparaison est relu dans le commit `e2637f692f7ad65313258d2396df21852b0c8d99`. Chaque série consigne les empreintes des trois binaires.
- Les mesures couvrent le lancement par posix_spawn et l’attente par wait4, stdout et stderr vers /dev/null. Pour chaque charge, le banc vérifie séparément les flux et le statut avant et après les mesures. L’échauffement est exclu. Chaque série utilise un nouveau répertoire, un nouvel ordre des charges et trois blocs réalisant chacun les six permutations des programmes, soit 18 observations par binaire et par charge.
- La campagne occupait le créneau Darwin attribué par la coordination. Aucun processus QEMU n’a été relevé aux deux bornes de chacune des douze mesures de charge. Les données brutes comprennent charge hôte, temps CPU, maximum résident, temps individuels, quartiles et écart absolu médian. La décision par charge utilise les sommes des deux temps centraux entiers et la comparaison croisée avec 11/10.
- **Darwin fonctionnel**. 221 cas du banc historique et neuf lots mixtes de chaînes, cibles longues, cycles, noms absents, refus d’accès et sorties NUL réussissent. La suite GNU amont réussit pour GNU et le candidat. Avec un consommateur fermé, le signal ou le statut et le diagnostic concordent selon le traitement de SIGPIPE. [Différentiel](preuves/options.json) · [Chaînes et erreurs](preuves/chaines.json) · [Suite amont](preuves/gnu-amont.txt) · [Pipe](preuves/pipe.json)
- **NovaOS**. Huit démarrages QEMU, un invité REALPATH à la fois. Sur chaque ISA et géométrie, la sonde vérifie 57 jeux d’arguments du moteur et le noyau de recette lance huit fois l’ELF livré dont les octets sont vérifiés dans le noyau démarré. Ce profil sans port observe les refus des flux et le statut 1. Les mesures invitées couvrent le moteur en mémoire, sous TCG, 2 vCPU et 256 Mio. Les volumes P9 synthétiques sont attachés en lecture seule et relus après arrêt. Les neuf tests du registre ABI réussissent. [Mesures par invité](preuves/tableau-qemu.md) · [Registre](preuves/abi.txt)
- Les règles de racine déléguée, la documentation ARC-026 et le contrat des flux conservent la portée de la première livraison. Les scénarios de liens de la sonde utilisent le modèle de test. L’accès aux liens persistants et le lancement métier depuis le vrai nsh dépendent des évolutions externes déjà détaillées. Les régressions de ce correctif portent sur le moteur et les adaptateurs affectés. [Qualification d’origine](../livraison-realpath-0240/README.md)

## Rejeu

Depuis ce dossier dans le worktree dédié, définir `NOVAOS_TMPDIR` conformément au monorepo et `TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/"`. Le répertoire temporaire doit exister. Le Mach-O est dans [bin/realpath](bin/realpath). Les deux [ELF AArch64](bin/realpath-aarch64.elf) et [x86_64](bin/realpath-x86_64.elf) sont liés statiquement.

```sh
python3 -B lab/fonctionnel.py bin/realpath "$TMPDIR/fonctionnel-rejeu"
python3 -B lab/decision.py bin/realpath "$TMPDIR/decision-1.json" --serie 1
python3 -B lab/decision.py bin/realpath "$TMPDIR/decision-2.json" --serie 2
sh lab/qualifier-qemu.sh --compiler
sh lab/qualifier-qemu.sh --executer
python3 -B lab/archiver.py --verifier
```

Pour reconstruire le Mach-O séparément, utiliser Rust stable et `cargo build --release --offline --locked --manifest-path ../components/realpath/Cargo.toml --bin realpath --target aarch64-apple-darwin` avec un CARGO_TARGET_DIR dédié sous TMPDIR. Les livraisons historique et candidate ont leurs propres preuves et binaires. [Archive de la campagne](qualification.tar.xz) · [Manifeste des membres](qualification.sha256)
