# Inventaire Git avant la préparation de v0.21.0

**15 novembre 2026.** Au début du mandat, sept worktrees étaient enregistrés. `main` et `origin/main` pointaient sur `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e`. Le huitième worktree a été créé depuis cette référence pour préparer les documents.

| Branche locale | Commit à l’inventaire | Classement | État et motif |
|---|---|---|---|
| `main` | `3514e7804f4c` | Actif | Deux chats Codex actifs ont ce répertoire de travail. Modification locale de `.gitignore`, deux liens symboliques non suivis et des artefacts de construction ignorés. |
| `codex/audit-v0300` | `a602051c9f3f` | À conserver | Propre, y compris les ignorés. Non fusionnée. Le commit est également sur la branche distante de même nom. |
| `codex/finalisation-smp-0170` | `573137c1f567` | À conserver | Fusionnée et publiée. Journaux de coupure, d’IPC et de régression, images disque et relevés locaux. |
| `codex/materiel-0180` | `a467fe686758` | À conserver | Fusionnée. Les trois commits au-delà de la branche distante matérielle sont sur `origin/main`. Traces PCI/VT-d, démarrages série, ISO et images. |
| `codex/nvme-0190` | `8839cff384e6` | À conserver | Fusionnée et publiée. Traces des contrôleurs et de reprise, résultats de l’oracle, images et journaux de régression. |
| `codex/rust-system-0200` | `6533078fc0fe` | À conserver | Fusionnée et publiée. Journaux série de phase 1, relevés, images et exécutables de qualification. |
| `codex/rust-system-0200-phase2` | `b46ddf190267` | À conserver | Fusionnée et publiée. Journaux série de l’exemple et des threads, matrice de compatibilité et produits de compilation. |
| `codex/preparation-0210` | Base `3514e7804f4c` | Actif | Worktree distinct de la préparation documentaire, conservé pour la revue MOA/MOE. |

Les deux branches locales sans worktree, `codex/smp-0170` et `codex/stockage-smp-0167`, sont fusionnées et publiées. Le retrait autorisé vise la branche correspondante après suppression de son worktree. Aucun retrait local ou distant n’a été effectué.

## Vérifications des preuves

L’inventaire a utilisé `git worktree list --porcelain`, `git branch -vv`, les sélections `--merged main` et `--no-merged main`, ainsi que `git ls-remote --heads origin`. Dans chaque worktree, l’état suivi et non suivi a été complété par `git status --short --ignored` et `git ls-files --others --ignored --exclude-standard`. La consultation des 134 chats Codex non archivés n’a identifié aucun répertoire de travail déclaré sous les six worktrees historiques. Ce contrôle ne prouve pas l’absence d’un accès depuis un autre répertoire.

Les cinq worktrees de qualification ont un état Git ordinaire propre. Le relevé des ignorés y trouve respectivement 1 495, 424, 1 183, 36 et 10 fichiers de journal ou de trace (extensions `.log`, `.uart`, `.trace`, `.jsonl`, dans l’ordre du tableau). Une empreinte locale par campagne a été recalculée et retrouvée dans le relevé publié.

| Campagne | Exemple local | SHA-256 du fichier local | Relevé publié |
|---|---|---|---|
| SMP | `remplacement-x86_64.log` | `ed2d89721b65deaeda8281a45b67dc4d3430ad27b7e2e7e574fe5a84e7bf1806` | [Régressions DMA](validation/reservations-dma-v0.17.0/regressions.json) |
| Matériel | `normal-2cpu-sp0sr9ma/qemu.log` | `03c081562e3244704a42c3173ab7a7f5608f7a82d669e2ddcc9aac8c0d42839c` | [Campagne matérielle](validation/fusion-materiel-0180-candidat.json) |
| NVMe | `serie-std.log` | `37c2ea7da833d011d5e16ad0147b4512a93db96bfb64b407b0ae9cc1369eec09` | [Compatibilité NVMe](validation/nvme-phase3-compatibilite.json) |
| Rust phase 1 | `serie-x86_64-threads-test-2cpu-cible.log` | `35e55fea3ceafa3ec0fcbfcf2e4ef94408bb50db4d68af77b6f17c9b7359fb74` | [Qualification des threads](validation/rust-threads-finale-v0.20.0.json) |
| Rust phase 2 | `serie-x86_64-sdk-example-2cpu.log` | `737900c6e141c125a8c641b6b5700448129be919ea47329f77875f5a805cc4b8` | [Qualification du SDK](validation/rust-sdk-phase2-v0.20.0.json) |

**Exemple d’une copie effectivement relue hors du worktree.** Pour le journal matériel `novaOS/build/serie-fusion-candidate-arm.log`, le rapport JSON de la copie principale contient le texte intégral au chemin `architecture/aarch64/serie_demarrage`. Son extraction en UTF-8 a reproduit les octets locaux, avec le SHA-256 `df0511c04fc450b53cd5643e49a555c4981e9179d9da31809040433588788476`. [Rapport contenant la copie](validation/fusion-materiel-0180-candidat.json)

Pour chacun des cinq worktrees, l’archivage intégral demeure invérifié. Les relevés, chemins et empreintes publiés permettent de reconnaître une preuve. Seule la relecture de chaque copie identifiée hors de son worktree et sa comparaison SHA-256 avec l’original permettent d’en autoriser le retrait. Tous les fichiers ignorés ont été conservés.
