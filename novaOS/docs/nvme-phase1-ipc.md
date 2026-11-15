# Interface privée du pilote NVMe en lecture seule

Le profil expérimental x86_64 Q35 exécute le gestionnaire, le pilote NVMe, le service bloc et le client de qualification dans des processus distincts. Le gestionnaire termine avant la publication des trois acteurs de lecture. Le lanceur utilise les images plates de NovaOS et les appels IPC v1. Les messages ci-dessous sont privés à ce profil. [Gestionnaire](../user/nvme_manager.rs) · [Pilote](../user/nvme_driver.rs) · [Service bloc](../user/nvme_block.rs) · [Client de qualification](../user/nvme_client.rs)

## Chemin d’une lecture

1. Le gestionnaire compare l’identité PCI observée à l’autorisation du lanceur. Il exige la classe PCI NVM Express, le modèle QEMU attendu, le même emplacement dans l’offre et le manifeste, l’image du pilote en lecture seule et ses droits exacts.
2. Le service bloc demande au pilote de démarrer. Le pilote interroge le courtier, demande l’activation, attend l’état prêt, puis séquence les trois Identify, la négociation du nombre de files, la création de la CQ puis de la SQ. Chaque commande est suivie d’une vérification de complétion. Le pilote recoupe le constructeur, la version, la série, le namespace et la géométrie. Le service bloc obtient lui-même l’identité et la géométrie sur son canal avec le courtier.
3. Pour une demande valide, le service bloc enregistre auprès du courtier le NSID, le LBA et le nombre de blocs. Le courtier associe le ticket à cette requête et à l’incarnation du service. Le pilote reçoit uniquement ce ticket lors d’une demande de lecture. Une seule commande d’E/S est admise à la fois.
4. Le pilote demande la soumission et consulte la complétion. Le service bloc attend l’avis du pilote, puis collecte auprès du courtier les octets de la commande effectivement achevée. Il transmet les octets au client sur une liaison dédiée. Une révocation, un échec de complétion ou une échéance dépassée interrompt ce chemin.
5. Lors d’un arrêt normal, le pilote demande la suppression de la SQ, puis de la CQ, l’arrêt du contrôleur et la vérification de sa désactivation. La confirmation de la restitution des cadres relève du superviseur et du remappeur VT-d.

## Canaux et trames

Lancer une autre image, connaître un PID ou deviner la valeur d’un handle ne suffit pas à détenir le droit de publier une commande. Chaque canal interne est unidirectionnel. Le courtier revalide les processus et les capacités avant l’action privilégiée. Les files matérielles, les doorbells et les pages DMA restent accessibles uniquement au superviseur. Le pilote peut refuser de progresser ou se terminer. Il ne peut pas choisir l’adresse d’une page RAM ni remplacer le NSID, la plage ou les octets d’un ticket autorisé. [Câblage par le lanceur](../kernel/src/sched/process/ipc/nvme_port.rs) · [Enregistrement et consommation des tickets](../kernel/src/sched/process/nvme/broker.rs)

| Échange | Format |
|---|---|
| Demande au courtier | Le tag IPC porte l’opération. Le corps de 40 octets contient cinq entiers de 64 bits little-endian, dans l’ordre époque, séquence, arguments A, B, C. Chaque rôle a sa propre séquence strictement croissante. Une seule demande de ce rôle attend sa réponse. |
| Réponse du courtier | Le courtier copie 128 octets dans la page DATA privée du demandeur à `0x401100`, puis publie une notification portant la séquence. Les six premiers mots sont époque, séquence, statut, X, Y, Z. Les 80 octets suivants transportent les informations Identify ou un fragment de lecture autorisé. L’époque, la séquence et la nature de la notification doivent correspondre. |
| Demande entre pairs | Le tag porte la séquence. Le corps de 40 octets contient époque, opération et trois arguments de 64 bits little-endian. Une trame d’une autre incarnation ou d’une autre longueur est refusée. |
| Réponse entre pairs | Le tag reprend la séquence. Le corps commence par époque, statut et trois valeurs de 64 bits. Un fragment contient également au plus 80 octets. Pour READ, les trois valeurs sont longueur totale en octets, décalage et longueur de la tranche. Un statut d’erreur a une longueur de corps de 40 octets. |

Les opérations du courtier sont réparties par titulaire. Le pilote peut demander DISCOVER, ENABLE, READY, les Identify, la création/suppression des files, SUBMIT_READ sur ticket, POLL, SHUTDOWN et STOPPED. Le service bloc utilise GEOMETRY, REGISTER, COLLECT, RELEASE et CANCEL. Le client ne possède aucun canal de courtier. Une commande matérielle peut lire huit LBA de 512 octets ou un LBA de 4 096 octets. Le découpage en tranches IPC de 80 octets concerne seulement le retour au client. [Constantes du protocole](../user/nvme_protocol.rs) · [Vérification des trames](../user/nvme_ipc.rs)

## Mémoire et oracles

Chaque image plate occupe une page de code RX, une page DATA RW et une page de pile utilisateur RW, soit 12 Kio privés. Le groupe pilote/bloc/client occupe 36 Kio de pages utilisateur pendant une session. Le gestionnaire occupe 12 Kio lors de sa décision. Ces programmes n’allouent aucun tas utilisateur. Les tables de pages, piles noyau, objets IPC, pages VT-d et tampons du contrôleur ont une comptabilité distincte. [Attribution des pages du processus](../kernel/src/memory/user.rs)

Le client vérifie chaque octet reçu pour le premier bloc, le dernier bloc et les 4 Kio commençant au milieu du volume témoin de 1 Mio. Il publie également une empreinte FNV-1a 64 bits pour chacune des trois lectures, dans cet ordre. Le banc peut ainsi confronter les octets reçus dans le processus aux octets du disque RAW relu sur l’hôte. FNV sert à ce contrôle reproductible. Les preuves d’intégrité de NovaFS restent celles de son format P9. Aucune commande WRITE ou FLUSH et aucun montage NovaFS ne sont engagés par ces programmes.
