# ABI et livrables installables au 15 novembre 2026

## Registre natif v1

Sur le commit de fusion `4569a6876720f6b4efa901db707b670e415abeeb`, les commandes suivantes ont réussi depuis la racine du dépôt. Le contrôle de régénération a terminé avec le statut 0. Les neuf groupes de tests ont réussi.

```sh
python3 -B novaOS/scripts/generate_abi.py --check
python3 -B novaOS/tests/abi_registre.py -v
```

| Critère | Code, test et périmètre vérifié | Résultat |
|---|---|---|
| A-01 Numéros et contrats historiques | Comparaison des entrées du [registre](../../abi/syscalls.toml) à celles de `80a3699c76af0db9ccdc1a85964fe15576c32439`. Les 47 définitions sont identiques. La seule entrée supplémentaire est `cache::CONTROL` à `0x1007c`. Les tests `invalides`, `signatures_historiques`, `extension_services` couvrent unicité, réservations, signatures et numéros. | Validé |
| A-02 Portes, dispatch et source de vérité | Le test `portes_et_dispatch` compare les 48 numéros aux branches effectivement traitées dans le [noyau](../../kernel/src/sched/process/syscall.rs) et aux conventions des deux ISA. Le test `selecteurs_litteraux` recherche les constantes recopiées dans les sources Rust et assembleur suivies, avec exceptions identifiées pour les sondes. | Validé |
| A-03 Génération et clients | Le [générateur](../../scripts/generate_abi.py) produit les mêmes octets lors de deux générations. `exports_tmp` compile les assertions des 48 symboles Rust pour les deux cibles bare metal. `entetes_c_cpp` fait vérifier les déclarations et les assertions C11 et C++11 par Clang sur ces deux cibles. `statut_tmp_call` vérifie le contrat filaire des résultats et erreurs. | Validé |

Les trois lignes reposent sur la même [suite de contrôle](../../tests/abi_registre.py), SHA-256 `bf4a81f7bb95e6ad31eeef22dee3a03d25536b52631e11ee832aa0ff886c0d16`. Le registre a pour SHA-256 `b4d4f183fd64c8c2868891d55f83db4f4bcd9f95f053d91c2915d1f13c543ce8`, et [l’en-tête généré](../../abi/generated/nova_abi.h) `16123e67faa06eab1fd0f498878fa4eda68777d9d5f31011488f7465dd1cf1a3`. Les essais antérieurs sur le même arbre fonctionnel figurent dans [les résultats de phase 4](../validation/cache-phase4-v0.21.0.json). Les quatre exécutions d’ELF historiques et la distribution du SDK ont chacune leur entrée dans [la matrice binaire](services-cache.md).

## Sources, noyau, SDK et support installable

| Objet | Version ou référence vérifiée | Preuve et portée |
|---|---|---|
| Sources des quatre phases du chantier v0.21.0 | Candidat `4a3d3079bad8a8ef074601df5afdf9c6d5faacfb`, intégré par `4569a6876720f6b4efa901db707b670e415abeeb` | Arbre commun `ea3fab4c63f669b3325ecf3c7af0aa5e21f68a7b`. |
| Noyau issu des sources | **0.20.0** | Le [manifeste](../../kernel/Cargo.toml) fixe la version et la [bannière](../../kernel/src/main.rs) l’obtient de Cargo. Les deux séries archivées `certifie/cache-{aarch64,x86_64}-1-512/00-cache-test.serial` affichent effectivement `NovaOS v0.20.0`. |
| SDK Rust distribué séparément | **nova-system 0.2.0**, 21 237 octets | [Manifeste de livraison](../../../distribution/manifests/novaos-0.20.0-sdk.json). SHA-256 de [l’archive](../../../distribution/sdk/nova-system-0.2.0.tar.gz) `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. |
| Moteur et disque | **NovaFS v0.9.13 P9** | P9 pour les nouveaux espaces privés, compatibilité de lecture historique P8 et refus du lanceur privé sur P8. [Contrat temporaire](../contrat-tmp-prive-v0.21.0.md) |
| Installateur UTM AArch64 distribué | **NovaOS v0.17.0**, génération du **14 novembre 2026**, binaires de `b76c863c9302bf6ead53fac3b80b26df0513fe0d` | [Procès-verbal de l’installation](../validation/fusion-smp-v0.17.0/installation-utm.json), champ `binaires_git`. Première inclusion de ces octets dans Git par `051e0ae95881f0efd4b771116223192759b6680c` le 14 novembre à 17 h 38 min 58 s, heure de Paris. Les dates internes du tar sont ramenées à zéro. L’heure exacte de construction n’est pas enregistrée. [Campagne datée](../fusion-smp-v0.17.0.md) |
| Intégrité du paquet UTM | **3 881 952 octets**, SHA-256 `5930b0d6cb3d1e2307f8f630fefa52af8e76967f4f13a4c34674be8b94205f9c` | Relecture de [l’archive distribuée](../../../distribution/images/novaos-0.17.0-utm-aarch64.tar.gz), des 27 fichiers désignés dans son `SHA256SUMS` et comparaison au [relevé de livraison](../validation/fusion-smp-v0.17.0/distribution.json). Le support `installateur.qcow2` relu a le SHA-256 `72cc18abcb27a794c54ede117f59293f43003b2bdeef79412d5f2cd365364efd`. |

Le rapport d’installation (SHA-256 `16aa18a251bee1d458c225ac1bb4c857544355202b71ea8ba28ee67431ccd4fd`) documente UTM 5.0.8 AArch64, un vCPU, TPM CRB, GICv3 sans ITS, VirtIO QCOW2/512, EDK2 expérimental, installation puis deux démarrages autonomes et relecture du contenu. Le [bordereau de distribution](../validation/fusion-smp-v0.17.0/distribution.json) (SHA-256 `641e2a1503d454916065ceae44470119223c65d1088ac0f99ea70dd037efea8d`) référence séparément les ELF système et installateur. L’audit de clôture a contrôlé l’archive en lecture seule. Les essais de démarrage cités datent de sa qualification v0.17.0.

## Ce que l’utilisateur peut réellement exercer

| Fonction ou exigence | Sources intégrées | Installateur UTM distribué v0.17.0 | Qualification ou condition restante |
|---|---|---|---|
| Installation AArch64, démarrage autonome, NovaFS P9, commandes et scripts nsh, vi | Présents | Présents. Le support utilise le chemin d’écriture classique et le polling VirtIO-blk. | Installation et persistance observées dans UTM pour cette image. |
| Identité durable d’instance, BootID et travailleurs v0.21.0 | Qualifiés dans les profils documentés | Absents | Unicité conditionnée à une source de démarrage qualifiée. |
| Temporaire vide par instance, délégation et récupération P9 | Qualifiés | Absents | Sur P8, le lanceur privé refuse le format. |
| Supervision par le gestionnaire utilisateur, dépendances, READY authentifié et reprise | Qualifiées sur services désignés | Absentes | Remplacement à chaud du gestionnaire non qualifié. |
| Cache P9 d’identités provisionnées et attestées au banc | Isolation, partage autorisé, persistance, quotas, purge et récupération exercés | Absent | Le banc fournit les identités utilisateur et d’installation. |
| Application ordinaire dépourvue d’attestation utilisateur | Lancement réussi, écriture dans son tmp réussie et ouverture du cache refusée avec `DENIED` | Ce contrat est absent | Cache de vrais comptes à qualifier avec la future autorité. |
| ABI v1 à 48 sélecteurs et SDK Rust 0.2.0 | Interfaces qualifiées, SDK distribué séparément | Les nouveaux sélecteurs et le SDK v0.20.0 ne font pas partie de ce support | Compatibilité des ELF historiques documentée sur les noyaux de sonde du candidat. |
| Épreuve UTM graphique des quatre phases, matériel de production, perte physique d’alimentation, rollback du disque entier | Hors des essais QEMU | La campagne historique UTM ne couvre pas ces fonctions v0.21.0 | Qualification matérielle à organiser si elle devient exigible. |
| Comptes, ACL, permissions et autres chantiers de la stable serveur | Voir la [matrice d’attribution](perimetre.md) | Aucune livraison correspondante sur cette image | Décisions de feuille de route et maturité détaillées dans l’annexe. |

Le [catalogue de distribution](../../../distribution/README.md) permet de récupérer le paquet UTM et le SDK. Aucune image installable v0.21.0 n’y figure au commit examiné.
