# NovaOS v0.19.0 — Contrat de stockage NVMe de phase 2

Le lanceur expérimental du Q35 choisit explicitement le backend NVMe. L’application de qualification, le service NovaFS, le service bloc et le pilote s’exécutent chacun dans un espace d’adresses utilisateur. L’application utilise les opérations ordinaires MOUNT, OPEN, WRITE, SYNC, READ et CLOSE. La création de volume est propre à l’essai `nvme-rw-init`. Le moteur P9 vérifie chaque bloc du bail avant sa première écriture de formatage. [Client de qualification](../user/storage/fileclient.rs) · [Service de fichiers](../user/novafs/files.rs)

## Transport et étendue du bail

Le service bloc conserve la syntaxe v1 des trames, le contrôle du tag croissant et les règles de négociation des lectures et écritures d’une page. Le service NovaFS et le backend NVMe échangent ces pages par la capacité IPC privée attribuée à leur paire de processus. Le code du backend exige le résultat de la négociation et les droits attendus avant d’ouvrir la vue P9. Le moteur et le format restent ceux de NovaFS v0.9.13. Le dernier bloc de 4 Kio d’un namespace physique de 4 Mio reste hors du bail. La capacité logique du volume P9 atteint 1 023 blocs de 4 Kio pour chacune des deux tailles de secteur. [Trames bloc](../abi/block.rs) · [Admission du service bloc](../user/storage/block_service.rs) · [Adaptateur P9](../user/novafs/device.rs)

| Opération | Autorisation et séquence |
|---|---|
| READ | Le bloc fait enregistrer NSID, LBA et nombre de secteurs. Le pilote demande la soumission du ticket, attend le CQE et renvoie le nombre d’octets. Le bloc collecte la page seulement après validation par le courtier puis l’envoie au service NovaFS. |
| WRITE | Le bloc reçoit la page privée de NovaFS, enregistre l’étendue avec sa capacité WRITE et transmet l’adresse de **sa propre** page à STAGE. Le courtier valide l’adresse dans cet espace et fige les octets une seule fois. Le pilote demande SUBMIT_WRITE avec le ticket et attend la complétion. Les octets ne passent jamais par une page utilisateur mappée au contrôleur. |
| FLUSH | Une capacité FLUSH distincte autorise un ticket sur le namespace. Le pilote demande SUBMIT_FLUSH sans tampon, attend sa complétion et renvoie un résultat de longueur nulle. Le service bloc libère le ticket avant de confirmer la barrière au moteur. |
| Fermeture | Après la fermeture de la session P9, le pilote supprime ses files, demande l’arrêt et attend sa confirmation. Le superviseur conserve la responsabilité de vérifier le retrait effectif du domaine. |

Les capacités d’écriture et de synchronisation appartiennent au bloc. Le pilote détient un canal distinct de gestion du matériel. Ses requêtes de données contiennent l’identifiant du ticket. Le courtier revérifie à la soumission le type d’opération, la génération, l’identité et les droits des titulaires. Les tests du bloc tentent aussi de réutiliser un ticket annulé et de fournir une seconde source pour une même écriture. [Backend NVMe](../user/nvme_rw_block.rs) · [Pilote](../user/nvme_driver.rs) · [Courtier](../kernel/src/sched/process/nvme/broker.rs)

## Acquittement et récupération

Une WRITE achevée acquitte le transfert de sa page. Le moteur P9 écrit les dépendances CoW, attend sa première barrière F1, publie l’ancre puis attend sa deuxième barrière F2. Le marqueur applicatif `NVME P9 sauvegarde confirmee` est émis uniquement après le retour de SYNC. Les sondes de phase 2 peuvent interrompre séparément les écritures, la publication et leurs barrières. Pour les commandes en vol, le banc hôte contrôle la suspension effective du backend. [Ordre P9](../../novaFS/docs/novafs-recovery.md) · [Sondes d’interruption](../user/novafs/qualification.rs)

Le transport conserve le statut du courtier et le service P9 ferme sa vue en cas d’erreur bloc. L’échéance de réception des réponses bloc de ce profil laisse douze secondes au transport pour transmettre l’expiration matérielle de cinq secondes. En cas d’échec de publication, le client conserve le statut de publication incertaine du protocole fichiers.

Le pilote de qualification extrait le CID et le champ de statut du CQE validé, après retrait du bit de phase. Pour une erreur média, le bloc renvoie le code E/S 26 suivi de huit octets : la signature ASCII `NCQ1`, le CID sur 16 bits little-endian et le statut sur 16 bits little-endian (SC, SCT et indicateurs). Le service de fichiers transmet les mêmes huit octets à partir de l’octet 40 de la réponse associée. L’application en dispose jusqu’à sa prochaine requête. Le code fonctionnel reste 26 avant publication ou 22 si la publication est incertaine. Les trois processus tracent séparément le code, le CID et le statut. Un timeout ou un retrait DMA sans CQE ne génère aucun identifiant de complétion. Cette extension ne figure que dans les images de qualification. [Encodage du diagnostic](../user/nvme_diagnostic.rs)

Le RAW est relu après arrêt du processus QEMU et redémarrage dans une nouvelle VM. Le modèle de panne de cette campagne conserve le système hôte alimenté.

Le profil conserve le polling, un seul ticket en vol, une seule paire de files d’E/S et une page de données maximum par commande. Les tests de cette phase n’évaluent pas le débit. [Régression du backend VirtIO classique](validation/nvme-phase2-virtio.json)

## Contrôles du diagnostic

Les deux tests du codec passent en Darwin ARM64 natif et en x86_64 sous Rosetta. Ils vérifient les octets little-endian, la conservation du CID et du statut, les trois traces et le refus des trames tronquées ou mal formées. Le banc x86_64 Q35 vérifie en plus le transfert sur trois processus distincts. Pour les trois injections observées sur secteurs de 512 octets, le statut NVMe est `0x0280`. Les CID sont `0x001a` sur WRITE, `0x0020` sur F1 et `0x0022` sur F2. Lors de l’échec de F2, le service et l’application donnent le code 22 avec le même CID et le même statut matériel. [Tests du codec](../tests/nvme_diagnostic.rs) · [Banc d’intégration](../tests/nvme_phase2.py)

Depuis `novaOS`, le contrôle du codec s’exécute avec les deux cibles Rust Darwin installées.

```sh
mkdir -p build/nvme-init
for arch in aarch64 x86_64
do
    rustup run stable rustc --edition=2024 --test --target "$arch-apple-darwin" \
        tests/nvme_diagnostic.rs -o "build/nvme-init/diagnostic-$arch" &&
        "build/nvme-init/diagnostic-$arch" || exit 1
done
```
