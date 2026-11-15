# Consignes communes au monorepo

## Reprendre le travail

- Au début d’une tâche et après une reprise de contexte, vérifier la branche, les modifications locales et le commit complet avec `git status --short --branch` et `git rev-parse HEAD`. Si la référence du mandat a changé, examiner les commits intervenus et préserver le travail présent.
- Avant de travailler dans un composant, lire ses consignes locales. Pour un changement qui traverse la frontière entre système et stockage, lire les deux fichiers et vérifier le contrat des deux côtés. [Consignes NovaOS](novaOS/AGENTS.md) · [Consignes NovaFS](novaFS/AGENTS.md)
- Travailler et répondre en français, avec concision, sans introduction ni conclusion. Pour une réponse de code, fournir uniquement le bloc nouveau ou corrigé. Expliquer la cause d’un bug corrigé et fournir les bilans expressément demandés.

## Worktrees additionnels et preuves

Le dépôt principal conserve son emplacement `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS`. Créer tous les worktrees Git additionnels de NovaOS exclusivement sous `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/`. Placer les temporaires de développement jetables dans le sous-répertoire frère `OS-worktrees/tmp/`. Ne créer volontairement aucun fichier de travail NovaOS dans `/tmp` ou `/private/tmp`. Cette politique concerne les outils de développement sur macOS. Le futur `tmp` des applications dans NovaOS relève du contrat du système invité.

- Conserver les journaux, images disque, traces et résultats nécessaires aux qualifications. Toute preuve à conserver, y compris les traces, mesures et artefacts nécessaires à la reproduction, doit avoir une copie durable hors de `/tmp`, de `/private/tmp` et de `OS-worktrees/tmp/`. Vérifier son emplacement physique, son existence, sa lisibilité et son SHA-256 en lisant ses octets. Une empreinte publiée seule ne sauvegarde pas le fichier.
- Avant un déplacement, relever le chemin, la branche, le HEAD et l’état des fichiers suivis, non suivis et ignorés. Inventorier les preuves et calculer leurs SHA-256. Vérifier qu’aucune tâche, VM ni aucun processus n’utilise le worktree ou ses images. Reporter le déplacement en cas de doute.
- Utiliser `git worktree move` sans forcer. Préserver les permissions, les liens, les métadonnées Git et les modifications locales. À destination, relire les preuves, recalculer leurs SHA-256 et comparer les inventaires et références Git. Consigner les écarts et conserver les originaux si la migration est bloquée.

### Temporaires des outils macOS

Pour chaque commande, compilation ou test NovaOS, définir ces variables dans le processus de travail et ses descendants. Laisser la configuration de macOS, du shell et de Git globale intacte.

```sh
export NOVAOS_TMPDIR="/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp"
mkdir -p "$NOVAOS_TMPDIR"
export TMPDIR="$NOVAOS_TMPDIR/"
```

- Fournir le chemin explicitement lorsqu’un outil expose un paramètre de répertoire temporaire. Vérifier qu’il est accessible en écriture dans le contexte effectif. Python peut sélectionner une autre racine si le bac à sable refuse le chemin. Plusieurs bancs du dépôt utilisent `TemporaryDirectory(dir=BUILD)` et le constructeur d’ISO donne un chemin explicite à `mktemp`. Contrôler séparément ces chemins et signaler les outils qui imposent le leur. Les caches internes de macOS ou de dépendances peuvent suivre leurs propres règles.
- Vérifier que `OS-worktrees/tmp/` reste hors de tout worktree. Limiter un nettoyage à des éléments identifiés comme jetables après la fin des tests, absence d’utilisateur actif et vérification des archives. Conserver les fichiers en cas de doute.
- Pour éviter de synchroniser les temporaires, proposer à Thierry la désélection de **ce seul sous-répertoire** dans les règles de synchronisation par dossier de la tâche Synology Drive, si le client et son mode le permettent. Vérifier le chemin exact et que les worktrees et archives restent sélectionnés. Toute modification des réglages de synchronisation exige une demande distincte. [Règles de sélection des dossiers Synology](https://kb.synology.com/DSM/tutorial/Why_are_files_not_synced_between_Synology_Drive_and_Drive_desktop_application)

## Composants et décisions

- `novaOS/` contient le micro-noyau, la HAL, l’ABI native, les services et les pilotes. `novaFS/` possède le moteur portable, le format disque et son laboratoire. `components/` contient `nsh`, les commandes et leur bibliothèque. `distribution/` rassemble les livraisons. Préserver les workspaces et les licences respectives, MIT pour NovaOS et les composants CLI, MPL-2.0 pour NovaFS. [Organisation du dépôt](README.md) · [Composants utilisateur](components/README.md) · [Distribution](distribution/README.md)
- Pour établir ce qui est livré, recouper le manifeste, le chemin réellement utilisé et le rapport de livraison. Pour une qualification, préciser le profil et le modèle de panne exercés. En cas d’écart, dater les résultats et retrouver l’arbitrage applicable. Signaler tout désaccord encore ouvert. Le VFS générique est une proposition. NovaUI et les mises à jour du système hybride immuable figurent dans la feuille de route. [Proposition VFS](novaFS/docs/vfs-architecture.md) · [Feuille de route](README.md#feuille-de-route)
- Les manifestes et bannières 0.20.0 identifient les sources NovaOS après intégration des phases 1 et 2 dans `main`. Le SDK Rust porte la version 0.2.0. Le jalon NovaOS v0.20.0 est clos avec la livraison du SDK Rust natif `nova-system` 0.2.0. Ces sources utilisent NovaFS v0.9.13 P9. L’archive UTM qualifiée est en v0.17.0. Leurs versions, le numéro d’ABI et le format disque évoluent séparément. Une rupture de compatibilité, une extension des droits existants ou l’intégration d’un prototype exige un arbitrage qui couvre ce changement. Les délégations de retrait nommées 7 et 8 remontent à v0.15.1. Les variables exportées se transmettent entre appels de `nsh` dans le processus du terminal. L’[éditeur vi](novaOS/docs/vi-v0.16.2.md) enregistre uniquement vers un nom libre ; le choix d’un autre nom pendant l’édition passe par le dialogue privé avec `nsh`. NovaFS P10, PC1 en production et le remplacement d’une destination exigent un autre arbitrage. Actualiser ces repères lors d’une nouvelle livraison validée. [Dossier de clôture v0.20.0](novaOS/docs/cloture-rust-v0.20.0.md) · [Moteur de scripts](novaOS/docs/nsh-scripts-v0.16.0.md) · [Contrat des commandes](novaOS/docs/cli-v0.15.1.md) · [Installation publiée](novaOS/docs/fusion-smp-v0.17.0.md#installation-dans-utm) · [Décision sur la publication transactionnelle](novaFS/docs/novafs-publication-conditionnelle.md#régressions-et-arbitrage)

## Validation et commits

- Exécuter les suites applicables au changement sur les deux architectures selon les consignes locales. Distinguer invités QEMU, exécution Darwin ARM64 native, x86_64 sous Rosetta et application UTM. Consigner le commit, les commandes, les profils, les résultats observés et la raison de tout essai non exécuté. Pour une modification documentaire, vérifier les références, les commandes et la portée des affirmations.
- Committer les changements de code validés sur la branche convenue. Respecter le libellé demandé. À défaut, les commits récents emploient `<composant> vX.Y.Z — <objet>` et précisent `Audit` ou `Spécification` pour ces travaux. Un audit conserve la version qu’il examine. Avant livraison, contrôler le diff indexé, son périmètre et `git diff --cached --check`, puis vérifier le `HEAD` et l’état du dépôt après le commit.
