# NovaOS sed 0.1.0 · livraison ARC-027

| Fichier | Usage |
|---|---|
| [sed-macos-arm64](sed-macos-arm64) | Exécutable Darwin ARM64. Bibliothèques système du Mac. |
| [sed-aarch64.elf](sed-aarch64.elf) | ELF statique de production NovaOS AArch64. Admission de la commande en attente. |
| [sed-x86_64.elf](sed-x86_64.elf) | ELF statique de production NovaOS x86_64. Même contrat. |
| [preuves-darwin.tar.xz](preuves-darwin.tar.xz) | 33 fichiers de preuves, plus manifeste interne. Oracle, diff, cas réduits, mémoire, pannes, essais des moteurs candidats et binaire d’injection. |
| [preuves-novaos.tar.xz](preuves-novaos.tar.xz) | 106 fichiers de preuves, plus manifeste interne. Profils QEMU des deux bases, noyaux et ELF témoins, UART, images, disques et régressions. |
| [LICENCES.txt](LICENCES.txt) | Attributions et licences du moteur et des dépendances. |
| [SHA256SUMS](SHA256SUMS) | Empreintes des deux archives, des trois exécutables et des licences, calculées et vérifiées par relecture. |

Depuis la racine du dépôt, vérifier la livraison puis essayer le témoin Darwin.

~~~sh
(cd distribution/sed-arc027 && shasum -a 256 -c SHA256SUMS)
printf 'a1\nb2\n' | distribution/sed-arc027/sed-macos-arm64 -nE 's/^([a-z])([0-9])$/\2\1/p'
~~~

La qualification NovaOS couvre le moteur en processus utilisateur et la documentation depuis nsh. La lecture d’un fichier, les deux rôles de pipe et le remplacement sur NovaFS P9 dépendent des arbitrages décrits dans le composant. [Architecture et reconstruction](../../components/sed/README.md) · [Matrice](../../components/sed/validation/matrice.md) · [Rapport](../../components/sed/validation/qualification.md)
