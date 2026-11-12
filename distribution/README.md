# Distribution NovaOS

NovaOS v0.15.0 fournit un installateur AArch64 pour UTM 5.0.8 avec NovaFS v0.9.13 P9. L’archive contient le support QCOW2 en lecture seule et le firmware expérimental propre à chaque VM. [Télécharger l’image](images/novaos-0.15.0-utm-aarch64.tar.gz) · [Créer la VM et installer NovaOS](../novaOS/docs/installer-v0.15.0.md#installation-dans-utm-508)

| Emplacement | Usage |
|---|---|
| `manifests/` | Sélection et versions des composants à livrer. |
| `images/` | Image de livraison UTM AArch64. |
| `scripts/` | Assemblage et validation des livraisons. |

Le projet de distribution prévoit une ISO UEFI x86_64 et une IMG UEFI AArch64, avec essais de démarrage dans QEMU et vérification de compatibilité UTM. Le modèle hybride immuable séparera le système livré des données modifiables. Les mises à jour atomiques et signées et le retour à une version précédente restent à concevoir et à valider.

Le Makefile de NovaOS produit également les deux ISO de développement dans `novaOS/build/`. [Démarrage des images actuelles](../novaOS/README.md)
