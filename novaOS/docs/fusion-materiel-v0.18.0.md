# Qualification d’intégration de NovaOS v0.18.0

## Candidat matériel et périmètre

Le 15 novembre 2026, le point de départ de `main` était `010ae3998aa158811b8a05c6d47753cd4589ad9a` (arbre `fd2a6fdeab3da4b38437d29dfd715db3fd587c5f`). Ses huit descendants sur la branche matérielle aboutissent à `65037718d8f8c20fcaa256d06df4774e8bf7709a` (arbre `3e40f99e035ac3516fb892d6d937109d6ab6a2f4`). Ce dernier commit ajoute le rapport et les preuves au code qualifié `b140be8c83cb2ab49b05b1aeb3c26919fd67c81f`. Les différences entre ces deux commits concernent uniquement la documentation. [Qualification de phase 4](materiel-phase4-v0.18.0.md)

Le profil d’inventaire découvre ACPI et PCI dans les bornes autorisées. L’attribution d’une BAR et d’une IRQ au pilote utilisateur EDU exige le laboratoire matériel. Le remappeur Intel VT-d exige en outre le profil x86_64 Q35 spécifique et l’admission du groupe câblé dans ce profil. Un cadre dont le retrait reste incertain demeure épinglé. L’émulateur peut bloquer une écriture à chaud sans remplir FRCD ni émettre de MSI. Le noyau et le banc vérifient indépendamment les permissions et le contenu des pages. Le routage pair-à-pair général, le FLR individuel et Intel physique restent hors qualification. AMD-Vi et ARM SMMU restent à développer. [Admission PCI](materiel-phase4-pci.md) · [Fautes à chaud](materiel-phase4-fautes.md) · [Retrait et quarantaine](materiel-phase4-retrait.md)

## Preuves de qualification reprises

| Domaine | Vérification du 15 novembre |
|---|---|
| VT-d | L’oracle Python relit les dix séries, décompresse les traces QEMU, vérifie les SHA-256 et retrouve les résultats archivés. La réinitialisation du profil mono a sa propre série et sa propre trace. Les neuf VM négatives PCI/DMAR utilisent le même ELF que ces dix VM. |
| Mémoire | Relecture de 21 captures brutes de la racine, du contexte et de la file sur sept configurations. En quarantaine pour fin EDU incertaine, le parcours des quatre niveaux confirme les deux seules feuilles et les 8 192 octets des deux pages RO/WO. |
| MMIO/IRQ | L’oracle hôte retrouve les onze scénarios dans chacune des six captures AArch64/x86_64 à un, deux et quatre vCPU. Sont notamment couverts l’accès après retrait, la mort du titulaire, le masque IRQ et les deux ordres entre acquittement et révocation. |
| Modèles et SMP | Vérification des empreintes des preuves de phase 4. Les modèles totalisent 65/65 sur Darwin ARM64 et 65/65 sous Rosetta. Les journaux d’ordonnancement et de FPU/SIMD confirment les essais x86_64 à quatre vCPU. |

Les 19 VM Q35 et leur redémarrage supplémentaire appartiennent à la campagne du 14 novembre. Les régressions longues NovaFS, l’endurance IPC, les benchmarks historiques et l’interface graphique UTM n’ont pas été relancés pour cette fusion. [Archives VT-d](validation/materiel-phase4-vtd.json) · [Archives PCI](validation/materiel-phase4-pci.json) · [Régressions ciblées](validation/materiel-phase4-regressions.json)

## Identification des nouveaux binaires

Le commit `7b7374f807e775978eeefdeb0ee40fba0f741a2d` porte à 0.18.0 les versions Cargo de `novaos`, de `nova-cli` et de l’adaptateur utilisateur `novaos-storage`, ainsi que leurs trois verrous. Le moteur portable NovaFS reste à 0.9.13 P9. Ses sources et l’ABI native conservent respectivement les arbres Git `3379ccd023e821e912130c7413113fe7546070cc` et `a70d5f23ae91211dd7a54c429e723f5157600e58`. La dernière archive d’installation UTM porte sa version historique 0.17.0. [Distribution](../../distribution/README.md)

| Essai sur le candidat réidentifié | Résultat |
|---|---|
| Démarrage ordinaire QEMU, deux architectures | Bannière série v0.18.0 et cinq captures QMP conformes. Les cinq empreintes attendues ont été calculées avant l’exécution en remplaçant le glyphe 7 par le glyphe 8 sur des images v0.17.0 authentifiées. Les 256 pixels modifiés dans chaque scène appartiennent à ce seul glyphe. |
| CLI sur P9 jetable | Quatorze commandes par architecture répondent à `--version` avec le code zéro. L’empreinte intégrale de chaque volume est identique avant et après la session. |
| Stockage classique | Sur chaque architecture et chaque géométrie, 512 et 4 096 octets, WRITE puis FLUSH, arrêt, nouvelle VM en lecture et vérification de l’image entière. |
| VT-d du nouvel ELF x86_64 | Un seul profil standard à deux vCPU, dix scénarios, six fautes acquittées et 22 triplets d’invalidation confirmés. |

Les mesures et captures du candidat sont identifiées par le SHA-256 de l’ELF effectivement utilisé. [Résultats des binaires v0.18.0](validation/fusion-materiel-0180-candidat.json)

## Régression du constructeur expérimental et arbitrage

Le constructeur applique les correctifs avec `patch -p1 -F0 --batch --forward` sur un export Git neuf. Sur `010ae39`, le correctif commun s'appliquait. Sur `6503771`, trois fragments échouaient dès la première étape. La découverte EDU avait ajouté deux gardes dans le constructeur des ELF, la visibilité interne de `block::io` avait changé pour le laboratoire VT-d et le module privé `ipc::lab_command` précédait désormais l'export de `bulk::transfer`. Les dix profils groupés dépendaient de ce premier correctif. Le chemin ordinaire n'applique aucune de ces transformations.

La MOA/MOE a autorisé l'adaptation des correctifs et la requalification de l'ensemble des profils avant fusion. Le commit `082ff4a78cf8f20ca4da5b5174183d888fc845e8` ajuste les contextes de sept correctifs, dont deux diagnostics optionnels. Les lignes de code effectivement ajoutées ou supprimées par ces correctifs restent identiques à celles de v0.17.0. Les 779 comparaisons de fichiers transformés, sur dix profils et trois variantes supplémentaires de diagnostic, reproduisent la composition des changements matériels et du stockage expérimental. Les anciens résultats et sondes compressées restent attachés à leur révision. [Profils, compositions et empreintes des exécutables](validation/fusion-materiel-0180-stockage.json)

| Contrôle après correction | Résultat |
|---|---|
| Constructeur officiel | Dix profils reconstruits, dix provenances et 19 ELF invités vérifiés. Les trois variantes UTM construisent leur cible AArch64. Le profil de régression inclut deux ELF AArch64 supplémentaires. |
| Négociation et capacités | 5/5 tests de trames sur chaque architecture Darwin. Les deux invités du profil de défauts passent les 19 groupes IPC de l’oracle, dont les lots, le prêt multipage et la révocation. |
| F1, F2, contrôle des bornes, retraits et témoin positif | Six cellules sur AArch64/512 et six sur x86_64/4096, à un vCPU. Après F1, la destination est absente. Après F2, la destination est complète même si le client n’a pas reçu l’acquittement. Les deux retraits refusent la publication. Chaque cellule remonte normalement une copie et vérifie son contenu. Le contrôleur et le Mac sont alimentés pendant ces essais de coupure QEMU. |
| Création de 4 Mio sur le chemin groupé | Un témoin par architecture, deux vCPU, secteurs de 4 096, polling et deux FLUSH. Le contenu relu porte le SHA-256 `37e348633fb3b4cd2c6969a6047cce79bb5c5981cda9a2b8a8eb33bd5780b6ed`. Le banc observe 27 allers-retours IPC de sauvegarde par invité (36 avec les contrôles noyau locaux). Il mesure 129,238 ms sur AArch64 et 155,191 ms sur x86_64. Ces deux exécutions fonctionnelles sous QEMU TCG ne définissent pas une distribution de latence ou un résultat UTM. |

Le groupe EDU ne reçoit aucun droit sur le stockage. Les capacités de commandes du laboratoire restent directionnelles. Le chemin bloc classique et le polling restent les valeurs par défaut. La négociation de lots et l'IRQ VirtIO expérimentale conservent leurs conditions d'activation explicite. [Stockage classique et expérimental](integration-v0.16.6.md) · [Provenance et relectures de la requalification](validation/fusion-materiel-0180-stockage.json)

Depuis la racine, le constructeur reproduit les dix exports dans un répertoire neuf. Les journaux et les exécutables sont isolés par profil.

```sh
python3 -B novaOS/tests/reconstruire_es_0166.py \
  082ff4a78cf8f20ca4da5b5174183d888fc845e8 "$repertoire_neuf" \
  defauts sans sonde regression coupe course utm utm-sonde utm-coupe diagnostic-stockage --jobs 3
```
