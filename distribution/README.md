# Distribution NovaOS

La distribution doit à terme sélectionner NovaOS, NovaFS et les applications, assembler les images et fournir la configuration initiale.

| Emplacement prévu | Usage |
|---|---|
| `manifests/` | Sélection et versions des composants à livrer. |
| `images/` | Images produites localement. |
| `scripts/` | Assemblage et validation des livraisons. |

Le projet de distribution prévoit une ISO UEFI x86_64 et une IMG UEFI AArch64, avec essais de démarrage dans QEMU et vérification de compatibilité UTM. Le modèle hybride immuable séparera le système livré des données modifiables. Les mises à jour atomiques et signées et le retour à une version précédente restent à concevoir et à valider.

Les images de développement actuelles sont deux ISO construites par le Makefile de NovaOS dans `novaOS/build/`. Les sous-répertoires réservés seront suivis dès leur premier contenu. [Démarrage des images actuelles](../novaOS/README.md)
