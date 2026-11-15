# Conservation des preuves et des worktrees

Relevé du 15 novembre 2026 après publication de la phase 4, avant rédaction de la branche documentaire. Le point de comparaison est `origin/main` à `4569a6876720f6b4efa901db707b670e415abeeb`. Les entrées Git ont été comptées par chemins séparés par NUL. Les non-suivis excluent les ignorés. Les suivis modifiés sont comptés séparément. Le dépôt principal est dans `StudioProjects/OS`, tous les worktrees additionnels dans le répertoire frère `OS-worktrees`.

| Worktree | Branche | HEAD complet | Suivis | Modifiés | Non suivis | Ignorés | Motif de conservation ou de retrait |
|---|---|---|---:|---:|---:|---:|---|
| OS | `main` | `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e` | 1178 | 1 | 2 | 929598 | Dépôt principal de Thierry, modifié avant l’audit. |
| novaos-abi-registry-v1 | `codex/abi-registry-v1` | `df88f1cfeaf0940da876ea543584e59215abacd4` | 1223 | 0 | 0 | 3408 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-audit-v0300-010ae39 | `codex/audit-v0300` | `a602051c9f3f1119552dd51a6b58ffc64285c556` | 1029 | 0 | 0 | 0 | Branche non intégrée à origin/main. |
| novaos-cache-0210-phase4 | `codex/cache-0210-phase4` | `4a3d3079bad8a8ef074601df5afdf9c6d5faacfb` | 1288 | 0 | 0 | 8228 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-cloture-0210 | `codex/cloture-0210` | `4569a6876720f6b4efa901db707b670e415abeeb` | 1288 | 0 | 0 | 0 | Branche active du rapport (relevé avant rédaction). |
| novaos-fusion-abi-tmp-0210 | détaché | `bcdfe4ccfd98c9e4a58f5cead3ce666e917ce33c` | 1255 | 0 | 0 | 67 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-fusion-cache-0210 | détaché | `4569a6876720f6b4efa901db707b670e415abeeb` | 1288 | 0 | 0 | 0 | Fusion publiée, détaché et sans contenu local supplémentaire détecté par Git. Retrait possible après publication et contrôle d’inactivité. |
| novaos-fusion-smp-835b9d9 | `codex/finalisation-smp-0170` | `573137c1f567ab98583fdd3138da39c228cef452` | 1013 | 0 | 0 | 137857 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-instances-0210 | `codex/instances-0210` | `6b75a2bd0a449d2cb3e36c1db9b50292b811280e` | 1208 | 0 | 0 | 3976 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-integration-abi-tmp-0210 | `codex/integration-abi-tmp-0210` | `3839f1a2525e7ad977b16b4a6b4aeacf4880bc63` | 1255 | 0 | 0 | 14810 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-materiel-0180-010ae39 | `codex/materiel-0180` | `a467fe6867587f8818f2add99ed11e8c215a3228` | 1082 | 0 | 0 | 12217 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-preparation-0210 | `codex/preparation-0210` | `622d8d97e547434d62b65da00827324cf9405507` | 1180 | 0 | 0 | 0 | Branche non intégrée à origin/main. |
| novaos-services-0210-phase3 | `codex/services-0210-phase3` | `aa2c44a81e3ce444a75db40efa91f91f68b093a4` | 1273 | 0 | 0 | 4209 | Archivage exhaustif de tout le contenu ignoré non établi. |
| novaos-tmp-prive-0210 | `codex/tmp-prive-0210` | `35f7d9619f116f42491f001af4b1cf5bdc84b395` | 1228 | 0 | 0 | 3303 | Archivage exhaustif de tout le contenu ignoré non établi. |

## Vérifications matérielles des copies

- L’index `OS-worktrees/preuves-cache-0210-phase4-80a3699/index-sha256.json` a pour SHA-256 `a1cf7197c8a5cf359f7f5429e581f30ce337b5c86a32cabcc58932a9ef0d589a`. La relecture des **4 159** fichiers indiqués dans l’index et des fichiers correspondants dans le worktree du candidat a vérifié les deux contenus, leur taille et des inodes distincts pour chaque paire. Somme des tailles logiques des entrées vérifiées : **2 720 208 175 octets**. Les sous-arbres `banc/` et `publication/` correspondent à `novaOS/build/cache-p4/` et `novaOS/docs/validation/` du candidat.
- L’archive compacte suivie dans le dépôt a pour SHA-256 `6034653cb7c2182cb3a6010db909e1ed9ed1f0086b04510f5ccc12f0051b3bf2`. Les **510** contenus déclarés dans son index interne ont été relus et vérifiés. Les pièces volumineuses restent disponibles dans l’archive physique sœur. [Archive versionnée de phase 4](../validation/cache-phase4-v0.21.0-preuves.tar.gz)
- Dans le worktree candidat, l’empreinte d’inventaire des 1 288 chemins suivis vaut `652928e252401f3fe0453b43f9c6c840e1d43d1ddd83cd58c505a419b722430b` et celle des 8 228 ignorés `5f0ca19717806b5b3fdf084a2414bc8b4e2b5896e3202c40a60dcdc8036df5f2`. Chaque entrée réunit dans l’ordre Git le nom, le mode et l’empreinte du contenu (ou la cible du lien), séparés par NUL. L’index de 4 159 copies couvre les preuves certifiées. Un produit de construction local `cache_client_test.bin` de SHA-256 `780ea1505f0bb401eb17ffaf9735965c8af43608f81acb6df9c63b11aba10357` est absent de cet index. Le worktree de phase 4 est conservé.
- Les historiques avec contenus ignorés contiennent entre autres séries QEMU, ELF, images et résultats de banc. L’archive précédente sous `banc/worktrees/` identifie individuellement des copies dans `copies-locales-et-archives-sha256.json.gz` et ses limites dans `bilan.md`. Une preuve de couverture partielle ne permet pas la suppression de ces worktrees.
- Le principal contient une modification de `.gitignore` et deux liens locaux non suivis. Le bilan de livraison les contrôle en lecture seule. Les deux branches historiques sans worktree, `codex/smp-0170` et `codex/stockage-smp-0167`, disposent chacune d’une branche distante au même commit, respectivement `835b9d9dd817a2feef54b6c17f000b0aec8a3879` et `7aaf6dc1672f54e3a91f37cf8ab87837d62f1912`. L’état après publication et nettoyage est consigné dans le bilan de livraison.
