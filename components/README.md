# Composants utilisateur

Le workspace Cargo construit un ELF Rust `no_std + alloc` par programme, pour x86_64 et AArch64. Le shell et les commandes utilisent la bibliothèque native de NovaOS.

| Répertoire | Contenu |
|---|---|
| `shell/` | Analyseur et exécuteur partagés par le terminal, les fichiers et `nsh -c`. Édition UTF-8, variables, conditions, boucles, globbing et délégation de capacités. L’historique est privé à la session. |
| `coreutils/` | Programmes isolés `pwd`, `ls`, `cat`, `head`, `mkdir`, `rm`, `rmdir`, `cp`, `mv`, `touch`, `more`, `wc` et `tail`. L’ELF `probe` sert à la qualification. |
| `libraries/` | Catalogue, protocole de lancement, attente, flux de texte et accès au client IPC commun. |

Depuis la racine, `make -C novaOS ARCH=x86_64 cli` ou `make -C novaOS ARCH=aarch64 cli` ouvre le terminal. Le manifeste produit l’exécutable `nsh`. Le paquet de construction fournit aussi le nom historique `shell.elf`. Un interpréteur de remplacement peut utiliser les mêmes interfaces natives. [Scripts et tests](../novaOS/docs/nsh-scripts-v0.16.0.md) · [Commandes natives](../novaOS/docs/cli-v0.15.1.md)

La licence MIT couvre le shell, les commandes et leur bibliothèque. Les dépendances conservent leurs avis de licence. [Licence](LICENSE)
