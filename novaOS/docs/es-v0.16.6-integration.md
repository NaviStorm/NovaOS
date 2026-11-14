# NovaOS v0.16.6 — Préparation de l’intégration des E/S groupées

**Avis du 14 novembre 2026 : différer la fusion.** Les piles homogènes ont préservé les données des scénarios exercés. Une mise à jour partielle peut rendre une ancienne commande inutilisable, voire empêcher le montage. Lors d’une erreur DMA, le pilote peut aussi perdre la liste de cadres à restituer ou à conserver en quarantaine. Les performances mesurées justifient de poursuivre le prototype.

Le nouvel audit qualifie les correctifs de négociation et de propriété DMA sur le candidat `13df1dc`. Il présente la matrice avant/après, les coupures et les mesures individuelles. [Qualification après correction](es-v0.16.6-correctifs.md)

## Révisions et contenu à intégrer

Comparaison de `main` publié, `3193bc9108bdc771f4627f57c84e093375cc9ea4`, avec `codex/es-groupees-0166` à `d6a7a67a1e5addbb1fe849ee24fa4d61fc9accdd`. Le premier commit est l’ancêtre commun. Le diff avant cet audit couvre huit commits et 129 fichiers, dont 25 patches de qualification. La livraison publiée est NovaOS v0.16.5 avec NovaFS v0.9.13 P9.

| Couche | Diff déjà présent sur la branche | Code effectivement exercé en profil groupé |
|---|---|---|
| ABI et noyau | Définitions de trames et opérations additionnelles, portails, copie utilisateur, contrôles d’extrémités et de propriétaire, compteurs | Les fixtures privées étendent le noyau pour le prêt immuable multipage et la réservation de lots. Les appels historiques restent présents. Le numéro d’ABI seul ne permet pas de choisir le protocole du pair. |
| vi, nsh, services de fichiers et de stockage | Adaptations du courtier et du chemin de lecture par page dans le VFS et les services | Plan ordonné de plages et littéraux dans vi, validation par le courtier, contrôles des objets et du nom libre dans le service, transmission des lots jusqu’au pilote. Le consentement reste dans nsh. |
| Moteur NovaFS | Cache de nœuds déjà vérifiés, lecture de la vue propre en transaction et raccourci sur append. Ces optimisations s’appliquent aussi à l’interface historique. | API de laboratoire sous feature, itération par plages, préparation du nouvel index d’extents et publication P9 existante. La compilation de l’exemple Darwin et celle du service invité activé sont deux opérations distinctes. |
| VirtIO-blk | Portail de page et vérification du bail. Le transport du profil ordinaire garde son chemin 4 Kio. | Arène DMA privée, chaînes SG et polling. L’attente IRQ fait l’objet d’un patch distinct. |
| Qualification | Bancs, sondes, injection, profils firmware/QEMU/UTM, traces | Le raccourci clavier générant 4 Mio, les sondes, les pauses d’injection et les changements de politique hôte sont réservés aux bancs. |

Les quatre patches fonctionnels sont `io-0166-prototype`, `io-0166-plages`, `io-0166-virtio-multipage` et `io-0166-invite`. Ils se réappliquent dans cet ordre sur une exportation privée du commit, sans décalage ni tolérance de contexte. Le rapport d’interfaces consigne également l’ordre des variantes instrumentées et les empreintes. Avec les deux binaires compilés depuis un export brut du candidat sans patch fonctionnel, le profil 42 a monté P9 et sauvegardé une copie conforme sur AArch64/512 et x86_64/4 096. Les générations 75 et 76 ont été relues. Une simple fusion Git de la branche ne produirait pas le système groupé mesuré. [Contrat et compositions](es-v0.16.6-integration-interfaces.md) · [Comparaison du transport reconstruit](es-v0.16.6-integration-dma.md)

## Interopérabilité et décision de protocole

Les compositions mixtes ont réellement démarré dans les deux invités. Le rapport distingue succès, refus au lancement de vi, refus avant/après consentement et échec au montage. Sur les 32 essais, 18 ont publié les octets attendus et 14 ont refusé. Les images ont été relues. Le profil 41 lance la CLI ordinaire. Le profil 42 active les voies de laboratoire dans les binaires qui les reconnaissent. [Matrice des 32 compositions et SHA des ELF](validation/es-v0.16.6-integration-interfaces.json)

- Le vieux vi exige une réponse `INFO` de dix octets. Le nouveau courtier lui en renvoie onze en profil 42. En sens inverse, un courtier publié refuse `TEXT_PAGE` d’un nouveau vi.
- La longueur de `LOOKUP_AT` indique la présence d’informations P9. L’ancien service P9 fournit déjà cette longueur, sans savoir publier un plan. Le nouveau courtier peut donc demander « OUI » pour une opération que son pair refusera.
- L’ancien noyau renvoie `UNKNOWN_CALL` à `IPC_PUT`. Le nouveau client limite son repli à `DENIED`. Un ancien relais ou pilote peut aussi recevoir une opération PAGE que le seul droit local du service appelant ne suffit pas à négocier. Le montage peut alors échouer.
- Pour présenter une intégration, conserver le format exact de chaque réponse historique. Négocier la version et les opérations de chaque liaison avec son pair avant la première mutation et le consentement. Un refus de capacité reste une décision d’autorisation. Une erreur d’E/S, un retrait et une publication incertaine exigent leur traitement propre. Une seconde publication automatique après une réponse incertaine pourrait dupliquer une opération déjà durable. Rejouer la matrice mixte dans les deux sens sur la solution retenue. [Analyse des contrats](es-v0.16.6-integration-interfaces.md)

## Publication, coupure et compatibilité disque

1. Le service résout la révision source, le parent et le nom libre. Les plages monotones sont bornées à la taille de l’objet et la somme des littéraux est vérifiée. Chaque page source dense est contrôlée avec l’identité du volume, son adresse, sa génération et son contenu, y compris les octets retirés par le plan. Le texte et le bourrage de fin sont contrôlés.
2. Le moteur construit les pages de la candidate hors de l’arbre publié avec un allocateur CoW privé. Il les écrit par lots, ferme l’index des extents et revalide le prêt au dernier point d’admission. Les deux générations montables protègent les blocs encore référencés. Le chiffrement des pools, les domaines de nonces et les formats des snapshots/clones suivent leurs chemins historiques. La voie accélérée évaluée utilise un volume autonome P9 et un fichier dense.
3. Le moteur écrit les métadonnées et le manifeste puis attend **F1**. Il écrit ensuite l’ancre alternée et attend **F2**. La confirmation de succès à vi suit F2. Une erreur d’ancre ou de F2 laisse un état de publication incertain et impose une nouvelle inspection du support. La complétion VirtIO d’une écriture ne remplace aucune des deux barrières. [Moteur de publication](../../novaFS/src/engine.rs) · [Construction des pages](../../novaFS/lab/es-groupees/preparation.rs)
4. Le cache appartient à l’instance montée. Un hit exige la famille et la référence complète. Un miss vérifie la page avant son insertion. En transaction, une page préparée masque la vue propre. L’optimisation d’append utilise la limite de l’ancien EOF contrôlée au montage, en conservant le traitement de la page de bord. Les encodeurs d’ancres, manifestes, références et empreintes sont ceux de P9. [Cache](../../novaFS/src/cache.rs) · [Arbres](../../novaFS/src/tree.rs) · [Étendues autorisées](../../novaFS/src/fs/check.rs)

Les 36 coupures sur les deux architectures et les deux géométries distinguent notamment données préparées, F1 terminé, FLUSH suspendu, écriture d’ancre suspendue et F2 terminé. Après redémarrage, le banc vérifie les générations et les octets. Une destination complète peut déjà survivre après F2 et avant le retour de la réponse à vi. Les 20 injections de défaut/retrait et les scénarios de conflits couvrent la source modifiée, supprimée ou recréée, la destination occupée et les écritures concurrentes. La perte physique d’alimentation et le rollback intégral du support restent hors de cette qualification. Une ancre déchirée peut imposer le refus du montage normal. [Régressions de cet audit](validation/es-v0.16.6-integration-regressions.json) · [Contrat des ancres P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

### Retour en écriture au moteur publié

Un banc indépendant compile le même client de contrôle sur deux exports Git neufs. L’ancien moteur crée un P9 et une source dense de 4 Mio. L’autre architecture monte le volume avec le profil P9 explicite, retire trois octets à cheval sur une frontière de page et insère « été » suivi d’un saut de ligne dans une nouvelle destination via la préparation CoW groupée. Le moteur publié remonte le résultat en écriture, contrôle la source, crée un fichier témoin, puis vérifie les deux générations et tous les octets attendus. Chacun des moteurs remonte ensuite le volume en lecture seule. Le SHA-256 de l’image entière reste identique pendant ces deux dernières étapes. [Client de contrôle](../../novaFS/lab/es-groupees/interop-integration.rs)

| Écrivain publié → écrivain groupé → retour publié | Secteurs | Générations après création, publication, écriture historique, deux remontages |
|---|---:|---|
| ARM64 natif → x86_64 Rosetta → ARM64 natif | 512 et 4 096 | 66 → 67 → 69 → 69 → 69 dans les deux essais |
| x86_64 Rosetta → ARM64 natif → x86_64 Rosetta | 512 et 4 096 | 66 → 67 → 69 → 69 → 69 dans les deux essais |

Les quatre images étaient jetables. Aucun outil de migration n’intervient. `Engine::mount_current` est l’entrée publiée qui négocie le contenu P9. Une vérification préliminaire a également constaté le refus prévu de `Engine::mount` sur cette image (« profil P9 non négocié »). Les pools chiffrés, les snapshots, les clones, la compression et les migrations figurent dans les régressions ordinaires. [Empreintes des quatre exécutables et des images à chaque étape](validation/es-v0.16.6-integration-p9.json)

## Mémoire, DMA et IRQ

La préparation dense utilise au plus deux fenêtres de 256 pages de 4 Kio, soit 2 Mio pour les pages d’entrée et de sortie réunies. Les références temporaires, l’index CoW, les métadonnées, le prêt de littéraux et la mémoire des processus s’y ajoutent. Les fenêtres réservent avant de traiter les pages et effacent leur contenu à la destruction. Cette borne locale ne borne pas tout le tas de la transaction. Certaines allocations de métadonnées utilisent les collections Rust ordinaires, sans retour local `NoMemory`. Le pic en cadres du système complet figure dans les mesures. [Préparation CoW](../../novaFS/lab/es-groupees/preparation.rs) · [Index CoW](../../novaFS/lab/es-groupees/arbre.rs)

Le maximum d’arène est **512 Kio** (64 pages par sens). Le total du transport qualifié atteint **576 Kio** en incluant les cadres fixes. La chaîne SG transporte au plus 256 Kio. Les soumissions vérifient le droit, la direction, les LBA et les bornes du tampon. En cours de retrait, les pages restent réservées jusqu’au drainage, à l’arrêt matériel et à la confirmation d’invalidation. Le test de réemploi lit un cadre effacé. [Bornes et cycle de vie](es-v0.16.6-integration-dma.md)

Deux défauts historiques concernent la propriété des cadres. Après allocation et avant exposition DMA, un refus précoce de `Domain::start` peut perdre 16 cadres ARM64 (64 Kio) ou sept cadres x86 (28 Kio). Après exposition, `close(self)` peut échouer alors que le transport a déjà été retiré de la session. Le pilote met celle-ci en quarantaine, mais perd l’inventaire des cadres épinglés. La future intégration doit conserver un propriétaire explicite et tester les deux chemins. Une libération inconditionnelle sur erreur exposerait les pages encore traduites. Aucune croissance illimitée ni réutilisation non autorisée n’a été observée sur ces chemins. [Audit et témoin d’allocation](es-v0.16.6-integration-dma.md)

Le polling reste le mode de référence, l’IRQ une variante expérimentale. Les tests du modèle IRQ couvrent 30 entrelacements et l’horloge figée sur les deux architectures hôtes. En polling AArch64, un SPI parasite sans bit ISR peut entraîner le masquage de la ligne et affecter la disponibilité des requêtes suivantes. Cet incident demande son propre essai sur le pilote par défaut. [Audit DMA](es-v0.16.6-integration-dma.md) · [Comparaison de latence IRQ/polling](es-v0.16.6-irq.md)

## Sauvegardes de référence

Mac M4 Max, 128 Gio, macOS 27.2 (26B123), disque interne APFS chiffré. Les VM ont 256 Mio, un vCPU TCG, et utilisent le polling. QEMU direct 11.1.3 écrit sur RAW. L’application UTM 5.0.8 utilise son QEMU 10.0.12, le QCOW2 clairsemé, la politique ordinaire et le profil sans charge ajoutée. Les mesures couvrent la commande `:w copie`, le consentement automatisé et le retour de succès après F2. La préparation de l’édition précède le chronomètre. La source, la destination et les deux générations sont relues. Sous UTM, le banc redémarre aussi la VM et vérifie l’export à froid.

| Sauvegarde hors sonde, en ms | QEMU AArch64 | QEMU x86_64 | UTM AArch64 | Dont après « OUI » sous UTM |
|---|---:|---:|---:|---:|
| Création de 4 Mio, secteur 512 | 77,687 | 142,530 | 98,305 | 74,633 |
| Insertion de Ω au milieu de 4 Mio, secteur 4 096 | 88,429 | 147,553 | 94,402 | 86,428 |
| Ajout de 4 Mio et éditions multiples, secteur 512 | 127,909 | 230,026 | 129,980 | 115,396 |
| Même opération, secteur 4 096 | 128,593 | 229,637 | **829,056** | **753,536** |

L’édition multiple publie 8 Mio plus un octet. Sur l’essai UTM à 829,056 ms, QEMU a enregistré 143 lectures, 45 écritures et deux FLUSH. Le temps cumulé des FLUSH hôte est 3,327 ms. Le banc n’a pas tracé le placement du vCPU pendant cet essai. Trois essais préparatoires QEMU AArch64 ont démarré pendant la construction du banc P9 (125,283, 95,978 et 131,641 ms). Ils figurent dans le relevé avec leurs trois répétitions après compilation.

| Comptage sur le chemin de sauvegarde, par scénario sondé | Interservices et dons | Avec contrôles | Avec copies locales du prêt | Soumissions VirtIO, dont FLUSH |
|---|---:|---:|---:|---:|
| Création 4 Mio / 512 | 27 | 36–37 | 42–43 | 43 dont 2 |
| Insertion locale / 4 096 | 39 | 43 | 43 | 169 dont 2 |
| Édition multiple / 4 096 | 47 | **56** | **64** | 190 dont 2 |

Les 21 observations de référence comprennent douze chronométrages hors sonde et neuf publications instrumentées, pour les trois scénarios sur chaque plateforme. Deux répétitions du banc livré ont pris 81,737 ms sur QEMU AArch64 et 270,031 ms sur UTM en création instrumentée. QEMU a compté une fermeture locale de plus au deuxième passage (trois contre deux). Le relevé individuel conserve les 26 observations, y compris les trois essais préparatoires.

Les appels locaux du portail de file et les soumissions du périphérique sont ventilés séparément dans les relevés bruts. Le coût conservateur de l’édition multiple comprend les contrôles et la publication. Les huit copies locales du prêt le portent à 64 si on les ajoute également. La limite de 64 allers-retours IPC est respectée dans les essais comptés. Le pic de session de la grande édition hors sonde est de 9 754 cadres invités sur AArch64 direct, 9 765 sur x86_64 direct et 9 735 sous UTM. En profil sondé, l’arène DMA utilise 80 cadres sur ce scénario, 65 sur la création. Ce sont des pics, distincts du nombre de cadres qui restent épinglés lors d’un échec de retrait. [Relevés individuels](validation/es-v0.16.6-integration-mesures.json)

La campagne du présent audit est une requalification ciblée. Une valeur par cellule ne suffit pas à estimer son p99. Dans le rapport QoS antérieur du même candidat, les 37 sauvegardes UTM en politique ordinaire sont restées sous la seconde, avec un maximum de 824,548 ms. La campagne polling ordinaire précédente avait mesuré **9 dépassements sur 30**, jusqu’à **1 951,191 ms**, dont quatre au repos sur l’édition multiple. Avec d’autres politiques hôtes, les extrêmes de 1 762,131 et 1 846,086 ms sont également archivés. Le contrat d’une seconde systématique sous UTM reste ouvert. [Distributions et conditions hôtes](es-v0.16.6-qos.md) · [Extrêmes du polling](es-v0.16.6-irq.md)

## Régressions, reproduction et décision de fusion

Le 14 novembre 2026 entre 05 h 51 min 39 s et 06 h 01 min 34 s UTC, **86/86 commandes de régression** ont réussi sur le candidat. Les SHA des noyaux et journaux sont conservés. Par architecture, les contrôles NovaOS incluent 25 démarrages, 24 groupes bloc, 44 groupes fichier, 24 groupes d’espace de noms, trois contrôles ELF/processus/IPC et quatre essais CLI. Ils couvrent aussi nsh, les scripts, le terminal, vi et la mémoire. Les campagnes firmware AArch64 sur le moteur fourni avec UTM ont donné 11/11 GICv3 et 23/23 DMA séquentiels, plus la reprise.

Sur Darwin ARM64 natif et x86_64 Rosetta, NovaFS a réussi 196 tests standards et 199 instrumentés par architecture. Les compteurs du framework indiquent également six tests marqués ignorés par profil. Les bancs dédiés ont exécuté un million d’opérations différentielles, 2 304 injections sur les ancres et 486 injections CoW par architecture. Les profils mémoire de 128 et 256 Mio ont été exercés. L’amorçage à 64 Mio reste hors du relevé. [Résultats par suite, horodatages, commandes et restrictions](validation/es-v0.16.6-integration-regressions.json)

Reproduction depuis la racine du dépôt, en utilisant les archives privées décrites dans les rapports CPU et DMA. Le générateur et les vérificateurs hôtes sont construits avant les mesures. Les profils UTM et les disques de test sont jetables. Choisir un chemin de sortie libre pour chaque mesure.

```sh
python3 -B novaFS/lab/es-groupees/interop-integration.py \
    3193bc9108bdc771f4627f57c84e093375cc9ea4 d6a7a67a1e5addbb1fe849ee24fa4d61fc9accdd \
    /tmp/nova-integration-p9.json

generateur=novaFS/target/aarch64-apple-darwin/release/examples/es-groupees
python3 -B novaOS/tests/integration_mesure.py qemu x86_64 4096 ajout-4m-multiple \
    novaOS/build/cpu-0166/cpu-sans "$generateur" /tmp/nova-integration-qemu.json
python3 -B novaOS/tests/integration_mesure.py utm aarch64 4096 ajout-4m-multiple \
    novaOS/build/irq-0166/utm-polling-sonde "$generateur" /tmp/nova-integration-utm.json \
    --profil-utm novaOS/build/io-0165/utm/NovaOS-nsh-systeme-6e5f7404.utm --sonde
```

**Décision soumise à la MOE/MOA.** Poursuivre sur la branche du prototype pour qualifier une négociation explicite par liaison, préserver les formats de réponse historiques et établir la propriété des cadres sur les chemins d’erreur DMA. Rejouer les croisements, les injections de retrait et les sauvegardes UTM ordinaires sur le candidat corrigé avant une demande de fusion. Le retour au moteur publié sur un volume autonome P9 est démontré par les quatre essais croisés. Un déploiement progressif des services exige encore les corrections de coexistence. Le mode IRQ reste expérimental. L’activation du nouveau contrat interservices et une publication de v0.16.6 relèvent d’un arbitrage ultérieur.
