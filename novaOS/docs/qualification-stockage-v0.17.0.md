# NovaOS v0.17.0 — Décision de qualification du stockage

**Fusion suspendue le 14 novembre 2026.** Sur AArch64 et x86_64, en secteurs de 512 et de 4 096 octets, le chemin expérimental groupé conserve une réservation de 256 pages après avoir rendu l’erreur 26 à `vi`. Le relais était retenu avant tout envoi au pilote. La source a été retrouvée intacte sur les quatre disques. Les critères 6 et 8 du mandat échouent. Le candidat examiné est `e08b9796e59523b917b9d020520d5df97c9a4380`, descendant du SMP `835b9d9dd817a2feef54b6c17f000b0aec8a3879`. [Résultats individuels](validation/fusion-smp-v0.17.0/reponses-stockage.json)

## A. Incident historique et cause du blocage actuel

**Incident historique non expliqué, mais scénarios d’échec représentatifs reproduits et qualifiés.** Lors du premier essai AArch64/512, l’éditeur avait reçu le choix de destination, puis l’erreur 26 avant l’armement de la coupure F2. L’image originale a été libérée par l’ancien banc. Une écriture suspendue, un FLUSH F1 suspendu et une réception IPC expirée ont depuis produit le même message. Aucun des 56 rejeux instrumentés antérieurs n’a identifié la cause d’origine. La réserve documentaire est recevable selon le mandat. La suspension actuelle repose sur une observation indépendante. [Série historique et contrôles antérieurs](fusion-smp-v0.17.0.md#diagnostic-du-refus-avant-f2)

### Autorisation de la destination et acquittement de la sauvegarde

| Étape | Condition observable |
|---|---|
| 1. Destination sélectionnée | `OUI` accepte le nom libre dans le dialogue de `vi` avec `nsh`. La capacité de destination délimite l’opération. |
| 2. Transaction commencée | Le service NovaFS valide le plan de plages, la source et la destination, puis prépare les blocs privés. Les sondes distinguent le premier lot, son tag, sa taille et son statut. |
| 3. Données écrites | Le service bloc relaie la commande vers le pilote VirtIO. L’acquit du périphérique termine le transfert de ces blocs. Leur persistance dépend encore de F1. |
| 4. F1 confirmée | Le périphérique a acquitté le FLUSH des données, des métadonnées CoW et du manifeste. L’écriture de l’ancre devient alors admissible. |
| 5. Publication CoW | L’écriture de l’ancre vise le slot suivant. Une erreur à cette frontière rend l’issue de publication incertaine. |
| 6. F2 confirmée | Le second FLUSH est acquitté. Le moteur peut adopter le nouveau sommet sous le contrat de persistance de son périphérique. |
| 7. Retour durable à l’application | Après la réponse de la transaction, `vi` affiche « fichier enregistré ». Le banc vérifie ensuite le fichier entier dans une nouvelle VM et avec le lecteur hôte. |

Les temps d’accès de la source font partie du modèle. Sur le volume groupé, ouvrir le fichier dans `vi` a publié une transaction autonome, de G66 à G67, avec ses propres F1 et F2. La photographie avant `OUI` et les compteurs de FLUSH distinguent cette écriture de la sauvegarde. Le banc classique crée un autre volume P9 avec `Atime::No`. [Publication du moteur](../../novaFS/src/engine.rs) · [Décodage des statuts du service](../user/novafs/files.rs) · [Éditeur](../../components/vi/editor.rs) · [Sonde des frontières](../tests/stockage_phases.py)

### Défaut de cycle de vie du lot groupé

1. Le client bloc de NovaFS ouvre la réservation du lot par `0x1002e` avant l’échange IPC. Le noyau attache au canal un tag, des LBA, un nombre de pages et l’intervalle virtuel autorisé.
2. Le relais conserve la requête sans l’envoyer au pilote. L’attente de réponse expire après 506 ticks. La sonde du transport observe `reception op=3 phase=4 n=256 tag=2216 statut=8`. L’expiration du noyau réveille le thread. Le retour d’erreur de `exchange` quitte `lot`. Le service marque le périphérique en échec puis rend 26 à l’éditeur.
3. Lorsque `vi` a reçu ce refus, une sonde lit le noyau sous la garde IPC. Elle trouve exactement une réservation pour ce tag, avec `pages=256 transfert=false retrait=false doute=false progression=0`. Le chemin de retour n’a déclenché ni retrait sur le canal du lot, ni clôture par le pilote. Libérer le droit de destination du dialogue avec `nsh` concerne un autre canal.

Un lot de 256 pages couvre **1 Mio d’adresses**. Le constat porte sur l’autorisation logicielle encore ouverte pour cette requête abandonnée. Le banc a maintenu le relais suspendu. Il n’a soumis aucun DMA tardif et n’a constaté ni perte de cadres physiques, ni défaut de l’IOMMU, ni corruption sur disque. Dans un essai distinct, la révocation explicite après soumission a bien attendu le drainage. [Réservation et retour sur erreur](../tests/fixtures/io-0166-prototype.patch) · [Contrôle du bail multipage](../tests/fixtures/io-0166-virtio-multipage.patch) · [Expiration et réveil IPC](../kernel/src/sched/process/ipc/mod.rs) · [Observation du noyau dans l’export d’essai](../tests/fixtures/stockage-0170-reponses.patch)

### Matrice exécutée

Chaque ligne ci-dessous a été exercée sous QEMU 11.1.3 TCG, à quatre vCPU et 256 Mio, dans les **quatre combinaisons** architecture × géométrie. Les disques sont des images raw fictives avec cache writeback. Les quatre succès de sauvegarde groupée créent une destination de 8 Mio en insérant 4 Mio devant la source de 4 Mio. Les lecteurs vérifient les deux ancres, le graphe P9 et les octets avant et après redémarrage sur copie. Le Mac est resté alimenté.

| Injection sur le chemin groupé | Retour de vi | Barrières de la sauvegarde acquittées | État effectivement relu |
|---|---:|---|---|
| Refus avant la première écriture | 26 | Aucune | G67, destination absente |
| Erreur QEMU sur l’écriture de données | 26 | Aucune | G67, destination absente |
| Données acquittées, refus avant F1 | 26 | Aucune | G67, destination absente |
| FLUSH F1 suspendu dans QEMU jusqu’à expiration du pilote | 26 | Aucune | G67, destination absente |
| Erreur QEMU sur l’écriture de l’ancre | 22 | F1 | G67, destination absente |
| Ancre écrite, refus avant F2 | 22 | F1 | G68, destination complète |
| FLUSH F2 suspendu dans QEMU jusqu’à expiration du pilote | 22 | F1 | G68, destination complète |
| Sauvegarde acquittée de bout en bout | 0 | F1 et F2 | G68, destination complète |

Après la confirmation du nom, QEMU a compté six écritures terminées au refus qui suit le premier lot, 42 à l’expiration de F1 et 43 au témoin de succès. Pour ce témoin, les deux FLUSH de la sauvegarde ont terminé dans chacun des quatre profils. [Compteurs QEMU et résultats des 32 essais](validation/fusion-smp-v0.17.0/phases-stockage.json)

Le statut 22, « publication incertaine », couvre les deux issues effectivement observées. Aucune des 28 injections de cette matrice n’a produit le message de succès. Le lecteur a retrouvé la source dans tous les cas. [Banc des phases](../tests/stockage_phases.py)

| Autres essais | Résultat et portée |
|---|---|
| Acquit réel tardif ou postérieur à une annulation | 8/8. La réception renvoie respectivement 8 ou 9. L’envoi tardif expire avec 8. |
| Ancien tag sur un canal actif | 4/4. Le transport a livré le tag 2215. Le service qui attendait 2216 a rejeté la réponse avec le statut 3. |
| Mort du relais ou du pilote | 8/8. Les processus sortent respectivement avec 37 et 38. Le transport observe la fermeture du canal, statut 11. |
| Révocation explicite avec commande en vol | 4/4. Retrait temporairement bloqué après le kick, puis drainage et révocation. |
| Abandon avec lot retenu avant envoi au pilote | **0/4 au contrôle de fermeture de l’admission**. Dans les quatre cas, le statut public est 26, la source est exacte et la destination absente. |
| Génération de l’objet IPC et cookie de réveil | `ipc-test` réussi sur les deux architectures à quatre vCPU. T04/T11 exercent la réutilisation d’un slot et le rejet de l’identité antérieure. Ce test noyau est indépendant du secteur et du test du tag applicatif. |
| Chemin classique P9, client natif | 24/24. Lecture, écriture ou F1 en erreur donnent 26 et G5 ancienne. Erreur de l’ancre et erreur de F2 donnent 22, respectivement G5 ancienne et G6 nouvelle. Le témoin donne 0 et G6 nouvelle. Les 24 copies redémarrées en lecture seule ont gardé le SHA-256 de l’image arrêtée. |
| Régressions historiques du client fichiers | 40/40 coupures ou frontières de fermeture/remontage et 16/16 erreurs. Ces images sont produites par la commande historique `novafs script`. Le contrôle P9 dédié figure à la ligne précédente. |
| Collecteur de preuves | 5 tests hôte réussis. 4 interruptions intentionnelles du banc avant `OUI` avec conservation des preuves et remontage indépendant. Une coupure QEMU supplémentaire pendant F2 sur AArch64/512 a laissé la destination de 8 Mio complète, sans succès annoncé par l’éditeur. |

Les 28 essais de réponses et de retrait ont tous rendu 26 à l’application et remonté les ancres G66/G67. Les 4 échecs représentent un défaut de gestion de ressources alors même que l’oracle de disque a réussi. Le profil classique utilise la commande native de qualification `files-inject` et lit son code de sortie. Le chemin groupé exerce le dialogue et le message réels de `vi`. [Résultats IPC et retrait](validation/fusion-smp-v0.17.0/reponses-stockage.json) · [Résultats classiques et empreintes](validation/fusion-smp-v0.17.0/classique-final.json) · [Résultats du collecteur](validation/fusion-smp-v0.17.0/collecteur-stockage.json)

### Conservation et reproduction

Chaque incident est isolé dans un répertoire unique en mode 0700. Le collecteur fige l’image initiale, conserve le disque après l’arrêt, puis utilise une copie pour le redémarrage. Le bilan contient les empreintes, les horodatages, la série brute, les traces IPC et stockage extraites, les statuts observés, l’erreur éventuelle du banc, la configuration de la VM et la provenance des ELF. La série constitue la source du diagnostic. Une coupure avant une réponse peut laisser le champ des statuts observés vide. Les premiers réglages du banc restent archivés avec leur propre bilan. En particulier, la première faute de lecture à usage unique n’a pas produit d’erreur dans le client classique. Les quatre essais ont échoué à l’oracle d’injection alors que le client avait confirmé la sauvegarde. Quatre redémarrages supplémentaires en lecture seule ont retrouvé pour chacun la source exacte et les 4 231 octets confirmés en G6. La campagne retenue maintient cette faute jusqu’à l’arrêt de la première VM et la retire pour le redémarrage. Les archives locales comprennent les médias et journaux synthétiques. Le nettoyage d’un incident exige la revue du dossier et la confirmation de l’empreinte actuelle de son manifeste. [Index des archives locales et empreintes des inventaires](validation/fusion-smp-v0.17.0/preuves-locales.json) · [Collecteur et commande de nettoyage](../tests/preuves_stockage.py) · [Banc de conservation](../tests/test_preuves_stockage.py)

Le commit des bancs est `3a404bc0cead0200fe65a508e7b075b3e896cbf6`. Depuis la racine de la branche de finalisation sur le Mac de qualification, dans un répertoire de sortie neuf, ces commandes reconstruisent le noyau classique et les deux variantes instrumentées pour AArch64 et x86_64. Les sondes ne sont appliquées qu’à des exports indépendants.

```sh
set -eu
sortie=$(mktemp -d)
revision=$(git rev-parse HEAD)
export NOVA_VCPUS=4 NOVA_QUALIF_VCPU=4
python3 -B novaOS/tests/test_preuves_stockage.py
python3 -B novaOS/tests/reconstruire_es_0166.py "$revision" "$sortie/classique" brut
for sonde in phases reponses; do
    python3 -B novaOS/tests/reconstruire_es_0166.py "$revision" "$sortie/$sonde" \
        diagnostic-stockage --sonde "stockage-0170-$sonde.patch"
done
brut="$sortie/classique/brut"
outil="$brut/novaFS/target/aarch64-apple-darwin/release"
(cd "$brut/novaFS" && ./lab/run build --offline --lib)
rustup run stable rustc --edition=2024 -L "dependency=$outil/deps" \
    --extern "novafs=$outil/libnovafs.rlib" novaOS/tests/volume_erreurs_p9.rs -o "$sortie/volume-p9"
mkdir "$sortie/rapports"
python3 -B "$brut/novaOS/tests/erreurs_stockage_classique.py" aarch64 512 \
    "$sortie/rapports/classique.json" --noyau "$brut/novaOS/target/aarch64-unknown-none-softfloat/release/novaos" \
    --generateur "$sortie/volume-p9" --lecteur "$outil/novafs"
python3 -B "$sortie/phases/diagnostic-stockage/novaOS/tests/stockage_phases.py" aarch64 512 \
    "$sortie/rapports/phases.json"
python3 -B "$sortie/reponses/diagnostic-stockage/novaOS/tests/reponses_stockage.py" aarch64 512 \
    abandon "$sortie/rapports/abandon.json"
```

Après reconstruction depuis le commit des bancs sur les deux architectures, quatre nouveaux essais du cas `abandon` ont retrouvé le même défaut et le même état G66/G67 sur copie redémarrée. Quatre essais supplémentaires (succès et expiration de F2 sur AArch64/512 et x86_64/4096) ont retrouvé G68 et les 8 Mio attendus. Les inventaires des rejeux sont conservés séparément de la matrice initiale.

Le dernier test termine en erreur tant que le lot reste ouvert. Les observations et le bilan d’échec sont écrits dans le répertoire privé de l’essai. Pour changer de profil, passer `x86_64` avec l’ELF `x86_64-unknown-none`, ou `4096` pour la taille de secteur. Utiliser un rapport neuf par invocation. `--cas` sélectionne une seule frontière de publication. Le banc de réponses accepte également `tardive`, `annulation`, `ancien-tag`, `mort-relais`, `mort-pilote` et `revocation`. [Reconstruction et provenance](../tests/reconstruire_es_0166.py) · [Client classique](../tests/erreurs_stockage_classique.py) · [Client de réponses](../tests/reponses_stockage.py)

## B. Décision sur la fusion

| Critère de clôture | Décision pour cette campagne |
|---|---|
| 1. Diagnostics internes distinguables | Vérifié sur les profils instrumentés. Les phases, opérations et statuts distinguent la réception, l’identité, l’erreur VirtIO, le délai du pilote et l’issue de publication. Le code public 26 historique demeure insuffisant pour attribuer seul la panne. |
| 2. Propagation à l’application | Vérifiée sur les défauts injectés. Les statuts 22 et 26 ont été observés sans message de succès. |
| 3. Durabilité après confirmation | Vérifiée après redémarrage QEMU pour les témoins confirmés. La perte d’alimentation physique de l’hôte reste hors qualification. |
| 4. Intégrité des générations | Vérifiée sur les images injectées. Les sources et destinations ont été comparées en entier. |
| 5. Réponses tardives | Les réponses arrivées après expiration ou annulation, ou avec l’identité précédente, n’ont confirmé aucune sauvegarde. |
| 6. Retrait des ressources DMA | **Échec** du cycle de vie de la réservation après abandon avant soumission. Le retrait explicite en vol a réussi. |
| 7. Conservation des preuves | Vérifiée sur des échecs volontaires du banc et les quatre échecs d’admission. |
| 8. Passage sur les deux architectures et géométries | **Échec** du même contrôle dans les quatre combinaisons. |
| 9. Incertitude historique documentée | Oui. Le nouvel échec ne permet aucune attribution de la panne initiale. |

Le candidat, ses manifestes et ses bannières indiquent v0.17.0. Le manifeste de l’archive UTM AArch64 a été revérifié, avec 27 empreintes internes conformes. Le profil installable conserve les E/S classiques et le polling. Les arbres Git de NovaFS v0.9.13 P9 et de l’ABI native sont identiques sur `main`, la branche SMP et le candidat. Les campagnes générales SMP, système, performance et UTM du candidat versionné restent consultables dans le rapport antérieur. Le §9 du mandat conditionne leur reprise finale à la réussite de la clôture. Aucune nouvelle mesure UTM, de performance ou de coupure physique n’est revendiquée ici. [Archive et contrôles antérieurs](fusion-smp-v0.17.0.md#régressions-du-candidat-versionné)

## C. Suite soumise à décision

L’audit transversal v0.30.0 reste en attente de la fusion. La correction à étudier se situe à la frontière entre l’abandon IPC et la réservation du lot. Elle devra fermer l’admission de nouvelles commandes pour le bon canal et le bon tag avant restitution du tampon, distinguer un lot jamais soumis d’un transfert déjà engagé et conserver les cadres en quarantaine lorsque leur retrait reste incertain. Le cas `abandon` devra passer sur les quatre profils, avec les contrôles de concurrence, d’acquittement tardif, de drainage, de réemploi et de récupération. La reprise de la procédure de fusion dépend de ces résultats.
