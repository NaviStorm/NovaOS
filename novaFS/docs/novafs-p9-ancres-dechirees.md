# NovaFS v0.9.13 P9 : récupération après déchirure d’une ancre

**Décision de qualification : le montage automatique du volume P9 autonome ne peut pas être autorisé sous les garanties demandées.** Une génération dont tous les blocs passent les contrôles peut être antérieure au dernier remplacement confirmé. Deux historiques qui exigent des décisions différentes produisent exactement la même image. Le refus de montage reste une limite de disponibilité, pas un résultat acceptable pour livrer le remplacement.

Base : `03e267b9cf710d7d53612ba89064a9af156be054`, branche `main` propre au départ. Les injections s’exécutent sur des disques simulés jetables, avec le moteur réel. La primitive, la reconstruction et les adaptateurs de test sont absents du service NovaOS. [Cas de coupure et de reconstruction](../src/fs/remplacement/recuperation.rs) · [Essais sur le pool chiffré](../src/secure_pool/qualification_ancres.rs) · [Mesures de validation](validation/v0.9.13/ancres-dechirees.json)

## Ce qui est écrit et ce que le montage peut prouver

Les blocs logiques 0 et 1 du volume portent A et B, de 4 096 octets chacun. Le formatage place le même manifeste de génération 1 dans les deux références, avec une enveloppe différente par position. Ensuite, la génération paire remplace A et l’impaire B. À partir de la génération 2, ce ne sont plus deux copies du dernier état. [Encodeur et décodeur](../src/codec.rs) · [Contrat de publication](novafs-recovery.md#publication-dune-génération)

| Octets (fin exclue) | Contenu de l’ancre autonome |
| --- | --- |
| 0–16 | Signature `NOVAP0FS`, version/réservé à zéro, type d’ancre, taille d’en-tête 96. |
| 16–56 | Identifiants FS et LV de 16 octets chacun, puis numéro du bloc de l’ancre. |
| 56–96 | Génération sur 8 octets, puis SHA-256 sur 32 octets. |
| 96–136 | Taille de bloc 4 096, réserve à zéro, capacité en blocs à l’offset 104, puis réserve à zéro. |
| 136–184 | Référence au manifeste : adresse, génération de naissance, SHA-256. |
| 184–4 096 | Zéros obligatoires. |

Les entiers sont en little-endian. Il n’y a **ni CRC, ni MAC, ni compteur externe** dans cette enveloppe. Le SHA-256 couvre le préfixe de domaine `NovaFS.P0.meta\0` et les 4 096 octets, avec le champ d’empreinte remis à zéro pour le calcul. Les données ont un domaine distinct qui lie aussi les identifiants, l’adresse et la naissance. Ces contrôles détectent la corruption ; une empreinte sans clé stockée sur le même disque n’authentifie pas celui-ci contre une réécriture malveillante.

Le moteur écrit hors des deux graphes protégés, attend les écritures CoW (données, arbres, espace et manifeste), puis attend **F1**. Il écrit ensuite une seule ancre, attend sa complétion puis **F2** avant de rendre le succès. Après redémarrage, le montage normal RO comme RW charge **les deux** ancres ; il contrôle position, identités, géométrie, parité et empreintes. Hors du cas initial, le manifeste récent doit engager la génération **et le digest** du précédent. Le contrôle traverse les cinq arbres, les données, la propriété des blocs, les âges de retrait et les références partagées. Le flush d’activation RW vient après ces contrôles. [Moteur et sélection de la paire](../src/engine.rs) · [Contrôle hors ligne par slot](../src/engine/integrity.rs)

**Pourquoi 96 octets suffisent à bloquer.** Après F1, un préfixe nouveau de 96 octets met en place la génération et le checksum de l’ancre candidate. L’adresse, la naissance et l’empreinte du manifeste à partir de 136 appartiennent encore à l’ancienne occupation du slot, deux générations plus tôt. Le nouveau checksum ne couvre pas ce mélange. Le décodeur rejette l’enveloppe avant de suivre la référence ; le montage ne choisit donc aucun des deux slots. Les 96 octets sont un exemple reproductible et non un seuil universel.

## Campagne sur le volume autonome

Sur chaque architecture : déplacement et publication d’une copie préparée, slot A puis B, secteurs de 512 puis 4 096 octets. Pour chacune des huit combinaisons, **32 plages × 9 événements = 288 coupures**. Les neuf points sont la soumission, l’effet et la complétion de F1 ; avant l’écriture d’ancre, à sa soumission et à sa complétion ; puis la soumission, l’effet et la complétion de F2. La trace et le reçu de l’appel sans panne servent d’oracles au banc, jamais d’entrées au montage. Le témoin historique rejoue aussi tous les événements d’E/S et les masques de secteurs entiers.

| Octets neufs persistants pendant que l’ancre est volatile | Résultat vérifié sur la fixture |
| --- | --- |
| Préfixe de 1 ou 32 octets | Ces octets sont identiques dans les deux versions. Ancienne paire valide, ancienne destination. |
| Préfixe de 64, 96 ou 128 octets | Métadonnées hybrides. RO et RW refusés avant écriture ou flush ; autre génération intégralement vérifiée et extractible en RO. |
| Préfixe de 184, 256, 512, 1 024, 2 048 ou 4 096 octets | L’ancre est identique, sur **tous** ses octets, à l’ancre nouvelle. Le manifeste et ses dépendances sont stables depuis F1. La nouvelle destination est lisible même si le succès n’est pas parvenu au client. |
| Plages intérieures ou à cheval sur deux champs ou secteurs | La plage est injectée à sa position réelle. Le résultat attendu est calculé octet par octet à partir des deux images de référence : ancienne enveloppe, nouvelle enveloppe ou refus. La liste exacte figure dans `PLAGES` du banc. |
| Deux écritures complètes de dépendances avant F1, autres écritures volatiles perdues | Deux blocs distincts de 4 Kio persistent. Les ancres et tous les autres blocs restent identiques à l’image initiale ; seule l’ancienne génération monte. En géométrie 4 Kio, une commande d’ancre ne contient jamais plusieurs secteurs. |

| Par architecture | Ancien état au montage | Nouvel état au montage | Refus normal | Total des injections de plage |
| --- | ---: | ---: | ---: | ---: |
| Secteurs 512 | 644 | 328 | 180 | 1 152 |
| Secteurs 4 096 | 644 | 328 | 180 | 1 152 |

Les huit coupures supplémentaires à deux blocs sont hors de ces totaux. Chaque montage réussi contrôle reçu, contenu exact, identité et quatre dates de la destination, ainsi que le nom source et l’original de la copie. Les écritures de préparation n’ont touché **aucune** adresse appartenant à l’un des deux manifestes, à leurs arbres ou à leurs pages de données. Dans chaque refus, le rapport indépendant établit quel slot est réellement exploitable ; le mode de sauvetage refuse une création sans produire d’E/S de mutation. Aucun mélange de générations ni destination manquante n’a été accepté.

## Pourquoi on ne peut pas choisir automatiquement la tête survivante

1. **Coupure avant confirmation.** À partir d’un état confirmé `g`, préparer le remplacement et réussir F1. Pendant F2, ne stabiliser que le préfixe de 96 octets. Le dernier succès rendu est celui de `g`.
2. **Corruption après confirmation.** Partir de la même base et terminer le même remplacement à horloge fixée. Le montage indépendant vérifie `g+1` et la nouvelle destination. Après ce succès, injecter un défaut latent sur le slot publié : rétablir uniquement ses 48 anciens octets de référence au manifeste.

Les **2 097 152 octets**, dépendances CoW comprises, sont identiques dans les deux images. La preuve a été répétée pour les deux parités et géométries (reçus 6/7 et 7/8). Il n’est pas supposé qu’une coupure ordinaire annule un flush réussi : le second historique est précisément le cas de corruption que le montage ne doit pas masquer. Même un avertissement suivi d’un repli automatique ferait perdre l’état acquitté dans le second historique. À l’inverse, le manifeste candidat et ses dépendances sont également présents sur une troisième image coupée **avant toute écriture d’ancre**. Les promouvoir sur la foi de leur présence confondrait préparation et publication.

**Stratégie éprouvée sur copie.** Le contrôle vérifie entièrement le slot survivant, puis le compare au reçu supposé frais de l’observateur extérieur. Le manifeste survivant donne la naissance et le digest de son propre parent. Le banc recherche exclusivement ce parent, refuse son absence ou son ambiguïté, reconstruit l’enveloppe du parent sur une branche mémoire indépendante et exige un **montage normal RO** de la paire ainsi reconstituée. Avec le reçu `g`, le graphe, l’espace et l’ancienne destination sont vérifiés. Avec le reçu confirmé `g+1`, le repli vers `g` est refusé. Une altération supplémentaire d’une page de l’ancienne destination invalide aussi le secours, malgré son enveloppe intacte. Aucun octet du support source n’est réparé.

Reconstituer au contraire l’ancre **nouvelle** à partir du reçu candidat permet même de retrouver exactement le SHA-256 neuf conservé dans le préfixe de 96 octets. Sur copie, la paire ainsi reconstituée monte aussi bien sur l’image déchirée que sur celle dont l’écriture d’ancre n’avait jamais été soumise. C’est une preuve de cohérence du candidat, pas de publication complète. Une déchirure qui ne conserve qu’une partie du checksum n’offre même pas cette comparaison.

Le reçu de laboratoire est une référence imposée par le test, **pas un service de fraîcheur**. Le test n’authentifie pas l’émetteur d’un reçu réel et ne met en œuvre aucun protocole de coordination avec F2. Un reçu ancien, facultatif, ou conservé dans la même image ne suffit pas. Si un observateur fiable sait que la seule panne est une coupure confinée à l’écriture en cours, le slot intact conserve la génération précédente ; le disque seul n’atteste pas cette prémisse. Sans autorité indépendante, reconstruire une paire cohérente ne prouve pas qu’elle était la dernière. Le banc ne réactive ni l’écriture, ni la collecte, ni le scellement à partir d’une vue de sauvetage.

## Snapshots, clones et chiffrement

Le pool P9 authentifié publie un **catalogue externe au LV**. Quatre emplacements logiques de génération portent chacun deux attestations HMAC physiquement distinctes ; le catalogue et les unités scellées ont leurs propres vérifications. Ces attestations ne sont pas des copies cachées des ancres d’un volume P9 autonome. Le [service installé](../../novaOS/user/novafs/files.rs) monte le volume par `Engine::mount_current`. [Contrat du pool et limite de fraîcheur](novafs-p7-chiffrement.md#publication-perte-de-fraîcheur-et-traitement-des-erreurs)

Le banc a exécuté le même remplacement privé dans l’origine d’un pool P9 chiffré, configuré en LZ4. Un snapshot conserve l’ancienne destination ; un clone possède sa propre destination divergente. Six injections par géométrie couvrent F1, chacune des deux copies du certificat, et l’effet de F2. Une copie partielle de 96 octets seule n’autorise pas la nouvelle génération ; une copie authentique entière suffit dans ces essais. Chaque réouverture vérifie le graphe, les trois contenus et le parent du clone. Une clé administrative incorrecte, un domaine verrouillé et une mauvaise clé de récupération interdisent l’accès correspondant ; les inspections n’écrivent rien. La politique de domaines et de nonces n’a pas été modifiée ; les tests historiques de rechiffrement, de rejeu du nonce après interruption et de migration sont conservés. Cela ne qualifie pas un passage du disque système au profil pool.

## Régressions exécutées

| Contrôle | ARM64 natif | x86_64 sous Rosetta pour NovaFS, invité QEMU pour NovaOS |
| --- | ---: | ---: |
| NovaFS standard / instrumenté, zéro échec | 196 / 197 | 196 / 197 |
| Opérations différentielles, 100 graines | 1 000 000 | 1 000 000 |
| Remontages et audits du modèle / publications observées | 1 000 / 345 183 | 1 000 / 345 183 |
| Interopérabilité liblz4 1.11.1, pages vérifiées dans chaque sens | 256 / 256 | 256 / 256 |
| NovaOS : démarrages / groupes bloc / ELF-processus-IPC / CLI | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| DMA/GICv3 / DMA séquentiel, QEMU 10.0.12 livré avec UTM 5.0.8 | 11 / 23 | Profil AArch64 uniquement |

Aux 192 tests standards et 193 instrumentés historiques s’ajoutent le témoin antérieur du remplacement et les trois nouveaux tests. Le corpus couvre les transactions, la récupération, les snapshots/clones, les archives, le chiffrement, la compression et la migration P8/P9. Les quatre tests marqués optionnels dans Cargo ont été exécutés séparément : différentiel, image de pool, coût des extents et corpus LZ4. L’indicateur macOS `sysctl.proc_translated` vaut 1 dans le processus x86_64. [Effectifs, reçus, versions et empreintes](validation/v0.9.13/ancres-dechirees.json)

Les coupures dans le secteur sont celles du simulateur : aucun test n’a interrompu l’alimentation physique du stockage hôte. Les régressions NovaOS exercent les commandes distribuées et les protections DMA ; elles ne démontrent ni l’atomicité du stockage réel, ni un remplacement depuis NovaShell.

## Arbitrage demandé

**Priorité proposée :** qualifier, sans changer le format P9, une chaîne de stockage où tous les octets variables d’une ancre sont publiés indivisiblement à leur adresse et où une coupure ne touche pas l’autre slot. Une écriture atomique de 4 Kio convient ; pour ce format précis, une garantie comparable sur le premier secteur aligné peut suffire si le reste du bloc reste inchangé. Il faut également démontrer F1/F2 de bout en bout. La taille de secteur annoncée, un flush isolé ou un arrêt de VM ne démontrent pas ces propriétés.

Si cette garantie est indisponible, arbitrer un **témoin de publication authentifié indépendant et résistant au recul** : identité FS/LV et lignée, génération, racine et ordre durable des acquittements. Il faudra qualifier les coupures entre les deux supports, les clones, l’absence du témoin, la rétention des blocs et la sécurité des clés avant d’envisager RW. Ce serait une nouvelle architecture, pas une modification implicite des images existantes. Dans l’attente, NovaOS reste en v0.15.0 et NovaFS en v0.9.13 P9 ; aucun changement d’IPC, de droits ou de format.

## Reproduction

Depuis la racine du dépôt sur le Mac de qualification, avec les deux cibles Rust Darwin et Rosetta :

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib fs::remplacement:: -- --nocapture
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib qualification_ancres -- --nocapture
    python3 -B novaFS/lab/p51-validation-suites.py "$arch"
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test
done
python3 -B novaFS/lab/p6-lz4-interop.py --bibliotheque /opt/homebrew/opt/lz4/lib/liblz4.dylib
```

Les bancs DMA utilisent les [procédures et firmwares expérimentaux UTM](../../novaOS/docs/installer-v0.15.0.md). Les tests de cette qualification n’exigent pas l’installation d’une nouvelle version du système.
