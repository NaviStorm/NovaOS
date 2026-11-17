# mktemp · ARC-027 · 0.24.0

La branche `codex/mktemp-natif-0240` part de `origin/main` vérifié à `425e4e7cc5c03e246efb64d3638376906344bf47`. Les exécutables sont [le Mach-O ARM64](bin/mktemp), [l’ELF AArch64](bin/mktemp-aarch64.elf) et [l’ELF x86_64](bin/mktemp-x86_64.elf). Les deux ELF sont statiques. Les empreintes, les outils et le matériel figurent dans [l’environnement](preuves/environnement.txt). Le [bilan par profil](RESULTATS.md) et la [matrice par invocation](MATRICE.md) donnent les résultats observés.

## Références et licence

GNU Coreutils 9.12, tag `v9.12`, commit `c0f8514d989184921d9b12a4d103a7b23abc5af8`. Lecture de `src/mktemp.c`, de la section mktemp de `doc/coreutils.texi` et des quatre fichiers de `tests/mktemp/`. Le générateur gnulib à `106e9b2384d08a1696fcbd40cbab52237943f208` utilise la création exclusive, mkdir ou lstat selon le mode. Le parseur accepte aussi `-V`. L’oracle est le Mach-O Homebrew `/opt/homebrew/bin/gmktemp` 9.12, formule 9.12_1. [Source GNU](https://github.com/coreutils/coreutils/blob/v9.12/src/mktemp.c) · [Manuel](https://www.gnu.org/software/coreutils/manual/html_node/mktemp-invocation.html) · [Générateur](https://github.com/coreutils/gnulib/blob/106e9b2384d08a1696fcbd40cbab52237943f208/lib/tempname.c)

L’implémentation MIT du composant utilise GNU GPLv3+ comme oracle externe. uutils MIT a été étudié à `4735bbe40d764a523c5ece37d47465b32452d1a2` dans `src/uu/mktemp/{Cargo.toml,src/mktemp.rs,LICENSE}`. Sa dépendance à std, clap, rand et tempfile aurait nécessité un second chemin pour NovaOS. Le parseur et le moteur de modèles livrés ici utilisent core et alloc. Le code partagé de NovaOS et l’outillage de recette reprennent les interfaces et bancs MIT du projet. Les licences déclarées pour la résolution Cargo complète sont [inventoriées](preuves/licences.txt). Les projets tiers restent sous leurs propres licences.

## Comportement

Sans opérande, le modèle est `tmp.XXXXXXXXXX`. Avec un modèle explicite, seule la dernière série de trois X ou plus est remplacée. Une partie finale après le dernier X forme le suffixe implicite. `--suffix` exige un modèle se terminant par X. Un suffixe contenant une barre échoue. `-d` crée un répertoire et `-u` rend un nom libre au moment de la vérification, sans réservation. `-q` masque l’erreur métier, y compris en combinaison. La validation du modèle reste audible. Sur Darwin, les permissions demandées sont 0600 et 0700, réduites par umask. Les candidats utilisent arc4random_uniform. Un lien symbolique ou un objet existant provoque un nouvel essai. Une erreur d’écriture du nom entraîne une tentative de retrait de l’objet créé. Les noms et diagnostics des essais en locale C sont conservés en hexadécimal.

`-p DIR` accepte aussi sa valeur accolée. `--tmpdir` accepte une valeur facultative après le signe égal. La priorité normale est le répertoire explicite non vide, puis TMPDIR, puis la racine temporaire de la cible. Avec `-t`, TMPDIR prend priorité et le modèle doit être un seul composant. `--` termine les options. Statuts 0 pour le succès, 1 pour un échec du programme. Le statut 127 observé dans nsh concerne la recherche du programme. La documentation ARC-026 utilise la page partagée et le validateur IANA de NovaOS. Une langue principale française valide affiche le français. Les autres entrées utilisent l’anglais.

## Création dans NovaOS

La racine logique `tmp/` est celle du demandeur. Le chemin relatif est résolu dans sa vue. Les chemins absolus et les remontées échouent. P9 fournit les droits de la capacité et le type d’objet. Le protocole v2 peut fragmenter un chemin dans la limite de `PATH_MAX`. Les limites de P9 sur les composants et la profondeur restent applicables.

Pour les répertoires, MKDIR_AT est exclusif. Pour un fichier, OPEN_AT avec CREATE accepte aussi un fichier existant. Le client crée donc un répertoire de préparation neuf, y crée un fichier vide, ferme son handle puis publie le fichier par RENAME_AT. Le moteur P9 refuse une destination occupée dans la transaction. Après une collision normale, le client nettoie sa préparation et change de candidat. Une issue UNCERTAIN remonte au demandeur. Une préparation incomplète peut subsister jusqu’au retrait du temporaire. La publication utilise les deux slots PATH_FRAGMENT de la session. Le demandeur de recette attend son travailleur avant de soumettre une autre opération sur cette vue. Les deux demandeurs du profil concurrent possèdent des instances distinctes.

### Entrées exercées

- **nsh ordinaire.** L’aide, la version, les abréviations documentaires et le manuel passent par le catalogue embarqué. Les compteurs de WRITE et FLUSH et le SHA du disque sont relevés autour des consultations. Le lancement métier exige encore l’installation de l’ELF, l’authentification de la session, les flux et la délégation de la vue temporaire du demandeur. La branche parallèle `codex/lancement-generique-elf` documente ce raccord encore ouvert à `a94b4a6e869122edde227ebdd293aedbcf5dc43f`. Sa révision distante suivante `f8966d38fa5ef021e4b2354780e0a0c4dd69f946` ajoute les preuves du registre SPAWN. La base testée conserve les 48 sélecteurs de l’ABI v1.
- **ELF de livraison en travailleur.** Le noyau de recette sélectionne les octets de l’ELF comme image du travailleur de l’application témoin. Le parent lance TMP_WORKER avec sa propre identité et un handle ouvert dans sa vue. Le noyau délègue la capacité réelle. Le demandeur attend le statut, lit la réponse, recherche le nom et rouvre le fichier ou vérifie le type répertoire. Le droit de démarrer un autre ELF depuis le shell ordinaire reste du ressort du lanceur.
- **Échange de recette.** Le seul mot de configuration du travailleur est le handle. La requête contient `MKTREQ01`, 32 octets de graine, l’étiquette de langue et TMPDIR terminés par NUL, puis chaque argument terminé par NUL. La réponse commence par `MKTRES01`, la longueur utile u32 little-endian et les octets de résultat ou de diagnostic suivant le statut. La graine de recette est l’identité de démarrage de l’instance. SHA-256 combine la graine, le PID et un compteur. Le caractère privé repose sur la capacité et l’exclusivité de la création. La graine de recette est publique. L’échange par handle réalise des E/S P9 propres au laboratoire, y compris quand il transporte un résultat dry-run. Les consultations publiques passent directement par le catalogue.
- **Sonde du moteur.** Un fournisseur déterministe propose d’abord le nom occupé puis un nom libre. Les contrôles du parent après la sortie distinguent la collision, la création et le retrait dans les deux types. Le banc Darwin force séparément une collision avec un lien symbolique et relit son contenu cible. Le banc SDK sans ports teste le refus de sortie du véritable ELF. Le banc de flux de Darwin collecte stdout par pipe.

## Rejouer

Depuis la racine de ce worktree, avec les toolchains Rust et cibles du projet, GNU Coreutils, les dépendances Cargo locales, QEMU, EDK II, xorriso et Python 3.11 ou ultérieur, exécuter les commandes ci-dessous. Les scripts placent leurs temporaires sous `OS-worktrees/tmp/mktemp-arc027`. Chaque session QEMU crée ses propres images. Les campagnes conservent les journaux, les images et les références de noyau.

```sh
d=livraison-mktemp-0240
"$d/reconstruire.sh" tout
"$d/tester-options-mktemp.sh" --rapide
"$d/tester-options-mktemp.sh" --complet
"$d/mesurer-performances-mktemp.sh" --rapide
"$d/mesurer-performances-mktemp.sh" --complet
"$d/qualifier-qemu.sh"
```

Les deux scripts acceptent un dernier argument pour le rapport JSON sur Darwin ou `--novaos` pour le banc invité sous verrou. Le mode invité rapide exerce AArch64/P9-512 et le mode complet les quatre profils. L’oracle QEMU utilise les vecteurs de la matrice publiés avec cette livraison. Les noms aléatoires sont comparés par préfixe, suffixe, alphabet, longueur, emplacement, type et droits. Les statuts, stdout et stderr sont comparés en octets quand la sémantique le permet. Le nom Homebrew `gmktemp` au début du diagnostic et le contenu documentaire propre à chaque produit sont traités explicitement. Les erreurs de stockage distinguent le diagnostic natif et errno Darwin. Les mesures Darwin incluent le lancement du processus, le pipe, la création et l’attente. Les contrôles d’unicité et le nettoyage se déroulent après le chronométrage.

Pour archiver une nouvelle campagne, passer le répertoire imprimé par le banc QEMU et un nom d’archive libre à `python3 -B livraison-mktemp-0240/lab/archiver.py`. Le vérificateur relit les octets décompressés de chaque membre et contrôle son SHA-256.
