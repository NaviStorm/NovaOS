# NovaOS v0.21.0 — Qualification du retrait de NovaFS entre NEXT et COMPLETE

Le candidat local `ccb50dde8c90ae5d1f8233cefe78b1530bf58305` corrige le défaut reproduit sur `97ea39c93a86cf750e27982cb1679d02893316a6`. Le dernier contrôle de la base de fusion a donné `a6df93fb8687c68893680707f8763471d9f8a2b3` sur `origin/main`. Le registre ABI qualifié `df88f1cfeaf0940da876ea543584e59215abacd4` et la phase 2 du temporaire privé `35f7d9619f116f42491f001af4b1cf5bdc84b395` figurent dans l’ascendance du candidat. [Qualification initiale et preuve du défaut](integration-abi-tmp-v0.21.0.md)

## Cause et correction

À la fin du service, le courtier effaçait simultanément les baux, la requête en vol et les réponses en attente de lecture. À son réveil, la pile de l’appelant consultait de nouveau le bail déjà révoqué et retournait `DENIED`. Le client perdait ainsi l’information permettant de décider si son opération avait pu s’exécuter.

Le noyau contrôle la capacité à l’entrée de chaque nouvel appel. Sous la même garde de l’ordonnanceur, le retrait du service ferme les baux et fixe le résultat des requêtes qui attendent encore. Un `COMPLETE` enregistré avant le retrait garde sa réponse. Seule la pile qui a soumis l’appel peut consommer le résultat, avec son ticket et l’identité générationnelle du processus. La mort du client libère les requêtes qu’il possède. Les résultats terminés sont exclus de la file de remise et les numéros de ticket continuent de croître après réinstallation du serveur. Un débordement du compteur produit `LIMIT`. [Courtier](../kernel/src/sched/process/tmp.rs) · [Contrat de l’appelant](contrat-tmp-prive-v0.21.0.md#retrait-coupure-et-reprise)

| Frontière observée | Résultat de l’appel admis | Décision |
|---|---|---|
| Arrêt avant `NEXT` | `CLOSED` (11) | Le service n’a pas reçu la requête. |
| `NEXT` réussi, arrêt avant l’effet | `UNCERTAIN` (22) | Le banc observe zéro effet. Le courtier ne dispose d’aucune preuve de non-exécution après la remise. |
| Effet effectué, arrêt avant `COMPLETE` | `UNCERTAIN` (22) | L’ancienne requête ne revient jamais dans la file. |
| `COMPLETE` accepté avant l’arrêt | `OK` (0), valeur 128 et trame exacte, ou statut d’erreur exact, valeur 0 | Le client reçoit une seule fois le résultat enregistré. |
| Nouvel appel sur le bail révoqué | `DENIED` (5) | La collecte de l’ancien résultat ne restaure aucun droit. |

Un acquittement de service a la portée de l’opération appelée. `WRITE` peut préparer des octets, tandis qu’un succès de `SYNC` dépend des barrières du stockage. Une écriture présente sur P9 ne prouve pas à elle seule quel ticket a abouti. En l’absence d’acquittement fiable, le courtier n’essaie pas de reconstruire ce lien après coup. La garantie de collecte concerne le noyau et l’appelant encore vivants. Un redémarrage complet ne restaure pas leurs tickets en RAM. [Sémantique de stockage](contrat-tmp-prive-v0.21.0.md)

## Essais sur les deux architectures

Les essais invités ont utilisé QEMU 11.1.3 TCG. Les SHA-256 des noyaux ordinaires sont `4518c1f86da7d20ad35feabb5c7aaa41dc759c8d110ba391ba3ba55d8a43158a` pour AArch64 et `03401cccdf1bad3e30c5b1084e8ea7a725873d4b1ad6d2cf0728c2973a738bb2` pour x86_64. Les noyaux instrumentés, le dispositif de stockage et les exécutables témoins sont identifiés séparément dans les archives.

| Campagne | Résultat |
|---|---|
| Courtier concurrent, 1, 2 et 4 vCPU par ISA | 6/6 profils, cinq instants d’arrêt par profil. Dans chacun des 30 cas, une deuxième écriture attendait sur le même droit et a obtenu `CLOSED`. Les demandes suivantes sur le droit révoqué ont obtenu `DENIED`. |
| Comptage indépendant dans le service témoin | Zéro exécution avant remise et après remise avant effet. Une exécution après effet, y compris après perte de l’acquittement. L’octet écrit et le compteur ont été lus indépendamment du statut client. Les profils ont conservé les anciens essais d’annulation avant et après remise. |
| Réinstallation dans le même démarrage | Le serveur témoin suivant trouve une file vide. Les anciens tickets sont rejetés au repos et pendant une nouvelle opération. À la fin des scénarios, zéro bail et zéro requête résiduelle. Le banc commande explicitement la réinstallation. |
| Résultat connu avant réveil | Le banc sérialise le vrai gestionnaire `complete` et le retrait du serveur sous la même garde. Le client retrouve l’octet de réponse `0xa7` en cas de succès et le statut `FULL` (23) en cas d’échec. |
| Tampon invalidé après acquittement | Sonde distincte à 2 vCPU par ISA. Le succès connu dont la trame ne peut plus être livrée renvoie `BAD_ADDRESS` (4). L’erreur acquittée reste `FULL`. Dans les deux cas la valeur ABI vaut 0 et les 128 octets sentinelles restent intacts. |
| ABI v1 | 46/46 sélecteurs, huit contrôles réussis, régénération identique. Compilations Rust, C11 et C++11 sur les deux cibles. Les sondes ABI des deux noyaux ordinaires ont réussi. |
| Quatre anciens ELF et SDK | Deux ELF v0.20.0 par ISA, vérifiés par SHA-256 puis exécutés sans recompilation. Exemples SDK sortis avec le code 0 en EL0 et Ring 3. Les témoins de threads ont restitué 62 088 cadres sur AArch64 et 62 589 sur x86_64. L’archive distribuée `nova-system` 0.2.0 reste identique, SHA-256 `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. Trois tests du SDK ont réussi sur Darwin ARM64 natif et trois sous Rosetta. |
| Instances et BootID | 24 familles retirées sur six profils SMP. Les deux ISA ont rejeté l’ancienne identité et accepté la courante après reset normal et après arrêt forcé suivi de relance. Le chemin durable a refusé l’absence de source. Cortex-A72 a utilisé la graine UEFI du profil QEMU, x86_64 a utilisé RDRAND. |

## Ce que le disque P9 a enregistré

Huit essais sur volumes VirtIO bruts jetables ont utilisé le véritable service de fichiers et le moteur P9. Pour chaque couple architecture/secteur (AArch64 ou x86_64, 512 ou 4 096 octets), une copie instrumentée du service a reçu un `SYNC` privé sur un fichier initialement vide. Le service est sorti normalement à l’un de deux points précis. Chaque essai a utilisé un vCPU. Le banc SMP ci-dessus exerce indépendamment les courses du courtier sur 1, 2 et 4 vCPU.

| Arrêt du service après `NEXT` | Observé par le client | Image après arrêt, relue sur l’hôte | Essais |
|---|---|---|---:|
| Avant l’exécution de `SYNC` | `UNCERTAIN`, puis `DENIED` avec l’ancien droit | Génération 51, ancres 50/51, fichier vide. Aucune barrière acquittée pour ce `SYNC`. | 4/4 |
| Après réussite de `SYNC`, avant `COMPLETE` | `UNCERTAIN`, puis `DENIED` avec l’ancien droit | F1 et F2 acquittées, génération 52, ancres 51/52, contenu exact `0x42`. Une seule publication P9. | 4/4 |

Le lecteur hôte a vérifié les ancres, le contenu et le catalogue public, ainsi que l’identité SHA-256 des images avant et après sa lecture. Les huit reprises ont employé le noyau ordinaire du candidat et retrouvé la sentinelle publique après nettoyage des orphelins. Le compteur du témoin RAM mesure les remises et les effets. La comparaison des générations sur disque mesure ici les publications P9. Ces observations ne supposent aucune atomicité des secteurs ni résistance démontrée à une coupure électrique du Mac.

## Admission, révocation et récupération du temporaire

Sur les noyaux ordinaires, la matrice suivante a exercé les secteurs de 512 et 4 096 octets sur les deux architectures. L’archive du stockage réunit 104 démarrages qualifiés en comptant les huit arrêts de service et leurs huit reprises décrits ci-dessus.

| Parcours | Résultat vérifié |
|---|---|
| Temporaire P9 | 48/48 démarrages, avec 1, 2 et 4 vCPU. Les 48 BootID relus sont distincts. La matrice couvre la racine initialement vide, l’isolation des familles, le partage entre threads, la délégation aux enfants, la révocation, les 12 arrêts forcés de QEMU et les 12 reprises correspondantes. Le catalogue public a gardé son contenu. |
| CLI sur P9 | 8/8 sessions, quatre avec le lanceur ordinaire, quatre avec le lanceur de qualification. La sentinelle publique subsiste après remontage. |
| Compatibilité P8 | 16/16 démarrages du profil historique. La génération initiale 40 est passée à 43 puis 44 dans les sessions qui écrivent. Les sessions de lecture ont préservé la génération. |
| Refus d’admission | 16/16, avec une empreinte de disque identique avant et après et zéro écriture bloc observée. Les cas couvrent le P8 présenté au lanceur P9, le profil historique présenté au P9, le volume invalide et l’absence de source BootID qualifiée. |
| Récupération P9 et quarantaine sur Darwin | Les 312 injections F1/F2 et les huit préparations interrompues, sur ARM64 natif et x86_64 sous Rosetta, sont des preuves antérieures réutilisées. Les sources du moteur, du service de stockage et du banc de panne sont identiques à celles de cette qualification. [Archive de référence](validation/integration-abi-tmp-v0.21.0-p9.tar.gz) |

Deux captures x86_64 ont montré un entrelacement sur la sortie série partagée par le noyau et `nsh`. En 512 octets/4 vCPU, l’invite précédait les chiffres du BootID sur la même ligne. En 4 096 octets/2 vCPU, elle était elle-même entrecoupée par les chiffres et le banc attendait encore l’invite pour terminer. Le correctif du lecteur hôte `f60019a267724667c7f79fe9b4c7411bd1794b12` accepte exactement 32 chiffres hexadécimaux et, le cas échéant, les cinq octets de l’invite dans leur ordre. Douze contrôles couvrent les deux captures et le rejet des identités ou invites malformées. Le profil interrompu a été rejoué sur une nouvelle image. Les 48 journaux définitifs ont été relus avec le lecteur corrigé. Les journaux bruts et les diagnostics initiaux figurent dans les preuves.

## Archives et portée de la qualification

Chaque pièce ci-dessous a été relue depuis son archive et vérifiée à partir de son manifeste SHA-256. Les nombres de pièces excluent le manifeste lui-même.

| Archive locale | Pièces | SHA-256 de l’archive |
|---|---:|---|
| [Courtier, concurrence et invalidation du tampon](validation/tmp-fin-service-v0.21.0-qemu.tar.gz) | 32 | `db9bad7ea89b91aab6f46be312d284787c46aa06c96535d582a6bc260525c063` |
| [ABI, ELF distribués et SDK](validation/correctif-next-complete-v0.21.0-abi.tar.gz) | 34 | `ac3316aec7489bd99671f8e774ac793099a85ab1516ce953b206a913eb4afba6` |
| [Noyaux ordinaires, instances et redémarrages](validation/tmp-fin-service-v0.21.0-instances.tar.gz) | 19 | `292ae348acd0a615dc44f532abfc2ee7e09e3db3ae21ef169efe562e29921014` |
| [Stockage P9/P8, injections du service et images avant et après reprise](validation/tmp-fin-service-v0.21.0-p9.tar.gz) | 392 | `9f48df6ddac87dc09401c3bbc2ade5edea65fd5247fc543cb59ad6d8c99a0b0e` |

Le périmètre couvre QEMU TCG et des disques VirtIO bruts. Aucun essai de cette campagne n’utilise UTM, un support physique ou une coupure électrique de l’hôte. La réinstallation du serveur est pilotée par le banc. La restauration d’un instantané comprenant la RAM peut rétablir son BootID et ses capacités. Les essais de fraîcheur couvrent le redémarrage avec une source qualifiée. [Hypothèses du BootID](bootid-phase1-v0.21.0.md)

Le registre conserve l’ABI native v1. Les manifestes identifient NovaOS 0.20.0, le SDK 0.2.0 et NovaFS 0.9.13. Le moteur et les formats P8/P9 sont inchangés. Le candidat local est prêt pour la revue MOA/MOE. Une décision distincte reste attendue pour publier le candidat et le fusionner dans `main`.

## Rejouer le banc du courtier

Depuis un worktree de développement, avec le répertoire temporaire prévu par le projet disponible, le banc exerce le client invité, le courtier et le récepteur. Les deux valeurs de `NOVA_ARCH` sélectionnent les exécutables indépendants. [Scénarios du banc](../kernel/src/sched/process/tmp_wait_probe.rs) · [Appels du témoin utilisateur](../user/tmp_wait.rs) · [Contrôles des journaux](../tests/tmp_attentes.py)

```sh
export NOVAOS_TMPDIR=/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp
export TMPDIR="$NOVAOS_TMPDIR/"
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel &&
    NOVA_ARCH="$arch" python3 -B novaOS/tests/tmp_attentes.py || exit 1
done
python3 -B novaOS/tests/abi_registre.py -v
python3 -B distribution/scripts/sdk_rust.py --check
```
