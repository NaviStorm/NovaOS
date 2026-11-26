# Ethernet v1 — décision et qualification de phase 2

**Décision proposée à la MOA/MOE, 26 novembre 2026.** Conserver la boîte IPC_PUT/TAKE de 4 Kio comme transport de référence pour les trames groupées. Conserver DATA comme solution de repli quand une boîte privée ne peut être réservée. Les fenêtres CPU méritent un mandat d’expérimentation distinct. Leur débit satisfait les critères sur les deux ISA, mais le délai de queue pour une petite trame dépasse la tolérance sur deux des trois démarrages QEMU x86_64. Leur médiateur reste une fonction du banc privilégié. Les deux conditions d’une adoption définitive restent à instruire.

## Résultats décisifs

Les seuils arrêtés au commit `dcdd51d87b4c292f6a3e2c8518676c20c929dd82` imposent +10 % de débit à B=4 et B=8 pour 1 514 octets et au plus +10 % au p99 unitaire pour les deux longueurs. Les pourcentages ci-dessous comparent les fenêtres au meilleur des deux transports existants dans chaque cellule. [Critères antérieurs aux essais](criteres-phase2.md) · [96 cellules, mesures détaillées et répétitions](mesures-phase2.md)

| Profil de référence | Gain de débit B=4 / B=8 | Variation du p99 unitaire 60 / 1 514 octets |
|---|---:|---:|
| Darwin ARM64 natif | +18,9 % / +17,8 % | 0,0 % / 0,0 % |
| Darwin x86_64 sous Rosetta | +18,9 % / +19,2 % | 0,0 % / 0,0 % |
| NovaOS QEMU AArch64 | +68,0 % / +132,4 % | −37,3 % / −40,9 % |
| NovaOS QEMU x86_64 | +62,4 % / +95,8 % | **+128,0 %** / −40,1 % |

Sur x86_64 invité à 60 octets, les p99 fenêtres et meilleur concurrent valent respectivement **801,4 / 351,5 µs**, puis **243,4 / 326,6 µs** et **633,3 / 379,1 µs** aux deux répétitions. Le classement du p99 change selon le démarrage. Un p99 sur 16 lots est leur maximum. Cette dispersion interdit de conclure à un respect stable du seuil ou d’attribuer la pointe à une cause précise. Sur AArch64 invité, les trois démarrages franchissent les quatre seuils. Les ELF sont identiques entre les trois démarrages de chaque ISA.

Pour huit trames de 1 514 octets, DATA requiert **260** appels, la boîte **12** et les fenêtres **4**. Sur l’invité x86_64 de référence les débits respectifs sont 1,36, 26,65 et 52,16 Mo/s, avec 785,4, 36,2 et 12,0 commutations moyennes par lot. Quatre appels de contrôle sont communs aux trois chemins. Le banc effectue lui-même la copie des fenêtres sous verrou de l’ordonnanceur. Un futur protocole de médiation, ses validations et ses appels supplémentaires devront entrer dans une nouvelle mesure.

## Contrat et conformité

Le [contrat binaire v1.0](contrat-v1-phase2.md) fixe 14 opérations privées, les autorisations, les règles d’admission et la réconciliation par ticket. Le [modèle Rust no_std](../../tests/ethernet_v1/lib.rs) couvre la machine d’état du port. Le [modèle de transport](../../tests/ethernet_v1/transport.rs) couvre séparément le passage d’un lot entre deux participants. Le benchmark vérifie le descripteur, les octets remis et le reçu. Il ne chronomètre pas toutes les transitions de publication du modèle de port.

Les 17 tests réussissent sur chacun des deux exécutables Darwin. Le même fichier de 1 764 octets fournit 30 messages canoniques, huit refus et une erreur canonique. Sa vérification indépendante en Python et son décodage/réencodage en Rust produisent les mêmes octets sur les deux ISA. Le modèle se compile aussi en bibliothèque sans système pour les deux cibles NovaOS. [Tests exécutables](../../tests/ethernet_v1/tests.rs) · [Vecteurs communs](../../tests/ethernet_v1/vecteurs.bin) · [Darwin ARM64](../validation/ethernet-v1-phase2/conformite-aarch64.txt) · [Darwin x86_64](../validation/ethernet-v1-phase2/conformite-x86_64.txt)

| Invariant | Essai et observation |
|---|---|
| Identité, port, session, incarnation | Démarrage, port, titulaire, incarnation, canal et époque substitués séparément. Accès étranger refusé, ticket ancien refusé dans une nouvelle session, voisin utilisable après révocation. |
| Délégation et négociation | Ouverture sans droit refusée, observateur privé de TX/RX et d’administration, version inconnue refusée, bit obligatoire inconnu refusé, option inconnue ignorée. |
| Propriété et immuabilité | Une mutation du tampon source après admission laisse l’image intacte. Le remplissage d’une trame de 14 octets est nul jusqu’à 60. Slot en double, longueur invalide ou ancienne génération en fin de lot préservent crédits et générations. |
| Admission, pertes de réponses, publication partielle | Un même lot rejoué rend son ticket. Une mutation sous la même clé échoue. Dans un seul lot, une entrée peut être complétée, une engagée et une admise. Les transitions répétées et l’acquittement prématuré échouent. |
| Crédit et pression | Huit crédits et deux lots non acquittés bornent l’admission. Une complétion ou annulation rend un crédit une seule fois. La restitution RX est idempotente à génération identique. La file reprend après libération. |
| Lien, multicast, notifications | Filtrage exact de destination, quota de groupes, départ, lecture des statistiques, évènements coalescés, attente réarmée, notification terminale unique. Les compteurs saturés ne réintroduisent pas une génération ancienne. |
| Fermeture et résultat incertain | Une fermeture au milieu d’un lot annule la trame encore admise, conserve l’issue inconnue d’une publication engagée et garde la complétion confirmée. Images TX/RX effacées, nouvel envoi refusé. Révocation entre PUT et TAKE testée sur le modèle. |
| Réception indépendante | Deux complétions TX synthétiques produisent un seul RX observé chez le pair du modèle. La perte volontaire de l’autre trame démontre la différence entre complétion locale et livraison. |
| Représentation et remise des données | Corpus commun, requêtes et réponses canoniques, codes d’erreur, lots aux huit géométries, lecture dans le tampon du destinataire et comparaison octet par octet. |
| Séparation NovaOS | Sur les six démarrages, deux paires disjointes, croisement refusé avant modification, première paire révoquée, nouvel accès refusé et deuxième paire encore productive. Après collecte, deux pages utilisateur fraîchement allouées sont lues entièrement à zéro. L’identité physique de ces nouveaux cadres n’est pas suivie. |

## Méthode et limites de mesure

- **Darwin.** macOS 27.2.1 sur Apple Silicon, Rust 1.99.1 optimisé, Apple clang 21. Un processus neuf par cellule et par série, trois ordres de passage, 128 lots de chauffe et 4 096 mesurés par série. Total de 144 séries et 589 824 lots chronométrés. Chaque cellule rapporte la médiane des trois résultats indépendants. La sonde utilise le temps utilisateur et système cumulé de getrusage, les cycles matériels attribués au processus par ri_cycles et les commutations et réveils de l’hôte. Sous Rosetta, les cycles couvrent l’exécution traduite sur le processeur Apple. Le RSS inclut le runtime et le banc. Le retrait hôte chronomètre l’effacement du canal sur 129 essais par série. Les durées affichées à zéro sont inférieures à la résolution observable.
- **NovaOS.** QEMU 11.1.3, TCG monoprocesseur, 256 Mio, q35/max sur x86_64 et virt/cortex-a72 GICv2 sur AArch64, sans NIC. Chaque démarrage exécute 24 cellules, un lot de chauffe puis 16 lots mesurés par cellule. La référence couvre 768 lots dans les deux invités. Deux démarrages de diagnostic de plus par ISA portent le total à 2 304. Une passe additionnelle par cellule échantillonne les cadres après chaque appel. Tous ont rendu les octets attendus et retrouvé le solde initial. Les journaux donnent ticks, fréquence, commutations du BSP et réveils de l’ordonnanceur. La fréquence du compteur de temps invité ne fournit pas de cycles CPU matériels par trame. Le retrait inclut les refus post-révocation et la collecte des deux processus, une observation par cellule et par démarrage.
- **Travail chronométré.** Un saut, trames fictives déterministes, validation du contrôle, remise CPU dans l’espace du destinataire, relecture d’intégrité et reçu. Le profil NovaOS utilise de vrais appels IPC depuis les acteurs utilisateur Ring 3 ou EL0. Le dépôt précède le RECEIVE, d’où zéro réveil de tâche bloquée dans les séries de référence. Ce scénario mesure une remise séquentielle. La charge concurrente, l’attente bloquante, les IRQ, le CPU de chaque acteur, le duplex et la livraison sur un lien restent à mesurer dans leurs profils respectifs.
- **Coûts de copie.** Pour B trames de L octets, DATA réalise quatre copies de charge utile par fragment (mise en fragment, dépôt, retrait, réassemblage) et copie au total 4BL octets. La boîte regroupe autant de trames entières que tiennent dans 4 096 octets, avec deux copies par groupe et 2BL octets. Les fenêtres font une copie de capture puis une remise par slot, soit 2B copies et 2BL octets. Les constructeurs d’enveloppes, copies internes de structures et lectures de l’oracle contribuent aux durées mais sortent du décompte logique de charge utile.
- **Corrections du banc.** Le premier essai AArch64 dépassait la borne de 4 Kio de l’aide de copie utilisateur lors de la préparation d’un lot. La préparation et la relecture sont maintenant découpées en plages d’au plus 4 Kio. Une première conversion du temps CPU sous Rosetta utilisait la base de temps du processus traduit pour des compteurs issus du noyau hôte. Le relevé final utilise les timeval de getrusage. Les chiffres publiés proviennent des exécutions corrigées. [Banc NovaOS](../../tests/ethernet_v1/guest.rs) · [Sonde Darwin](../../tests/ethernet_v1/usage_darwin.c)

## Ressources et portée de l’isolation

Les quatre compteurs d’inventaire avant ouverture valent 0 processus utilisateur, 0 endpoint, 0 page IPC et 0 boîte dans ce scénario de démarrage. Le noyau, le firmware et le banc sont déjà en place. Avant la première cellule, l’allocateur annonce 62 025 cadres libres sur AArch64 et 62 357 sur x86_64. Pour chaque cellule, la comparaison finale utilise sa propre valeur initiale, après allocation des vecteurs de l’orchestrateur.

| Ressource d’une paire active | DATA | Boîte | Fenêtres |
|---|---:|---:|---:|
| Processus utilisateur / endpoints | 2 / 2 | 2 / 2 | 2 / 2 |
| Racines du producteur / du récepteur | 2 / 0 | 2 / 0 | 2 / 0 |
| Pages des tables IPC, boîte incluse / boîtes actives | 4 / 0 | 5 / 1 | 4 / 0 |
| Taille de l’objet de canal synthétique Darwin, octets | 288 | 4 256 | 12 272 |
| Pic invité échantillonné à B=8, L=1 514, Kio additionnels | 264 | 268 | 264 |

Chaque cellule retrouve ses quatre compteurs et son nombre de cadres libres initiaux. Le pic invité est un maximum aux points échantillonnés, pas une mesure continue du RSS de processus. En particulier, le tampon de contrôle d’intégrité, réutilisé comme instantané de la fenêtre expérimentale, est déjà provisionné dans le banc. Une implémentation autonome devra budgéter cette copie privée et la rétention des tickets, y compris en panne. Aucun cadre de ce transport ne reçoit de traduction DMA.

Le prototype garde les plafonds de **32 processus, 64 endpoints, quatre racines par acteur et 48 sélecteurs ABI v1**. La base possède également 32 handles locaux et huit boîtes de 4 Kio globales. Les tables ordinaires ne sont pas dimensionnées pour cent ports simultanés. Une référence matérielle passive du futur catalogue ne crée aucune instance dans le modèle. L’ouverture par le lanceur réserve les ressources d’une paire active. Aucun catalogue dynamique n’a été exécuté. Pour une future configuration des pools, compter les réserves système avant admission et retrait, fixer les quotas par acteur et par paire, vérifier les bornes des identifiants et bitsets, tester l’échec atomique des réservations et publier mémoire au repos, au pic et en quarantaine pour chaque profil. [Sources et projections de capacité](capacite.md)

## Adaptation expérimentale de VirtIO-net et arbitrages

1. **Choisir le transport du prochain mandat.** La boîte existante permet d’établir un témoin utilisateur avec la paire liée réservée par le lanceur. DATA fournit l’oracle de repli. Pour les fenêtres, décider au préalable comment installer, authentifier et retirer les deux extrémités depuis un acteur autorisé. Tout nouveau sélecteur, type de capacité ou primitive noyau demande un arbitrage préalable.
2. **Prouver la médiation autonome.** Borner les slots et les longueurs, figer l’image avant publication, refuser l’accès inter-session et les anciennes générations, fermer pendant une copie et tuer chaque acteur avec des reçus en vol. Tester deux paires et un voisin actif. Refaire débit et p99 avec le coût du vrai chemin utilisateur, sur des séries plus longues, en incluant petits paquets, attente bloquante, pression et coûts de retrait.
3. **Adapter un seul port VirtIO-net sous mandat distinct.** Épingler l’ELF, le firmware et le profil historiques. L’adaptateur traduit les tickets Ethernet vers la paire de files VirtIO de profondeur 16 et normalise l’en-tête privé. Les droits DMA directionnels, l’en-tête VirtIO, EVENT_IDX, les IRQ et les adresses matérielles restent chez leurs propriétaires. Exiger la comparaison des trames au pair isolé et la réconciliation indépendante des retours TX et RX.
4. **Décider de l’extension du profil.** Soumettre les essais à deux instances, le retrait nominal ou incertain, la continuité du voisin et les contrôles de stockage P9-512/P9-4096. Préserver les réserves concernant OVMF et fw_cfg PIO Q35, le correctif RX QEMU, IR/IRTA, le BUSY (24) en mode 11, les baisses de débit sous trafic, le p95 FLUSH et ses pointes historiques, les 34 Mio PIO, les 301 cadres ARM et la récupération de quarantaine. Les anciennes campagnes conservent leurs binaires et leur portée. [Réserves et plan de phase 1](qualification.md#intégration-progressive-de-virtio-net)
5. **Appliquer l’arbitrage Intel au stade de conception.** Le futur type AnneauDescripteursRW est limité aux pages dédiées d’anneaux des profils autorisés. Les données TX restent en lecture par le périphérique et les données RX en écriture. La réalisation physique, le contrôle de non-extension des permissions et la qualification sur machine nécessitent leur mandat. Les prochains arbitrages concernent aussi l’autorité de liaison, le budget de journal et de quarantaine, les plafonds configurables et le critère de latence sous charge.

## Traçabilité et reproduction

La base de phase 1 est `323793849e3e814e8804b675e39a294d84fbfbb6`. Les ELF invités qualifiés correspondent à `51466ff5543a259cc39efeaea201af59f69edea7`. La campagne Darwin corrigée correspond à `668ea6422486565b2f58ad5f7b790229fbb788ee`. Le changement entre ces deux commits porte sur le compteur hôte et le dépouillement Python. Les deux noyaux ordinaires ont aussi été reconstruits sans la fonctionnalité expérimentale et contrôlés pour l’absence du marqueur du banc. Les neuf tests du registre ABI contrôlent notamment sa régénération et sa compatibilité historique. Le mandat ne qualifie ni une installation UTM, ni une NIC, ni le réseau physique, ni une pile IP. Les campagnes historiques des pilotes, du stockage et des utilitaires sont extérieures à ces mesures. [Empreintes des sources, compilateurs, firmwares, ELF et ISO](../validation/ethernet-v1-phase2/provenance.json) · [Séries et résultats](../validation/ethernet-v1-phase2/) · [Sommes de contrôle](../validation/ethernet-v1-phase2/SHA256SUMS)

Depuis la racine du worktree dédié, le dépouillement suivant relit les octets publiés, vérifie chaque géométrie, les compteurs, les refus inter-paires, le bilan de retrait, puis régénère les deux rapports. Il fonctionne hors ligne avec Python 3.11 ou ultérieur.

```sh
export NOVAOS_TMPDIR="$(dirname "$(pwd -P)")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
preuves=novaOS/docs/validation/ethernet-v1-phase2
banc=novaOS/tests/ethernet_v1
sortie="$(mktemp -d "$NOVAOS_TMPDIR/ethernet-bilan.XXXXXXXX")"
(cd "$preuves" && shasum -a 256 -c SHA256SUMS)
python3 -B "$banc/vecteurs.py"
python3 -B "$banc/bilan.py" "$preuves" "$sortie/mesures.md" --json "$sortie/mesures.json" --repetitions
cmp "$sortie/mesures.md" novaOS/docs/architecture-ethernet/mesures-phase2.md
cmp "$sortie/mesures.json" "$preuves/mesures.json"
```

Pour exécuter les bancs avec un nouvel emplacement d’archives durable, utiliser les mêmes variables et les quatre cibles Rust installées. Les scripts conservent les séries, commandes, sorties de tests et empreintes. Le lanceur QEMU acquiert atomiquement le verrou commun pour chaque campagne et le libère après vérification de son propriétaire.

```sh
archives="$(dirname "$(pwd -P)")/archives-ethernet-phase2"
mkdir -p "$archives"
essai="$(mktemp -d "$archives/rejeu.XXXXXXXX")"
python3 -B "$banc/hote.py" "$essai/darwin"
python3 -B novaOS/tests/abi_registre.py
(
  cd novaOS || exit 1
  export RUSTC="$(rustup which --toolchain stable rustc)"
  for cible in aarch64-unknown-none-softfloat x86_64-unknown-none
  do
    rustup run stable cargo build --offline --locked --release --features ethernet-ipc-lab --target "$cible" || exit 1
    rustup run stable rustc --edition 2024 -O --crate-name ethernet_v1 --crate-type rlib \
      --target "$cible" tests/ethernet_v1/lib.rs -o "$essai/modele-$cible.rlib" || exit 1
  done
) && for numero in 1 2 3
do
  python3 -B "$banc/invite.py" "$essai/qemu-$numero" aarch64 x86_64 || exit 1
done
```

Les archives locales durables du mandat sont `OS-worktrees/archives-ethernet-phase2/{darwin-668ea64,qualification-finale,repetition-2,repetition-3}`. Les deux bibliothèques sans système sont également conservées dans `darwin-51466ff/<ISA>/modele-novaos.rlib`. Le répertoire `initial/aarch64` conserve la trace du premier échec de préparation des buffers. Les mesures antérieures aux corrections restent distinctes des séries de référence.
