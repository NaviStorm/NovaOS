# v0.16.6 — Négociation des interfaces de stockage et de vi

## Contrats du candidat

Le commit `826496f78b145a27729ae6d6e5c75e972b82ef68` introduit la découverte sur la branche de prototype. Le profil brut dispose de la voie PAGE. La dernière fixture des profils groupés ajoute BATCH et le plan de sauvegarde. La voie commune à toutes les versions utilise les fragments privés de l’éditeur, les appels fichiers publiés et les transferts sectoriels du bail. [Correctif du profil groupé](../tests/fixtures/io-0166-negociation.patch) · [Audit antérieur](es-v0.16.6-integration-interfaces.md)

| Frontière | Requête et réponse | Conditions et repli |
|---|---|---|
| Client IPC → noyau | `IPC_FEATURES` (`0x1003b`), arguments handle local, une direction SEND ou RECEIVE, version 1. Bit 0 de la réponse pour la boîte PAGE. | Le noyau vérifie le droit demandé, l’identité du processus, le numéro du handle et la génération du nœud. Un endpoint vivant sans boîte correspondante renvoie zéro. Une porte inconnue du noyau publié donne le chemin historique. Un droit refusé se propage. |
| NovaFS → relais → pilote → portail du noyau | Trame bloc v1, opération 7, marque `NVBQ` à +32 puis `NVBA` en réponse, version u32=1 à +36, masque u32 à +40. Les autres octets réservés valent zéro. PAGE=1, BATCH=2. | Le relais limite le masque à son implémentation. Le pilote le limite aux voies provisionnées. Le portail vérifie PID, bail, droit INFO, état du support et phase au repos. NovaFS conserve l’intersection, vérifiée sur toute la réponse. Un ancien relais ou pilote refuse l’opération inconnue par une trame d’un octet BAD_ARGUMENT. Le mode sectoriel est choisi avant tout transfert. |
| Client → NovaFS | La demande MOUNT_PROGRESS conserve son octet d’option et son indicateur explicite READ_PAGE à +16. La découverte du plan utilise la trame fichiers v2, opération 31, session à +8, marque `NVFQ` à +24 (`NVFA` en réponse), version 1 à +28, masque u64 à +32. | Le service vérifie la session montée et la forme entière. PLAN_V1=1 est disponible dans le profil 42 avec BATCH négocié, un volume P9 monté en écriture et les droits d’ajout et d’écriture. Un service publié répond UNSUPPORTED. Un masque nul conserve les écritures par la candidate historique. |
| `vi` → `nsh` | INFO conserve exactement ses dix octets de réponse. Nouvelle opération 15, charge `NVVQ`, version 1, masque sur un octet. Réponse de succès de sept octets avec statut zéro, `NVVA`, version et masque accepté. | Les bits 1, 2 et 4 désignent respectivement TEXT_PAGE, le plan avec source paginée et le plan sans source paginée. Les deux types de plans sont exclusifs. TEXT_PAGE et PLAN sont indépendants, un plan transporte ses données dans son propre message ou par prêt immuable. Un ancien courtier répond BAD_ARGUMENT sur un octet. Le nouveau courtier n’envoie la réponse enrichie qu’à une demande de découverte et garde le masque pour ce dialogue. |

Tous les entiers multioctets sont en little-endian. Les consommateurs valident les bornes, la marque et la version des réponses enrichies. Les deux fonctions vi qui transfèrent des pages utilisent une boîte explicitement reliée à la session. La réussite de la découverte précède la sélection et la confirmation du nom. Chaque opération continue de vérifier les capacités et l’identité de sa source. Une erreur après le début d’une publication est propagée, avec le résultat incertain prévu en cas de perte de réponse. [Contrat IPC de la boîte](../abi/bulk_call.rs) · [Bail matériel](../kernel/src/hardware/block/lease.rs) · [Transport sectoriel et PAGE](../user/storage/medium.rs)

## Origine des quatorze refus de l’audit

- `vi` publié attendait dix octets pour INFO. Le courtier expérimental en retournait onze à tous ses clients en mode 42. Le même INFO conserve désormais sa forme historique.
- Le client expérimental tentait un dépôt PAGE avant de savoir si le courtier acceptait TEXT_PAGE. Seul DENIED déclenchait le repli. Le choix entre page et fragments précède désormais le dépôt.
- Le courtier interprétait les 73 octets de LOOKUP_AT comme preuve du plan. Cette longueur décrivait déjà les dates d’un fichier P9 publié. Le courtier interroge maintenant directement la session NovaFS.
- Pour monter la chaîne, la présence du handle local 10 déclenchait PAGE même si le relais ne connaissait que les secteurs. La découverte traverse désormais toute la chaîne avant la première lecture. En présence d’un composant historique, NovaFS découpe chaque page en secteurs selon la géométrie observée et poursuit le montage.

Le premier empilement des changements IPC et DMA construits à `47b7dc5` échouait à la compilation : un hunk de l’ancienne fixture de découverte déplaçait `features` selon un numéro de ligne qui, après l’ajout du contrôle DMA, désignait désormais l’intérieur d’une autre fonction. Le correctif `245f1a6` place sa seule implémentation dans le module brut, devant `transfer`. La fixture de négociation ne déplace plus cette fonction. La vérification a inclus sa portée après application des dix profils de qualification et du profil IRQ expérimental, ainsi que la compilation des invités réguliers des deux architectures et des profils sondés AArch64.

## Méthode de qualification

Le banc relie les produits binaires de la version publiée `3193bc9108bdc771f4627f57c84e093375cc9ea4` et du candidat identifié pour chaque campagne. Chaque croisement vérifie les empreintes des cinq programmes avant de démarrer l’invité QEMU direct 11.1.3, en TCG, un vCPU et 256 Mio. Les secteurs RAW sont de 512 octets en AArch64 et 4 096 octets en x86_64. Dans chaque cas, le premier invité ouvre la source UTF-8 avec CRLF et sans LF final, ajoute `Z`, confirme `OUI` pour le nom libre `copieaudit`, ferme l’éditeur et quitte le shell. Un deuxième invité remonte le même disque et relit la destination avec `cat`. Le lecteur P9 Darwin indépendant vérifie ensuite les générations et compare les contenus complets de la source et de la destination. Dans la campagne finale, le banc a recopié depuis l’archive du premier audit les cinq programmes publiés et le noyau publié. Le contrôle de chaque composition vérifie donc aussi les composants historiques par rapport à leurs SHA-256 de cet audit. [Lanceur](../tests/integration_interfaces.py)

## Résultats avant et après correction

Les résultats de cette section proviennent des noyaux combinant les corrections IPC et DMA au commit `13df1dce48677f6fd92256b99b0ab0948bde803a`. Une première campagne au seul commit IPC a également validé les 58 scénarios, y compris leurs remontages. Les deux candidats cumulés antérieurs, `af0fd10565636038d530066b450aadf7eca46a08` puis `245f1a66ff430b623bb87c102119c91338ee96d9`, ont chacun réussi les 68 essais. Chaque campagne dispose de ses empreintes.

**Les 32 essais de la matrice initiale ont réussi, contre 18 avant correction.** Les 24 essais supplémentaires et les deux témoins de la branche brute ont aussi réussi. Au total, 58 sauvegardes suivies de 58 redémarrages ont donné une destination lisible dans l’invité et conforme octet par octet dans le lecteur indépendant. [Observations individuelles et SHA-256](validation/es-v0.16.6-negociation.json)

Dans la matrice ci-dessous, chaque profil représente un essai par architecture. Les résultats ont été identiques sur les deux. Les composants anciens sont ceux de la version publiée.

| Composition | Profil 41 avant | Profil 42 avant | Profil 41 après | Profil 42 après |
|---|---|---|---|---|
| Témoin publié | Conforme | Conforme | Conforme | Conforme |
| Témoin du prototype | Conforme | Conforme | Conforme | Conforme |
| Ancien vi | Conforme | Ouverture refusée | Conforme | Conforme |
| Ancien nsh | Conforme | Sauvegarde refusée | Conforme | Conforme |
| Anciens nsh et vi | Conforme | Conforme | Conforme | Conforme |
| Ancien service NovaFS | Conforme | Sauvegarde refusée | Conforme | Conforme |
| Anciens relais et pilote | Montage refusé | Montage refusé | Conforme | Conforme |
| Nouveau vi sur le système publié | Sauvegarde refusée | Sauvegarde refusée | Conforme | Conforme |

Avec l’ancien relais seul, l’ancien pilote seul, le nouveau service NovaFS seul sur le noyau publié, le nouveau relais seul sur ce noyau, le nouveau pilote seul et les cinq nouveaux programmes sur ce noyau, les deux profils ont chaque fois réussi sur les deux architectures. Le témoin brut sans fixture a également réussi en profil 42 dans les deux géométries. Les six combinaisons croisées supplémentaires et les deux témoins sont identifiés séparément dans le JSON.

Les 58 sources relues ont pour SHA-256 `b18a4934f93d5d174256429e4af544d0b14f698dd5ca1f670e5230f4d556707b`, les 58 destinations `71903a8d71fb8c977381a94dc28093e74a4ac8c3dae5de5a3092fbd38ba53651`. Les deux générations inspectées après le remontage étaient successives dans chaque image. Le lecteur indépendant, l’ELF noyau, les cinq programmes et les deux traces série sont identifiés individuellement. Les temps d’exécution de cette campagne fonctionnelle ne constituent pas une mesure du débit.

Chaque invité lance tous ses programmes depuis le même catalogue immuable. Le remplacement à chaud de processus, la perte physique d’alimentation et la récupération F1/F2 relèvent d’autres qualifications. Sur ces deux géométries, toutes les compositions testées disposent d’une interface commune. La décision d’intégration reste également conditionnée par le cycle DMA, la persistance, les régressions et les mesures sous UTM.

Sur la géométrie inverse, le banc a aussi vérifié cinq frontières de montage en profil 42 dans chaque invité (AArch64/4 096 et x86_64/512). Les dix sauvegardes, leurs dix remontages et les comparaisons indépendantes ont réussi avec l’ancien relais seul, l’ancien pilote seul, les deux anciens composants bloc, l’ancien service NovaFS et les cinq nouveaux programmes exécutés sur l’ancien noyau. Le total du candidat combiné atteint 68 sauvegardes et 68 redémarrages. [Résultats des secteurs inversés](validation/es-v0.16.6-negociation.json)

## Reproduction

Préparer trois exports distincts. `PUBLIE` contient la révision publiée. `BRUT` contient la correction IPC sans fixture. `PROTOTYPE` utilise la même correction et reçoit successivement `io-0166-prototype`, `io-0166-plages`, `io-0166-virtio-multipage`, `io-0166-invite` et `io-0166-negociation`. Appliquer chaque fichier avec `patch -p1 -F0`. Installer le lecteur Darwin indépendant dans chaque export sous `novaFS/target/aarch64-apple-darwin/release/novafs` et créer les répertoires `novaOS/build` avant le lancement. Pour reproduire les empreintes historiques à l’identique, utiliser les artefacts de l’audit initial dans l’export publié après compilation. Les SHA individuels identifient ces fichiers, même si une nouvelle édition de liens de NovaFS produit une disposition différente du code.

```sh
for arch in aarch64 x86_64; do
    for export_git in "$PUBLIE" "$PROTOTYPE" "$BRUT"; do
        make -C "$export_git/novaOS" ARCH="$arch" kernel
    done
    python3 -B novaOS/tests/integration_interfaces.py "$PUBLIE" "$PROTOTYPE" "$SORTIE" "$arch" --etendue --remontage
    python3 -B novaOS/tests/integration_interfaces.py "$PUBLIE" "$PROTOTYPE" "$SORTIE" "$arch" --branche-brute "$BRUT" --remontage
    secteur=512
    if [ "$arch" = aarch64 ]; then secteur=4096; fi
    python3 -B novaOS/tests/integration_interfaces.py "$PUBLIE" "$PROTOTYPE" "$SORTIE-inverse" "$arch" --frontieres --secteur "$secteur" --remontage
done
```
