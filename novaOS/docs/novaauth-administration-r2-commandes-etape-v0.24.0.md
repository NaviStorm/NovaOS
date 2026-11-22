# NovaAuth v0.24.0 — R2 : façades des commandes et historique

Point de départ : `85d7d5bf4dc9c3eac9cf61811d0fa5a5a4f3638e`.
Cette étape n'est pas une qualification R2 de bout en bout.

## Résultats du 22 novembre 2026

| Contrôle | Environnement | Résultat |
| --- | --- | --- |
| Grammaires, requêtes et réponses des commandes, NXEXEC01/NXADMIN1 | Darwin ARM64 natif | 18/18 |
| Mêmes contrôles | Darwin x86_64 sous Rosetta | 18/18 |
| Codecs mutations/époques/admissions/historique | Darwin ARM64 natif | 9/9 |
| Mêmes contrôles | Darwin x86_64 sous Rosetta | 9/9 |
| Autorité de mutation : publication, concurrence, drainage, consultation sans effet | Python hôte | 6/6 |
| Six ELF administratifs | AArch64 `unknown-none-softfloat` | Compilation réussie |
| Six ELF administratifs | x86_64 `unknown-none` | Compilation réussie |
| Mutations natives, deux sessions et effecteurs R2 | Quatre profils QEMU | Non exécutés à cette étape |

`whoami`, `id`, `groups`, `useradd`, `usermod` et `groupadd` disposent de façades
natives sur NXADMIN1. La présence du client ne signifie pas que le serveur de
mutation est disponible. Les préparations/confirmations, les effecteurs de
révocation, les facteurs préenregistrés et les deux sessions restent à raccorder
et qualifier. L'entrée `man` du shell reste à intégrer ; l'argument `man` de
chaque ELF est déjà local et testé, comme `--help` et `--version`.

Le premier essai de compilation a choisi par erreur `aarch64-unknown-none`,
absente de la toolchain. Aucun composant n'a été installé : le rejeu emploie la
cible NovaOS existante `aarch64-unknown-none-softfloat`. Le journal négatif est
conservé. Rust stable 1.99.1 est utilisé explicitement ; ce contrôle ne lève pas
la réserve de reproductibilité entre environnements.

## Preuves

Archive durable hors temporaires :
`/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/commandes-85d7d5b-01/`.

Manifeste `SHA256-commandes-historique-01.txt`, 55 fichiers relus :
`974cfecb120c9dd865ba315a74af00949dd5904e99eed7245fb5e18b8c9cfbb7`.
Il conserve les journaux positifs/négatifs, les commandes de construction,
les douze ELF, les sources correspondantes et le point de reprise préparatoire.
Aucune clé privée de laboratoire n'y est ajoutée.

Aucune VM lancée pour cette étape : attente du verrou QEMU et temps d'exécution
QEMU nuls. La réserve instrumentée AArch64 de 301 cadres reste ouverte. Aucun
noyau ordinaire, format P9, sélecteur ABI ni utilitaire en pause n'a été modifié.
Aucune fusion ni aucun tag ; les évolutions restent sur la branche R2.
