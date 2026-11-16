# NovaOS v0.23.0 — Clôture des sources

**Portée de la décision soumise à la MOA/MOE le 16 novembre 2026.** Le jalon porte sur les sources intégrées au commit `e5d7124a53e9b0d6a896b7001f717f655c371367`, d’arbre `e73ce9e2209449bda3e1e1475ac15053ac3e312e`. Le dossier d’audit est son descendant direct `cddaa346e6c8845f1f7618bde5ef1dc311328044`. La publication du tag annoté `v0.23.0` est soumise à l’approbation de ses références exactes. [Audit final et inventaire des commandes](audit-cloture-v0.23.0.md) · [Annexe de mesures](validation/audit-cloture-v0.23.0.json)

## Périmètre qualifié

| Fonction | Résultat retenu |
|---|---|
| Catalogue ARC-026 | Les 37 commandes inventoriées disposent de `--help`, `--version` et d’une page du manuel natif. La consultation publique fonctionne sans droit métier. Le choix français ou anglais est partagé par le shell, le manuel et les exécutables documentaires. |
| `cat fichier \| wc -l` | Deux processus natifs transmettent les octets sur un pipe borné à 1 Kio de file. Le lecteur compte les fins de ligne et reçoit la fin de flux. Le shell collecte les deux processus. |
| `cat fichier \| head -n 1` | Sur le témoin de 64 Kio, le lecteur affiche la première ligne et ferme son extrémité. L’écrivain bloqué se réveille avec CLOSED. Les statuts sont 11/0 dans ce cas et 0/0 pour le petit fichier intégralement acquitté. |
| Stabilisation du service de fichiers | READDIR vérifie la révision du répertoire. READ/CONTINUE revalide celle du fichier après un changement de génération du volume. Une écriture indépendante du journal laisse la lecture se poursuivre. Une modification de la cible ou une véritable révocation reste détectée. |

Les qualifications de stabilisation comprennent 24 passages des suites concernées, huit passages de navigation et 32 régressions complémentaires. La relecture du dossier a couvert 153 fichiers des archives documentaires de phase 1, 406 de phase 2, 172 du premier pipe, 170 de l’archive publiée du second pipe, 2 753 originaux de stabilisation et 35 pièces de l’audit final. Les archives des campagnes se recoupent. Les 144 observations individuelles de latence, chauffe comprise, ont été rapprochées des séries et les 16 couples médiane/maximum recalculés. Les neuf contrôles du registre ABI ont réussi au cours de cette préparation. [Documentation](documentation-v0.23.0-phase2.md) · [Premier pipe](pipe-natif-v0.23.x.md) · [Second pipe](pipe-head-v0.23.x.md) · [Stabilisation et preuves](stabilisation-v0.23.x.md)

## Profils et limites

- **QEMU direct.** Les essais en invité portent sur AArch64 et x86_64 avec QEMU 11.1.3 TCG. Les pipes et la stabilisation couvrent NovaFS P9 avec secteurs de 512 et 4 096 octets. La recette documentaire des quatre lots a utilisé P9/512. Les latences publiées concernent le terminal série, deux vCPU, 256 Mio et le protocole décrit dans l’audit. La charge soutenue reste à qualifier.
- **UTM.** L’image AArch64 distribuée demeure en v0.17.0. Son SHA-256 relu est `5930b0d6cb3d1e2307f8f630fefa52af8e76967f4f13a4c34674be8b94205f9c`. L’installation et l’interface UTM de ces sources v0.23.0 restent à qualifier.
- **Matériel physique.** L’exécution de ces fonctions sur un serveur réel et la récupération après perte électrique physique restent à qualifier. Les coupures de VM et les injections F1/F2 ont leur propre périmètre.
- **Compatibilité et développements parallèles.** Le noyau et les commandes affichent 0.20.0, le SDK nova-system porte la version 0.2.0, l’ABI native v1 conserve 48 sélecteurs et NovaFS reste en v0.9.13 P9. Les chantiers `grep`, `sed` et `awk` suivent leurs branches et leurs arbitrages distincts.

Le rapport et l’annexe fixent les preuves de cette décision. L’annexe Git correspond octet pour octet à la copie archivée, de SHA-256 `c5de31d6de6f7879f517b5366bc8581e68f6b398e28f26805ad5ddf09407f88e`. [Provenance de l’image et du SDK](audit-0220-abi-livraisons.md)
