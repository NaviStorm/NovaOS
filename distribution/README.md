# Distribution NovaOS

NovaOS v0.16.4 fournit un installateur AArch64 pour UTM 5.0.8 avec le shell de scripts, l’éditeur `vi` et NovaFS v0.9.13 P9. L’archive contient le support QCOW2 en lecture seule et le firmware expérimental propre à chaque VM. [Télécharger l’image](images/novaos-0.16.4-utm-aarch64.tar.gz) · [Créer la VM et installer NovaOS](../novaOS/docs/vi-v0.16.4.md#installation-dans-utm)

| Emplacement | Usage |
|---|---|
| `manifests/` | Sélection et versions des composants à livrer. |
| `images/` | Image de livraison UTM AArch64. |
| `scripts/` | Assemblage et validation des livraisons. |

Le projet de distribution prévoit une ISO UEFI x86_64 et une IMG UEFI AArch64, avec essais de démarrage dans QEMU et vérification de compatibilité UTM. Le modèle hybride immuable séparera le système livré des données modifiables. Les mises à jour atomiques et signées et le retour à une version précédente restent à concevoir et à valider.

Le Makefile de NovaOS produit également les deux ISO de développement dans `novaOS/build/`. [Démarrage des images actuelles](../novaOS/README.md)

Les livraisons précédentes restent accessibles pour vérifier les images P9 et les anciens shells. [Archive v0.16.0 avec vi](images/novaos-0.16.0-vi-utm-aarch64.tar.gz) · [Archive v0.16.0 antérieure à vi](images/novaos-0.16.0-utm-aarch64.tar.gz) · [Archive v0.15.1](images/novaos-0.15.1-utm-aarch64.tar.gz) · [Qualification v0.15.1](../novaOS/docs/cli-v0.15.1.md) · [Archive v0.15.0](images/novaos-0.15.0-utm-aarch64.tar.gz) · [Qualification de l’installation GPT](../novaOS/docs/installer-v0.15.0.md)
