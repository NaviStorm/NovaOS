# Composants de la distribution

| Emplacement prévu | Contenu envisagé |
|---|---|
| `shell/` | Shell utilisateur et commandes internes comme `cd`. |
| `coreutils/` | Commandes telles que `ls`, `cat`, `cp`, `mv`, `rm`, `mkdir`, `chmod`, `chown` et `chgrp`. |
| `editors/` | Éditeurs tels que `vi`. |
| `viewers/` | Outils de consultation tels que `less` et `more`. |
| `libraries/` | Bibliothèques partagées entre applications. |

Ces emplacements sont réservés. Git matérialisera chaque sous-répertoire lorsqu’il contiendra un premier fichier suivi. Le noyau garde son éditeur de ligne. Les programmes de démonstration et de vérification de l’IPC sont construits avec lui dans [novaOS/user/](../novaOS/user/).
