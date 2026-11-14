# Préparation de l’intégration v0.16.6 — ABI, IPC et coexistence des services

**La mise à jour indépendante de `vi`, de `nsh` et des services de stockage échoue dans plusieurs compositions.** Sur 32 démarrages, 18 sauvegardes ont produit exactement les octets attendus. Deux ouvertures, huit sauvegardes et quatre montages ont été refusés. Les 32 images ont passé la vérification P9. Les 14 refus ont conservé la source et laissé la destination absente. [Résultats et empreintes des binaires](validation/es-v0.16.6-integration-interfaces.json)

## Références et méthode

Le 14 novembre 2026, le banc a compilé dans deux exports neufs la version publiée sur `main` au commit `3193bc9108bdc771f4627f57c84e093375cc9ea4` et le prototype sur `codex/es-groupees-0166` au commit `d6a7a67a1e5addbb1fe849ee24fa4d61fc9accdd`. La composition fonctionnelle choisie pour le prototype applique, dans l'ordre, `io-0166-prototype`, `io-0166-plages`, `io-0166-virtio-multipage` et `io-0166-invite`, avec polling. Les deux compositions ont été compilées pour AArch64 et x86_64. Le banc a aussi vérifié l'application des fixtures de neuf profils sur neuf autres extractions vierges de la référence. Les 77 applications par `patch -F0` sont passées sans rejet ni décalage. Les sept profils de diagnostic ou d'injection supplémentaires et le témoin IRQ ont uniquement fait l'objet d'un contrôle d'application dans ce volet. [Ordre et SHA-256 par profil](validation/es-v0.16.6-integration-interfaces.json) · [Protocole DMA](es-v0.16.6-dma.md#reproduction)

Pour chaque croisement, le banc conserve le noyau et les services de la composition de base, remplace le ou les programmes désignés par leurs produits binaires exacts et relie le noyau qui les embarque. Il vérifie le SHA-256 de chaque programme avant le démarrage. L'image jetable contient le texte `premier\r\nété\nfin`. L'éditeur doit ajouter `Z` en tête et publier vers le nouveau nom `copieaudit` avec confirmation. Après chaque session, le lecteur indépendant vérifie le volume, les deux générations et les octets de la source. Il vérifie aussi tous les octets de la destination en cas de succès ou son absence après refus. L'ouverture d'un fichier peut mettre à jour atime. Le contrôle porte sur les données et la cohérence des générations. [Banc de coexistence](../tests/integration_interfaces.py) · [Création de l'image témoin](../tests/vi.py)

## Compositions exécutées

Chaque cellule ci-dessous correspond à deux invités QEMU 11.1.3 TCG en RAW, AArch64 avec secteurs de 512 octets et x86_64 avec secteurs de 4 096 octets. Les deux architectures donnent le même résultat. Chaque invité a 256 Mio et un vCPU. Le profil 41 utilise la CLI ordinaire. Le profil 42 active les voies de laboratoire dans les binaires qui les connaissent. La matrice croise les versions effectivement chargées et les empreintes sont consignées par essai. [Les 32 observations](validation/es-v0.16.6-integration-interfaces.json)

| Noyau et services de base | Programmes remplacés | Profil 41 | Profil 42 |
|---|---|---|---|
| Publiés | Aucun | Sauvegarde conforme | Sauvegarde conforme |
| Prototype | Aucun | Sauvegarde conforme | Sauvegarde conforme |
| Prototype | `vi` publié | Sauvegarde conforme | Ouverture refusée avant affichage |
| Prototype | `nsh` publié | Sauvegarde conforme | Sauvegarde refusée avant confirmation |
| Prototype | `nsh` et `vi` publiés | Sauvegarde conforme | Sauvegarde conforme |
| Prototype | Service NovaFS publié | Sauvegarde conforme | Sauvegarde refusée après confirmation |
| Prototype | Relais et pilote bloc publiés | Montage refusé avant l'invite | Montage refusé avant l'invite |
| Publiés | `vi` du prototype | Sauvegarde refusée avant confirmation | Sauvegarde refusée avant confirmation |

### Deux témoins supplémentaires sur la branche brute

Un troisième export de `d6a7a67a1e5addbb1fe849ee24fa4d61fc9accdd` a fourni tous les binaires invités. Les 843 fichiers ordinaires extraits correspondent octet par octet à l’archive Git, de SHA-256 `166d5c3b6051941133ea60cb01948bfe7d705054d3f56c7c01656af0441332cc`. La compilation a utilisé directement ces sources. Ces deux témoins du profil 42 évaluent le chemin ordinaire présent sur la branche en l’absence des quatre correctifs de laboratoire.

| Invité QEMU direct / RAW | SHA-256 du noyau exécuté | Montage et sauvegarde vers `copieaudit` | Générations P9 relues |
|---|---|---|---|
| AArch64, 512 octets | `4597b179a78e9324f1d0f168c956056fa1bfd31d74beed785809345ebacd6892` | Invite `nsh`, confirmation `OUI`, acquittement | 75 et 76 |
| x86_64, 4 096 octets | `960e96a0b20a173fbde95e1e82ab255057f7111c6a09706372372fcc7ed7a7b3` | Invite `nsh`, confirmation `OUI`, acquittement | 75 et 76 |

Dans chaque invité, les quatre processus ont retourné zéro et le retrait du bail a réussi. Après fermeture de la VM, le lecteur indépendant a vérifié le volume, puis comparé tous les octets de la source et de la destination dans la génération 76. SHA-256 de la source `b18a4934f93d5d174256429e4af544d0b14f698dd5ca1f670e5230f4d556707b`, de la copie avec son préfixe `Z` `71903a8d71fb8c977381a94dc28093e74a4ac8c3dae5de5a3092fbd38ba53651`. Les deux sessions figurent sous `temoins_branche_brute` dans les résultats. [Empreintes du lecteur, des programmes et des traces série](validation/es-v0.16.6-integration-interfaces.json)

### Origine des refus

1. **Réponse INFO de `nsh`.** Le client publié exige une réponse de dix octets au total pour `INFO`. En profil P9 de qualification, le nouveau courtier renvoie onze octets au même appel sans connaître la version de son interlocuteur. Le client sort avec « argument invalide » avant le premier rendu. L'ancien couple `vi`/`nsh` devant les nouveaux services a réussi dans les quatre sessions. [Client publié](../../components/vi/main.rs) · [Extension du dialogue](../tests/fixtures/io-0166-plages.patch)
2. **Transfert de texte avec un ancien pair.** Le client récent accepte l'ancien `INFO` puis tente `IPC_PUT` suivi de `TEXT_PAGE` pour sauvegarder. Sur un noyau publié, `IPC_PUT` renvoie `UNKNOWN_CALL`. Sur un noyau de prototype en mode 42, la boîte du vi existe et l'ancien `nsh` refuse `TEXT_PAGE` avec `BAD_ARGUMENT`. Le repli du client traite uniquement `DENIED`. En mode 41 du noyau de prototype, l’absence de voie d'éditeur produit justement `DENIED` et la paire retrouve les fragments historiques. [Repli du client](../tests/fixtures/io-0166-prototype.patch) · [Courtier publié](../../components/shell/vi.rs) · [Dispatch des appels natifs](../kernel/src/sched/process/syscall.rs)
3. **Détection du service NovaFS.** Le courtier déduit la disponibilité du plan de la longueur 73 de `LOOKUP_AT /` en mode 42. Cette longueur transporte déjà les dates P9 du service publié. Pendant le croisement, le nouveau courtier a demandé « OUI » puis envoyé l'opération 30. Le service publié a suivi le traitement ordinaire du handle et le client a affiché « capacité révoquée ». La source est restée lisible et la destination absente dans les deux invités. La longueur de `LOOKUP_AT` ne permet donc pas de négocier cette opération. [Détection et opération 30](../tests/fixtures/io-0166-plages.patch) · [Réponse de stat P9](../user/novafs/files/namespace.rs) · [Traitement historique du handle](../user/novafs/files/selected.rs)
4. **Connexion au service bloc.** Le noyau du prototype crée une voie de copies à l'index de configuration 10 dans les deux profils. Le nouveau service emploie alors les trames de phase `PAGE` pour le montage. Le relais bloc publié rejette cette phase à la vérification de syntaxe. Le client de montage s'arrête avec le statut 26, les autres services ferment leurs endpoints et le bail est retiré. Ce croisement remplace à la fois le relais et le pilote. La première incompatibilité sur le chemin est celle du relais. [Sélection de la voie bloc](../user/novafs/device.rs) · [Validation du relais](../user/storage/relay.rs) · [Trames bloc](../abi/block.rs)

## Contrats d'interface et bornes

- **ABI native.** Les 152 constantes entières à valeur littérale présentes dans les fichiers Rust de `novaOS/abi` de la version publiée conservent toutes leur valeur. Cette comparaison exclut les constantes calculées. Les numéros de version restent à 1 pour les appels natifs et à 2 pour les messages fichiers. Les structures IPC envoyée, reçue et donnée gardent respectivement 160, 184 et 48 octets, avec un alignement de huit. Les tests utilisent ces tailles sur les deux invités. [ABI native](../abi/native.rs) · [Trames fichiers](../abi/files.rs)
- **Appels et opérations.** L'étape intermédiaire apporte `IPC_PUT/TAKE` aux numéros `0x1002c/2d`, le portail page `0x10046` et `READ_PAGE` 28. Les fixtures affectent `0x1002e..0x10032` à la réservation et au traitement des lots, `0x10034..0x10039` au prêt, et `0x1003a` à la chaîne DMA. Le dispatch conserve le trou `0x10033`. Aucun de ces numéros ne recouvre un appel natif publié. Les numéros 29 et 30 sont des opérations dans la trame des fichiers. `TEXT_PAGE` 13 et `PLAN` 14 se trouvent sur le canal privé d'édition. Les identifiants identiques dans plusieurs champs ou plusieurs protocoles n'ont pas le même espace de dispatch. [Lots et droits](../tests/fixtures/io-0166-prototype.patch) · [Prêt et plan](../tests/fixtures/io-0166-plages.patch) · [Portail DMA](../tests/fixtures/io-0166-virtio-multipage.patch)
- **Publication.** L'opération 30 nécessite la voie de laboratoire, une session inscriptible et une sélection `SELECT_NEW` pour un nom explicite. Une source référencée requiert `SELECT_FILE` et la concordance de son identité, de sa révision et de sa taille. Un plan compte de 8 octets à 8 Mio et au plus 2 049 enregistrements de 24 octets. Les plans jusqu'à 64 octets sont transportés en ligne. Chaque lecture d'un prêt par le moteur est limitée à 1 Mio et à la plage scellée. Le mécanisme conserve au plus un prêt actif dans le domaine IPC global du noyau actuel. Ces valeurs sont vérifiées dans le code, avec des essais sur les frontières et les droits détaillés ci-dessous. [Validation dans le moteur et le noyau](../tests/fixtures/io-0166-plages.patch)
- **File bloc.** La boîte historique porte au plus 4 Kio, avec huit boîtes superviseur au maximum. Une réservation de lot porte de 1 à 256 pages. Le résultat d'une chaîne DMA est borné une seconde fois par la file VirtIO et l'arène privée. Les capacités, les deux titulaires, le tag, la direction et les bornes déterminent l'autorité. Le profil 42 peut déléguer explicitement la voie à l'index 11. La conservation de ces contrôles est nécessaire pour tout mécanisme de découverte. [Transport de pages](../kernel/src/sched/process/ipc/bulk.rs) · [Lot multipage](../tests/fixtures/io-0166-virtio-multipage.patch)

Sur les deux noyaux complets du prototype, la campagne QEMU `ipc-test` a validé les contrats historiques de son mode et les trois groupes supplémentaires `lots`, `file-revocable` et `pret-multipage`. Les assertions couvrent les masques de droits, le tiers sans délégation, le tag périmé, la fermeture d'un handle, la longueur et l’alignement, le refus d'une lecture hors plage, l'annulation de prêt en concurrence avec la lecture et le refus de retrait définitif tant que la publication utilise le prêt. L'ancien handle garde son refus après nouvelle dérivation. Le profil atteint 71 pages IPC au point de saturation mesuré et retrouve zéro page IPC à la destruction comme à la reprise. Les huit tests de mémoire de vi ont également réussi sur Darwin ARM64 et x86_64 sous Rosetta, avec annulation, rétablissement, plages et échec d'allocation. Les mesures temporelles de cette campagne fonctionnelle ont été conservées comme traces secondaires. [Contrats et bornes testés](../tests/ipc.py) · [Assertions de la file](../tests/fixtures/io-0166-prototype.patch) · [Assertions du prêt et du tampon](../tests/fixtures/io-0166-plages.patch) · [Résultats par architecture](validation/es-v0.16.6-integration-interfaces.json)

## Décision à prendre avant intégration

Préserver la réponse `INFO` historique pour toute requête historique. Une découverte explicitement demandée par le client pourrait annoncer la version, les modes et les bornes négociés entre éditeur, courtier, service et pilotes. Une composition dépourvue d'une fonction pourrait utiliser le chemin historique dès l'ouverture, avant tout prêt, transfert ou publication. Une capacité resterait nécessaire pour exercer chaque opération découverte. La qualification devrait alors rejouer les mêmes binaires publiés et la même matrice. Les changements correspondants de protocole restent soumis à arbitrage. Les essais de ce volet couvrent QEMU direct et des disques RAW jetables. Le scénario utilise une édition courte avec destination libre. Il ne qualifie pas une mise à jour à chaud des processus, des lectures intégrales de volumes historiques arbitraires ni une perte d'alimentation physique.

## Reproduction

Créer deux exports jetables aux SHA indiqués, contenant `novaOS`, `novaFS` et `components`. Y appliquer les quatre fixtures fonctionnelles uniquement dans le répertoire destiné au prototype. Installer le lecteur Darwin indépendant en `novaFS/target/aarch64-apple-darwin/release/novafs` dans chacun, puis compiler les quatre noyaux avec `make -C novaOS ARCH=… kernel`. Les arguments `PUBLIE` et `PROTOTYPE` sont les chemins de ces exports. Le banc remplace des produits de compilation dans leurs répertoires `target` et rétablit les produits de référence après chaque composition. Employer un répertoire de résultats neuf. [Lanceur du croisement](../tests/integration_interfaces.py)

```sh
for arch in aarch64 x86_64; do
    python3 -B novaOS/tests/integration_interfaces.py "$PUBLIE" "$PROTOTYPE" "$SORTIE" "$arch"
    make -C "$PROTOTYPE/novaOS" ARCH="$arch" ipc-test vi-memoire-test
done
```

Pour les deux témoins supplémentaires, extraire un troisième export au SHA de la branche étudiée, installer le même lecteur indépendant et créer son répertoire `novaOS/build`. Utiliser son chemin dans `BRUT`.

```sh
for arch in aarch64 x86_64; do
    make -C "$BRUT/novaOS" ARCH="$arch" kernel
    python3 -B novaOS/tests/integration_interfaces.py "$PUBLIE" "$PROTOTYPE" "$SORTIE" "$arch" --branche-brute "$BRUT"
done
```
