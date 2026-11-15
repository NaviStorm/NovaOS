# Audit v0.22.0 — ABI, binaires historiques et livraisons

## Références des contrôles

Le 16 novembre 2026, les contrôles ont porté sur le commit de fusion `b83ebb1dd6eb5aeaa2f67bf478112acfd0c72556`. Son arbre `5f67cf474314b3cf296bdcc602a53a9ba429c5f8` est celui du candidat de phase 3 `dec941d25302ffcd4619aa563ecabb3894257e74`. Les chemins de preuves locales ci-dessous sont relatifs à `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/preuves-cloture-0220/`.

## Matrice de l’ABI native v1

| ID | Exigence et phase | Code et contrôle | Résultat observé et preuve | Statut |
|---|---|---|---|---|
| ABI-01 | Trois phases · préserver les 48 numéros, leurs contrats et leur unicité | [Registre](../abi/syscalls.toml), tests `invalides`, `signatures_historiques`, `extension_services` | 48 numéros distincts sur les deux ISA. Comparaison Git au tag v0.21.0 identique pour le registre et les définitions générées. `controles/selecteurs.json`, SHA-256 `e77efe8b12a5fd4f9317dfa639a64bd57662c07d43d6d7f543f9c6dee381cb66`. | VALIDÉ |
| ABI-02 | Trois phases · faire correspondre les portes et le dispatch au registre | [Dispatch](../kernel/src/sched/process/syscall.rs), tests `portes_et_dispatch`, `selecteurs_litteraux`, `statut_tmp_call` | 48 entrées effectivement traitées, conventions AArch64 et x86_64 contrôlées. Le statut du courtier est couvert. Journal commun des neuf contrôles ci-dessous. | VALIDÉ |
| ABI-03 | Trois phases · régénérer les interfaces Rust, C et C++ | [Générateur](../scripts/generate_abi.py), tests `regeneration`, `exports_tmp`, `entetes_c_cpp` | Sorties reproductibles et identiques aux fichiers suivis. Assertions Rust, C11 et C++11 vérifiées sur les deux cibles. Journal commun des neuf contrôles ci-dessous. | VALIDÉ |
| ABI-04 | Intégration · exécuter l’exemple SDK v0.20.0 AArch64 archivé | [Banc ELF](../tests/abi_elf_archive.py), `aarch64-sdk-example` | EL0, sortie 0 et restitution confirmée. Série SHA-256 `9ae9dd822ecb39cbf38fcd8c83af3ffff6c629833ba3cda46c1fecbe71d24efa`. | VALIDÉ |
| ABI-05 | Intégration · exécuter le témoin de threads v0.20.0 AArch64 archivé | Même banc, `aarch64-threads-test` | EL0, sortie 0, transfert après annulation et collecte unique. 61 981 cadres libres avant et après. Série SHA-256 `ac71c0bea3b55337e29538de12a4be109753786be1626be3db1d4f6d0a99f6af`. | VALIDÉ |
| ABI-06 | Intégration · exécuter l’exemple SDK v0.20.0 x86_64 archivé | Même banc, `x86_64-sdk-example` | Ring 3, sortie 0 et restitution confirmée. Série SHA-256 `c36807a1ba2255d50b5f91613539cd5b40654537726ddba90fdb5e6d6d01eb86`. | VALIDÉ |
| ABI-07 | Intégration · exécuter le témoin de threads v0.20.0 x86_64 archivé | Même banc, `x86_64-threads-test` | Ring 3, sortie 0, transfert après annulation et collecte unique. 62 517 cadres libres avant et après. Série SHA-256 `0d8c6704595e1818c2586d20bf0aa7e389477791094a832248366b8678af5d74`. | VALIDÉ |

La [suite ABI](../tests/abi_registre.py) a réussi **9 contrôles sur 9** sur le commit de fusion. Son journal `controles/abi.log` porte le SHA-256 `2f679a94c9ef3ccd69ca61616f65829aec3f91ffc18998b5039c8904794c6786`. Le SHA-256 du registre relu est `b4d4f183fd64c8c2868891d55f83db4f4bcd9f95f053d91c2915d1f13c543ce8`, celui de [l’en-tête C](../abi/generated/nova_abi.h) est `16123e67faa06eab1fd0f498878fa4eda68777d9d5f31011488f7465dd1cf1a3`.

## Compatibilité binaire effectivement exercée

Chaque essai a démarré QEMU 11.1.3 en TCG avec **deux vCPU**, sur un noyau de sonde `thread-probe` construit avec Rust 1.99.1 depuis la fusion. Le banc embarque l’exécutable utilisateur d’origine octet pour octet dans son catalogue. Il compare son empreinte avant le démarrage à celle de la [qualification initiale v0.20.0](validation/rust-sdk-phase2-v0.20.0.json). Les quatre applications ont été exécutées sans recompilation. Le noyau ordinaire et l’installateur distribué constituent des binaires distincts. [Banc d’exécution et assertions](../tests/threads.py) · [Archive des ELF d’origine](validation/integration-abi-tmp-0210-elf.tar.gz)

| Architecture et profil | Octets de l’ELF utilisateur | SHA-256 de l’ELF v0.20.0 | SHA-256 du noyau de sonde décompressé |
|---|---:|---|---|
| AArch64 · sdk-example | 28 400 | `db2d14ec8a1f8d1ac22acc331ee4d3d90336a90adc8d50ca057c3129e129c3a8` | `aa86a3d7fba998c627042494ceff558384c0f2d628367b69d19cf056637e8755` |
| AArch64 · threads-test | 77 176 | `fa1640df7589a2700f172acc43197c2904e6e2f833fb748048bc15984575cbd9` | `636183bd44c9e17240dd6a6f84adbae803443d83c114db1bdf102305bd75e14a` |
| x86_64 · sdk-example | 22 080 | `1ad5ebf8b6a0d80b10d2df826500f39052fdc222fac418640b408b9657d78c6e` | `7d0cac02a1565e5add139aa6bcb9bbd124eba351908f494e9c1aa283590c3d57` |
| x86_64 · threads-test | 62 736 | `6073a78fb2ffa5c716e75ac942932728faa218d614c3dedf6f30c9f067f6b30c` | `1c6c16268870f893acaf6b3fd7baaff43e5bb4c7b585cdffc7e634753f9274fd` |

L’[archive des résultats du rejeu](validation/elf-0220-rejeu.tar.gz), SHA-256 `5da79a6c3fc05fcc4632f24488702b2e60a7de28827679ba381416854770165d`, contient quatre séries, quatre résultats individuels, le bilan et son manifeste. La copie durable locale `elf/` conserve aussi les quatre originaux, les quatre noyaux de sonde compressés et les journaux de commande. Les 22 fichiers de `elf/SHA256.json` ont été relus. SHA-256 du manifeste `410337cec28b06b445bacf06fd6b4289175214c351a679c091486d52186e734a`. La copie de chaque noyau, une fois décompressée, contient effectivement l’ELF utilisateur entier indiqué dans le tableau.

Pour reproduire un essai depuis la racine d’un worktree de la fusion, appliquer les variables de [temporaires durables](../../AGENTS.md#temporaires-des-outils-macos). Fournir le chemin de l’ELF extrait de l’archive d’origine.

```sh
python3 -B novaOS/scripts/generate_abi.py --check
python3 -B novaOS/tests/abi_registre.py -v
python3 -B novaOS/tests/abi_elf_archive.py aarch64 sdk-example "$ELF_ARCHIVE"
```

Le banc accepte aussi `x86_64` et le profil `threads-test`. L’archive d’origine a pour SHA-256 `b336de3a5bd66c673e812392a68cd1604b6cfb82c204ad2b4dd049e1aac4ceec`. Ses 26 entrées déclarées ont été relues et comparées au manifeste interne. Pour chaque ELF, le SHA-256 a également été comparé à la référence initiale indépendante, SHA-256 du fichier de qualification `09e9cf9753ff4f45a58cd889d4da9bbfbd3fb80b4684fa485ae8d482ed805320`.

## Matrice des versions et des livrables

| ID | Objet et phase | Valeur vérifiée | Contrôle, preuve et portée | Statut |
|---|---|---|---|---|
| LIV-01 | Trois phases · distinguer les sources du numéro de construction | Sources v0.22.0 intégrées par `b83ebb1dd6eb5aeaa2f67bf478112acfd0c72556`. Noyau affiché **v0.20.0**. | Le [manifeste noyau](../kernel/Cargo.toml) alimente la [bannière](../kernel/src/main.rs). Les quatre séries du rejeu contiennent cette version. | VALIDÉ |
| LIV-02 | Intégration · identifier et vérifier le SDK distribué | **nova-system 0.2.0** pour AArch64 et x86_64, 21 237 octets, 23 fichiers. | SHA-256 de [l’archive d’origine](../../distribution/sdk/nova-system-0.2.0.tar.gz) `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. Relecture des 23 fichiers et comparaison individuelle à `provenance.json` de l’archive ELF. [Manifeste SDK](../../distribution/manifests/novaos-0.20.0-sdk.json) | VALIDÉ |
| LIV-03 | Trois phases · identifier le moteur et le format | **NovaFS 0.9.13**, format des installations **P9**, prise en charge historique de P8. | [Manifeste NovaFS](../../novaFS/Cargo.toml). Arbre `novaFS/` identique entre le tag v0.21.0 et la fusion sur les deux architectures. Autorisation P9 et compatibilité P8 sont détaillées dans [la qualification de phase 3](capacites-services-v0.22.0.md). | VALIDÉ |
| LIV-04 | Intégration · identifier le dernier support UTM et sa qualification | Installateur AArch64 **v0.17.0**, 3 881 952 octets, 27 fichiers vérifiés dans le manifeste interne. | SHA-256 de [l’archive UTM](../../distribution/images/novaos-0.17.0-utm-aarch64.tar.gz) `5930b0d6cb3d1e2307f8f630fefa52af8e76967f4f13a4c34674be8b94205f9c`. Les essais en application UTM portent sur cette ancienne version. Heure de construction exacte inconnue. [Bordereau des binaires](validation/fusion-smp-v0.17.0/distribution.json) | VALIDÉ AVEC LIMITES |

Le SDK distribué a été introduit par `0d5808c80bdc948f298d3a8e308954b110fbab3f` le 15 novembre 2026. Le manifeste de livraison du SDK rattache la qualification à `b46ddf1902678265b45642a0c2c90a696b21f53f` et à son intégration `7114badafa50a30d43e92174d03e2fe997285b73`. Les fichiers suivis de `distribution/` et du SDK sont identiques entre le tag v0.21.0 et le commit de fusion.

L’archive UTM a été générée le **14 novembre 2026** avec les binaires de `b76c863c9302bf6ead53fac3b80b26df0513fe0d`. Sa première inclusion dans Git est `051e0ae95881f0efd4b771116223192759b6680c`, à 17 h 38 min 58 s, heure de Paris. Les horodatages du tar sont normalisés. Le SHA-256 du support interne `installateur.qcow2` relu est `72cc18abcb27a794c54ede117f59293f43003b2bdeef79412d5f2cd365364efd`. [Rapport de distribution](validation/fusion-smp-v0.17.0/distribution.json) · [Campagne SMP et installation UTM](fusion-smp-v0.17.0.md)

Le procès-verbal historique couvre l’installation puis deux démarrages autonomes, la relecture du fichier et l’exécution de scripts dans UTM 5.0.8 AArch64, à un vCPU, TPM CRB, GICv3 sans ITS, VirtIO QCOW2 avec secteurs de 512 octets. Le laboratoire utilise le terminal série de l’application. L’image comprend NovaFS P9, `nsh` et `vi`. La supervision des services v0.21.0, la configuration persistante des services, le journal 16 et les restrictions configurables v0.22.0 sont absents de ce support. Le SDK v0.2.0 est disponible séparément. L’interface graphique UTM, le matériel serveur et les coupures d’alimentation physiques pour les fonctions v0.22.0 restent à qualifier. [Procès-verbal d’installation](validation/fusion-smp-v0.17.0/installation-utm.json) · [Distribution publique du dépôt](../../distribution/README.md)
