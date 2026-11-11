# Stockage bloc natif — audit préalable à NovaOS v0.10.0

**Décision du 11 novembre 2026.** Arrêt avant implémentation selon le §15 de la mission. Sur la base `66f7b028446fe429da7d1903d1562569c6d16748`, le noyau peut retirer un endpoint à un processus et restituer ses pages. Il ne possède aucun domaine d’adressage de périphérique qui lui permette de retirer les accès DMA avant cette restitution. Accorder le contrôle d’une virtqueue à un pilote Ring 3 ou EL0 dans les configurations actuelles laisserait des accès mémoire hors de l’autorité accordée au processus. Le choix de la frontière de confiance requiert un arbitrage.

Le dépôt était propre sur `main` au début de l’audit. NovaOS reste en v0.9.0 sous MIT et NovaFS en v0.9.12 sous MPL-2.0. [Manifeste NovaOS](../kernel/Cargo.toml) · [Licence MIT](../LICENSE) · [Manifeste NovaFS](../../novaFS/Cargo.toml) · [Licence MPL-2.0](../../novaFS/LICENSE)

## Préconditions vérifiées dans le code

| Domaine | État de la v0.9.0 | Conséquence pour le service bloc |
|---|---|---|
| Processus | Le lanceur charge des images plates RX de 4 Kio maximum, avec DATA et pile privées RW/NX, un PID et des tables propres. Le superviseur exécute déjà deux processus, traite une mort et relance un service. | La chaîne application → NovaFS → service bloc demande une troisième identité et une seconde paire d’endpoints. Le lanceur peut provisionner un groupe. La chaîne à trois n’existe pas dans le service actuel. |
| IPC | Six appels natifs et dix sélecteurs IPC sont implémentés. Un nœud de capacité désigne un endpoint. SEND, RECEIVE, GRANT et MANAGE sont contrôlés par PID et incarnation. GIVE, révocation transitive, fermeture et réveil des attentes sont effectifs. | Ces droits permettent une session interservices. Le droit SEND seul ne distingue pas une opération READ d’une opération WRITE sur un même endpoint. |
| Mémoire | Les copies utilisateur vérifient les segments avant accès. Les pages de `UserSpace` sont des cadres RAM gérés. Sa destruction démape et restitue les cadres. | Aucune API d’épinglage pour E/S en vol, de prêt révocable de page, de traduction IOVA ou de quarantaine sur échec de reset. Des fragments IPC de 128 octets suffisent pour le transport entre processus. |
| Matériel | Le noyau gère le timer sur les deux cibles et le clavier sur Q35. Les accès aux ports x86 et les mappings ARM du GIC et de l’UART sont réservés au superviseur. | Il manque l’attribution exclusive d’une fonction PCI, d’une fenêtre de registres et d’une source d’IRQ à un pilote. Une première file pourrait être scrutée avec un délai borné. Cela résout l’attente de complétion, sans résoudre le confinement DMA. |
| Bloc et durabilité | Q35 démarre sur CD. Sur ARM, le CD dépend de VirtIO SCSI utilisé par le firmware. Le serveur utilisateur accepte INFO et FIN et annonce zéro volume. Le moteur macOS utilise un trait Device de 4 Kio et une synchronisation explicite. | NovaOS ne dispose d’aucun chemin de lecture, d’écriture ou de flush du disque de données. La capacité du firmware à charger l’ISO prend fin avec cette phase du démarrage. |

Sources auditées. [Chargement et fin de processus](../kernel/src/sched/process.rs) · [Droits, nettoyage et lanceur](../kernel/src/sched/process/ipc/mod.rs) · [Descripteurs et copie IPC](../kernel/src/sched/process/ipc/traffic.rs) · [ABI](../abi/native.rs) · [Mappings privés](../kernel/src/memory/user.rs) · [Ports x86](../kernel/src/arch/x86_64/ports.rs) · [Mappings ARM](../kernel/src/arch/aarch64/paging.rs) · [Serveur NovaFS actuel](../user/storage/service.rs) · [Configuration QEMU](../scripts/qemu.sh) · [Contrat Device](../../novaFS/src/lib.rs) · [Adaptateur fichier](../../novaFS/lab/device.rs)

## Arbitrage sur le pilote

**A — VirtIO Block PCI moderne avec domaine DMA, option recommandée.** Les deux QEMU installés acceptent le même modèle de disque et le même modèle `virtio-iommu-pci`. Cela permettrait de conserver les commandes bloc, la géométrie et les erreurs dans le pilote utilisateur. Le noyau porterait l’autorité générique sur les ressources matérielles. Le protocole VirtIO distingue la négociation de `VIRTIO_F_ACCESS_PLATFORM` de la configuration effective des traductions. Le banc a vérifié la présence des deux objets émulés avec le CPU arrêté. La création d’un domaine, sa politique par défaut, son attachement au bon identifiant PCI et ses invalidations restent à développer dans l’invité. [VirtIO 1.2, §§ 2.7, 5.2, 5.13 et 6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

Ordre minimal proposé pour l’option A.

1. Le lanceur identifie la fonction dédiée sur le bus PCI et conserve les opérations sensibles de configuration, de reset et d’activation du bus master. Il remet au pilote une autorité matérielle locale, bornée à cette fonction et à son incarnation. Les clients ordinaires reçoivent seulement leurs endpoints de session. Vérifier aussi les fenêtres MMIO à la granularité de la page et la maîtrise des interruptions MSI avant toute délégation de registres.
2. Attacher un domaine initialement fermé. Y inscrire uniquement les pages du ring et les tampons dédiés, dans le sens d’accès nécessaire. Les pages restent épinglées jusqu’à complétion ou arrêt démontré du périphérique. Le pilote pourra manipuler les IOVA autorisées sans connaître les adresses physiques des autres processus. Activer le périphérique seulement après contrôle de la négociation et de la configuration du domaine.
3. À la révocation ou à la mort du pilote, fermer les admissions et invalider la génération de session. Arrêter la file, obtenir le reset ou le drainage confirmé, retirer les mappings CPU et DMA et attendre les invalidations avant tout recyclage. Sur délai expiré, conserver les pages en quarantaine, déclarer le périphérique indisponible et interdire sa réattribution tant que sa quiescence reste incertaine.
4. Qualifier ces mécanismes, y compris la mort pendant une opération et la conservation de cadres témoins extérieurs au domaine, avant la première écriture applicative. Ajouter ensuite le pilote, le service bloc et la liaison de NovaFS. Le moteur P8 et les politiques de système de fichiers demeurent en espace utilisateur.

**B — Transport VirtIO médié par le noyau.** Le superviseur serait seul propriétaire des registres de configuration et des descripteurs matériels. Il construirait des chaînes bornées à partir de tampons autorisés et copiés, avec métadonnées immuables pendant l’E/S, barrières de publication et de complétion, et la même discipline de reset. Cette option réduit les prérequis IOMMU et augmente le code de transport de confiance. Une simple vérification des descripteurs laissés ensuite inscriptibles au pilote introduirait une course entre contrôle et usage. Le choix entre A et B détermine qui peut programmer le périphérique et qui doit prouver le confinement.

**Alternative sans DMA étudiée.** Le modèle PIIX3 IDE figure dans le QEMU x86_64 installé. Le QEMU AArch64 installé refuse `-device piix3-ide,help`. SDHCI PCI est proposé des deux côtés. Pour le chemin SD ordinaire, le code amont examiné journalise côté hôte des erreurs de lecture et d’écriture et ne fournit pas, dans ces routines, de confirmation de flush exploitable par le client. Il faudrait qualifier la propagation des erreurs et la barrière du modèle exact avant de le retenir. Aucun de ces constats ne justifie d’exposer des ports globaux, l’espace de configuration PCI ou un CD d’amorçage comme interface de données. [Modèle PIIX de QEMU](https://github.com/qemu/qemu/blob/master/hw/ide/piix.c) · [Modèle SD de QEMU](https://github.com/qemu/qemu/blob/master/hw/sd/sd.c)

## Contrat proposé après arbitrage

Le chemin proposé est **application → NovaFS → service bloc → pilote utilisateur → disque de données QEMU**. Le protocole bloc est indépendant du format NovaFS. Seul le pilote reçoit les ressources matérielles. Le lanceur publie séparément le canal du client vers NovaFS et celui de NovaFS vers le service bloc. Le service bloc associe **à l’endpoint** l’identité du périphérique, l’intervalle logique délégué, les droits de lecture, d’écriture et de synchronisation et une génération de session. Chaque client obtient un canal de réponse privé. Les PID, adresses et numéros de session fournis dans les charges utiles ne font pas autorité. Le pilote se voit confier un disque de données dédié. Si plusieurs domaines de confiance doivent partager un disque, l’IOMMU protège la RAM et le service bloc de confiance reste responsable des bornes LBA. Un pilote autorisé sur le disque entier appartient à la base de confiance pour son contenu. Si les bornes LBA doivent aussi résister à une faute de ce pilote, attribuer un backend matériel distinct par plage ou imposer la vérification des commandes dans un composant de confiance indépendant.

| Opération proposée | Validation et réponse |
|---|---|
| INFO | Géométrie logique et physique, taille de la vue déléguée, mode lecture seule, capacité de flush. |
| READ | Droit de lecture et un secteur logique entier dans la vue. Livrer les fragments après complétion du pilote. Sur erreur, le client abandonne toute réponse incomplète. |
| WRITE | Droit d’écriture, secteur dans la vue, longueur exacte et fragments complets avant soumission. Un succès indique que le périphérique a accepté toute la requête. |
| FLUSH | Droit de synchroniser, file ordonnée, écritures antérieures terminées et confirmation positive du périphérique. Une plateforme incapable de confirmer le flush doit refuser l’engagement durable. |
| CLOSE | Retirer les droits de la session, régler le sort de la requête en vol et fermer les endpoints. La fermeture n’est pas une barrière de persistance. |

Premier profil proposé, sans changement des seize sélecteurs ABI. Un échange en vol par session, une file matérielle et un tampon borné à un secteur logique (512 ou 4 096 octets). Un en-tête little-endian de 24 octets peut porter version, opération, phase et drapeaux (4 × u8), longueur du fragment et statut (2 × u16), identifiant de requête (u64), position et longueur totale (2 × u32). Il reste 104 octets par message, soit cinq fragments pour 512 octets et quarante pour 4 096. BEGIN porte l’adresse logique. Les fragments doivent couvrir chaque octet, dans l’ordre, avant COMMIT. Contrôler les additions, conversions de secteurs, limites de vue et de disque avant tout accès. Pour VirtIO Block, l’unité d’adressage du champ matériel reste 512 octets, même si le secteur logique annoncé vaut 4 096. [Limites IPC existantes](ipc-abi.md) · [Format des requêtes VirtIO Block](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

Distinguer capacité absente ou révoquée, droit insuffisant, cible invalide, hors plage, dépassement arithmétique, taille ou alignement invalide, trame invalide, indisponibilité, erreur d’E/S, échec de synchronisation et service interrompu. Une déconnexion avant soumission abandonne le tampon. Après soumission, un timeout, une erreur ou la perte de la réponse peuvent laisser l’issue d’une mutation indéterminée. Une nouvelle session utilise de nouveaux endpoints. Écarter les réponses tardives et proscrire le rejeu automatique des mutations incertaines.

### Domaine de persistance et oracle de reprise

Pour le banc, choisir une image raw ordinaire, `cache=writeback`, `rerror=report,werror=report`, sans surcouche éphémère pour les données. QEMU doit transmettre les flushs au backend (`cache.no-flush=off`). Le pilote doit négocier le support FLUSH, ordonner la soumission et vérifier le statut final. Le domaine de persistance s’arrête à la confirmation rendue par QEMU et le stockage macOS. Une coupure de VM laisse subsister les caches de l’hôte. La future intégration de NovaFS exigera F1 après données et métadonnées, puis publication de l’ancrage et F2 avant confirmation au client. [Modes de cache QEMU](https://www.qemu.org/docs/master/system/invocation.html) · [Contrat de durabilité P1](../../novaFS/docs/novafs-p1-durabilite.md)

Le futur banc devra conserver hors VM le numéro de génération, le motif et la somme du dernier contenu confirmé durable. Sur image jetable, injecter les coupures avant soumission, en vol, après WRITE, pendant FLUSH et après son acquittement. Tuer le processus QEMU, en lancer un nouveau sur **la même image**, puis relire via le service invité et comparer aussi depuis un lecteur hôte indépendant. Vérifier les sentinelles et les secteurs durables non visés. Avant confirmation, qualifier l’ancien contenu, le nouveau contenu et toute écriture partielle observable suivant l’atomicité effectivement documentée. Une déchirure doit être rapportée explicitement. Après confirmation, tout écart au contenu durable attendu fait échouer le test. Injecter séparément les erreurs de lecture, d’écriture et de flush.

## Observations et portée des essais

- Hôte Darwin ARM64, QEMU 11.1.3, TCG, un vCPU, 256 Mio, UEFI/Limine. Une image raw de 4 Mio initialisée côté hôte a été jointe à Q35 et à `virt`. Pour **chaque architecture et chaque géométrie** (512/512 puis 4 096/4 096), deux processus QEMU indépendants ont démarré sur la même image. Huit démarrages au total. QMP a confirmé l’image inscriptible, la taille, les deux propriétés de géométrie, la transmission des flushs configurée et `iommu_platform=false`. Dans les huit invités, NovaFS a répondu `INFO volumes=0 FIN confirmee`. Après chaque arrêt, l’empreinte de l’image entière restait `2b07811057df887086f06a67edc6ebf911de8b6741156e7a2eb1416a4b8b1b2e`.
- Deux essais distincts, CPU arrêté avant le firmware, ont associé `virtio-iommu-pci,boot-bypass=off` et `virtio-blk-pci,iommu_platform=on`. QMP a relu ces deux propriétés sur chaque architecture. Il reste à démontrer l’attachement et l’isolation par un pilote d’IOMMU dans NovaOS.
- **Backend hôte seul.** Pour des transferts de 512 et 4 096 octets à l’offset 8 192, `qemu-io` a écrit un motif, demandé un flush et relu le motif dans un autre processus. Un lecteur Python a vérifié l’image entière, sentinelles comprises. Les trois injections `blkdebug` ont donné un code de sortie 1 et une erreur d’E/S à chaque taille. L’échec de vidage était notamment visible à la fermeture du backend. Ces contrôles qualifient l’oracle hôte. [Injection d’erreurs QEMU](https://www.qemu.org/docs/master/devel/testing/blkdebug.html)
- **Stockage bloc invité.** Les cinq opérations du nouveau service, leurs droits sur les secteurs, les erreurs renvoyées au client, les pannes du pilote, la relecture d’une écriture invitée après redémarrage et la durabilité restent **non validées**. Latences READ/WRITE/FLUSH, débits séquentiel et aléatoire, allocations, pics et fuites propres au futur service bloc restent à mesurer. Le démarrage vérifié, les mises à jour atomiques et le portage du moteur P8 restent hors de ce jalon.

### Régression et mémoire du service existant

Les deux commandes `make test` ont terminé avec le code 0, chacune après 25 démarrages UEFI. Le scénario `storage-test` a revérifié le client sans bootstrap, les droits inversés, les trames invalides, la perte du serveur, la seconde session, la pénurie de cadres et la restitution du Buddy. Les images testées ont les mêmes SHA-256 que dans la qualification v0.9.0. [Référence v0.9.0](novafs-integration-v0.9.0.md)

| Mesure du scénario v0.9.0 dans cet audit | x86_64 | AArch64 |
|---|---:|---:|
| RAM résidente avant lancement et après collecte | 1 212 Kio | 1 140 Kio |
| RAM avec le serveur en attente après collecte du client | 1 336 Kio | 1 264 Kio |
| RAM à la publication des deux processus | 1 428 Kio | 1 356 Kio |
| Pic dans la fenêtre panne, reprise et second client | 1 432 Kio | 1 360 Kio |
| Allocations de cadres dans cette fenêtre | 162 | 161 |
| Blocs Buddy vivants après collecte | 3 | 3 |
| Échantillons d’aller-retour IPC | 513 | 513 |
| Latence IPC p50 / p99 | 30,68 / 53,19 µs | 45,06 / 57,34 µs |
| Commutations directes chronométrées | 1 027 | 1 026 |

Ces mesures concernent INFO et FIN en mémoire. La qualification publiée donnait 49,07 et 53,25 µs au p99, avec des campagnes successives. Les suites de cet audit ont tourné simultanément. Le pic et le bilan final des cadres sont identiques à ceux de la référence.

## Reproduction depuis la racine du dépôt

```sh
make -C novaOS ARCH=x86_64 test
make -C novaOS ARCH=aarch64 test
qemu-system-x86_64 -device virtio-blk-pci,help
qemu-system-aarch64 -device virtio-blk-pci,help
```

Les commandes suivantes contrôlent l’attachement des images via QMP et attendent l’échange série entre les deux processus invités. Les images de test sont exclusivement temporaires.

```sh
python3 - <<'PY'
import hashlib, os, subprocess, sys, tempfile
from pathlib import Path

sys.path.insert(0, str(Path("novaOS/tests").resolve()))
from demarrage import qmp, wait_demo, wait_storage

with tempfile.TemporaryDirectory() as tmp:
    disk = Path(tmp) / "audit.raw"
    disk.write_bytes(bytes(range(256)) * 16384)
    digest = lambda: hashlib.sha256(disk.read_bytes()).hexdigest()
    initial = digest()
    for arch in ("x86_64", "aarch64"):
        for sector in (512, 4096):
            for boot in (1, 2):
                log = Path(tmp) / f"{arch}-{sector}-{boot}.log"
                args = ["sh", "novaOS/scripts/qemu.sh", "-display", "none",
                        "-monitor", "none", "-serial", f"file:{log}", "-qmp", "stdio",
                        "-drive", f"if=none,id=data,file={disk},format=raw,cache=writeback,werror=report,rerror=report",
                        "-device", f"virtio-blk-pci,drive=data,id=audit,disable-legacy=on,logical_block_size={sector},physical_block_size={sector}"]
                with subprocess.Popen(args, env={**os.environ, "NOVA_ARCH":arch},
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0) as vm:
                    try:
                        qmp(vm, "qmp_capabilities")
                        wait_demo(vm, log, "Ring3" if arch=="x86_64" else "EL0")
                        wait_storage(vm, log)
                        node = next(d["inserted"] for d in qmp(vm, "query-block") if d["device"] == "data")
                        assert node["image"]["virtual-size"] == 4*1024**2 and not node["ro"]
                        assert not node["cache"]["no-flush"]
                        for prop in ("logical_block_size", "physical_block_size"):
                            assert qmp(vm, "qom-get", path="/machine/peripheral/audit", property=prop) == sector
                        assert not qmp(vm, "qom-get", path="/machine/peripheral/audit", property="iommu_platform")
                        qmp(vm, "quit")
                        assert vm.wait(5) == 0
                    finally:
                        if vm.poll() is None:
                            vm.kill()
                            vm.wait()
                assert digest() == initial
                print(arch, sector, boot, "INFO volumes=0", initial, flush=True)
PY
```

Pour reproduire le contrôle de topologie IOMMU sur chaque architecture, exécuter depuis la racine le même lanceur avec le processeur arrêté, puis interroger ses propriétés à l’accueil QMP. Ces commandes sont indépendantes de l’image jetable utilisée plus haut.

```sh
for arch in x86_64 aarch64; do
    printf '%s\n' \
        '{"execute":"qmp_capabilities"}' \
        '{"execute":"query-status"}' \
        '{"execute":"qom-get","arguments":{"path":"/machine/peripheral/remap","property":"boot-bypass"}}' \
        '{"execute":"qom-get","arguments":{"path":"/machine/peripheral/disk","property":"iommu_platform"}}' \
        '{"execute":"quit"}' |
        NOVA_ARCH="$arch" sh novaOS/scripts/qemu.sh -S -display none -monitor none -serial none -qmp stdio \
            -device virtio-iommu-pci,id=remap,boot-bypass=off \
            -blockdev driver=null-co,node-name=topologie,size=4194304 \
            -device virtio-blk-pci,drive=topologie,id=disk,disable-legacy=on,iommu_platform=on
done
```

Le backend nul de cette commande sert uniquement à instancier les objets de la topologie CPU arrêté. Pour le contrôle de persistance **hôte**, utiliser des fichiers raw neufs. Chaque invocation suivante de `qemu-io` ouvre l’image dans un processus distinct.

```sh
python3 - <<'PY'
import errno, subprocess, tempfile
from pathlib import Path

def io(image, commands):
    args = ["qemu-io", "-t", "writeback", "-f", "raw"]
    for cmd in commands:
        args += ["-c", cmd]
    return subprocess.run([*args, image], capture_output=True, text=True)

with tempfile.TemporaryDirectory() as tmp:
    for size in (512, 4096):
        disk = Path(tmp) / f"disk-{size}.raw"
        disk.write_bytes(bytes(1024 * 1024))
        assert io(str(disk), [f"write -P 0x5a 8192 {size}", "flush"]).returncode == 0
        assert io(str(disk), [f"read -P 0x5a 8192 {size}"]).returncode == 0
        expected = bytearray(1024 * 1024)
        expected[8192:8192 + size] = bytes([0x5a]) * size
        assert disk.read_bytes() == expected
        for event, commands in [
            ("read_aio", [f"read 8192 {size}"]),
            ("write_aio", [f"write 8192 {size}"]),
            ("flush_to_disk", [f"write -P 0x5a 8192 {size}", "flush"]),
        ]:
            config = Path(tmp) / "fault.conf"
            config.write_text(f'[inject-error]\nevent = "{event}"\nerrno = "{errno.EIO}"\n')
            result = io(f"blkdebug:{config}:{disk}", commands)
            assert result.returncode == 1 and "Input/output error" in result.stdout + result.stderr
            print(size, event, "EIO")
PY
```
